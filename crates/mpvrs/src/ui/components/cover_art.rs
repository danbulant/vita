use std::collections::{HashMap, HashSet};
use std::fs;
use std::fs::File;
use std::hash::{Hash, Hasher};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant, UNIX_EPOCH};

use image::imageops::FilterType;
use image::{DynamicImage, GenericImage, ImageBuffer, Rgba};
use imgui::{Image, TextureId, Ui};

use crate::library::ART_DIR;
use crate::plumbing::rendering::{create_rgba_texture, delete_texture, GlTexture};
use crate::plumbing::threading::{set_current_thread_priority, ARTWORK_THREAD_PRIORITY};

const ART_TEXTURE_BUDGET_BYTES: usize = 8 * 1024 * 1024;
const MAX_SOURCE_PIXELS: u32 = 2048 * 2048;
const COVER_ROUNDING: f32 = 5.0;
const STALE_REQUEST_TICKS: u64 = 96;
const REQUEST_HISTORY_TICKS: u64 = 512;
const MAX_WORK_BATCH: usize = 4;
const LIST_THUMBNAIL_DEBOUNCE_TICKS: u64 = 9;
const SLOW_ART_STEP: Duration = Duration::from_millis(12);
const SLOW_ART_TOTAL: Duration = Duration::from_millis(32);
const SLOW_GL_UPLOAD: Duration = Duration::from_millis(6);
const CACHE_READ_CHUNK_BYTES: usize = 16 * 1024;
const PLAYBACK_ARTWORK_THROTTLE: Duration = Duration::from_millis(3);
const PLAYBACK_ARTWORK_INTER_JOB_DELAY: Duration = Duration::from_millis(80);

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub enum CoverSize {
    Small,
    Large,
}

impl CoverSize {
    pub fn pixels(self) -> u32 {
        match self {
            Self::Small => 48,
            Self::Large => 320,
        }
    }

    fn cache_dir_name(self) -> &'static str {
        match self {
            Self::Small => "48",
            Self::Large => "320",
        }
    }
}

#[derive(Debug)]
struct CachedTexture {
    texture: GlTexture,
    last_used: u64,
}

struct WorkRequest {
    key: String,
    source_path: String,
    cache_path: PathBuf,
    size: CoverSize,
    cache_only: bool,
}

struct WorkResult {
    key: String,
    size: CoverSize,
    outcome: WorkOutcome,
}

enum WorkOutcome {
    Loaded(Vec<u8>),
    Cancelled,
    Failed,
}

enum WorkerMessage {
    Load(WorkRequest),
    Shutdown,
}

pub struct CoverArtCache {
    textures: HashMap<String, CachedTexture>,
    failed: HashSet<String>,
    pending: HashSet<String>,
    requested_at: HashMap<String, u64>,
    visible_since: HashMap<String, u64>,
    request_tx: Sender<WorkerMessage>,
    result_rx: Receiver<WorkResult>,
    playback_active: Arc<AtomicBool>,
    worker: Option<JoinHandle<()>>,
    bytes_used: usize,
    tick: u64,
    budget_bytes: usize,
}

impl CoverArtCache {
    pub fn new() -> Self {
        let (request_tx, request_rx) = mpsc::channel();
        let (result_tx, result_rx) = mpsc::channel();
        let playback_active = Arc::new(AtomicBool::new(false));
        let worker_playback_active = Arc::clone(&playback_active);
        let worker =
            thread::spawn(move || cover_worker(request_rx, result_tx, worker_playback_active));

        Self {
            textures: HashMap::new(),
            failed: HashSet::new(),
            pending: HashSet::new(),
            requested_at: HashMap::new(),
            visible_since: HashMap::new(),
            request_tx,
            result_rx,
            playback_active,
            worker: Some(worker),
            bytes_used: 0,
            tick: 0,
            budget_bytes: ART_TEXTURE_BUDGET_BYTES,
        }
    }

    pub fn set_playback_active(&self, active: bool) {
        self.playback_active.store(active, Ordering::Relaxed);
    }

    pub fn get_or_load(&mut self, source_path: &str, size: CoverSize) -> Option<GlTexture> {
        self.get_or_load_with_policy(source_path, size, true, 0, true)
    }

    pub fn get_or_load_thumbnail(
        &mut self,
        source_path: &str,
        request_now: bool,
    ) -> Option<GlTexture> {
        self.get_or_load_with_policy(
            source_path,
            CoverSize::Small,
            request_now,
            LIST_THUMBNAIL_DEBOUNCE_TICKS,
            true,
        )
    }

    fn get_or_load_with_policy(
        &mut self,
        source_path: &str,
        size: CoverSize,
        request_now: bool,
        debounce_ticks: u64,
        decode_allowed: bool,
    ) -> Option<GlTexture> {
        self.tick = self.tick.wrapping_add(1);
        self.poll_completed();
        self.prune_request_history();

        let key = texture_key(source_path, size);
        self.requested_at.insert(key.clone(), self.tick);
        self.visible_since.entry(key.clone()).or_insert(self.tick);

        if let Some(cached) = self.textures.get_mut(&key) {
            cached.last_used = self.tick;
            return Some(cached.texture);
        }

        if self.failed.contains(&key) || self.pending.contains(&key) {
            return None;
        }

        let visible_for = self
            .visible_since
            .get(&key)
            .map(|first_seen| self.tick.wrapping_sub(*first_seen))
            .unwrap_or(0);
        if !request_now && visible_for < debounce_ticks {
            return None;
        }

        let Some(cache_path) = resized_cache_path(source_path, size) else {
            self.failed.insert(key);
            return None;
        };
        let cache_only = !decode_allowed;
        if cache_only && !cache_path.exists() {
            return None;
        }

        let request = WorkRequest {
            key: key.clone(),
            source_path: source_path.to_owned(),
            cache_path,
            size,
            cache_only,
        };
        if self.request_tx.send(WorkerMessage::Load(request)).is_ok() {
            self.pending.insert(key);
        } else {
            self.failed.insert(key);
        }

        None
    }

    pub fn get_or_load_fallback(&mut self, size: CoverSize) -> Option<GlTexture> {
        self.tick = self.tick.wrapping_add(1);
        self.poll_completed();
        let key = format!("fallback:{}", size.cache_dir_name());

        if let Some(cached) = self.textures.get_mut(&key) {
            cached.last_used = self.tick;
            return Some(cached.texture);
        }

        if self.failed.contains(&key) {
            return None;
        }

        let target = size.pixels();
        let expected_len = target as usize * target as usize * 4;
        let pixels = match fs::read(fallback_asset_path(size)) {
            Ok(pixels) if pixels.len() == expected_len => pixels,
            _ => {
                self.failed.insert(key);
                return None;
            }
        };
        let Some(texture) = create_rgba_texture(target, target, &pixels) else {
            self.failed.insert(key);
            return None;
        };

        if texture.bytes > self.budget_bytes {
            delete_texture(texture.gl_id);
            self.failed.insert(key);
            return None;
        }

        self.evict_until_fits(texture.bytes);
        self.bytes_used = self.bytes_used.saturating_add(texture.bytes);
        self.textures.insert(
            key,
            CachedTexture {
                texture,
                last_used: self.tick,
            },
        );
        Some(texture)
    }

    fn poll_completed(&mut self) {
        while let Ok(result) = self.result_rx.try_recv() {
            self.pending.remove(&result.key);

            let last_requested = self.requested_at.get(&result.key).copied();
            let recently_requested = last_requested
                .map(|tick| self.tick.wrapping_sub(tick) <= STALE_REQUEST_TICKS)
                .unwrap_or(false);

            let pixels = match result.outcome {
                WorkOutcome::Loaded(pixels) => pixels,
                WorkOutcome::Cancelled => continue,
                WorkOutcome::Failed => {
                    self.failed.insert(result.key);
                    continue;
                }
            };

            if !recently_requested {
                self.requested_at.remove(&result.key);
                continue;
            }

            let target = result.size.pixels();
            let expected_len = target as usize * target as usize * 4;
            if pixels.len() != expected_len {
                self.failed.insert(result.key);
                continue;
            }

            let upload_started = Instant::now();
            let Some(texture) = create_rgba_texture(target, target, &pixels) else {
                self.failed.insert(result.key);
                continue;
            };
            let upload_elapsed = upload_started.elapsed();
            if upload_elapsed >= SLOW_GL_UPLOAD {
                eprintln!(
                    "mpvrs: cover upload slow key={} size={} elapsed_ms={}",
                    result.key,
                    target,
                    upload_elapsed.as_millis()
                );
            }

            if texture.bytes > self.budget_bytes {
                delete_texture(texture.gl_id);
                self.failed.insert(result.key);
                continue;
            }

            self.evict_until_fits(texture.bytes);
            self.bytes_used = self.bytes_used.saturating_add(texture.bytes);
            self.textures.insert(
                result.key,
                CachedTexture {
                    texture,
                    last_used: self.tick,
                },
            );
        }
    }

    fn prune_request_history(&mut self) {
        let tick = self.tick;
        self.requested_at
            .retain(|_, requested| tick.wrapping_sub(*requested) <= REQUEST_HISTORY_TICKS);
        self.visible_since
            .retain(|key, _| self.requested_at.contains_key(key));
    }

    fn evict_until_fits(&mut self, bytes_needed: usize) {
        while self.bytes_used.saturating_add(bytes_needed) > self.budget_bytes {
            let Some(key) = self
                .textures
                .iter()
                .min_by_key(|(_, texture)| texture.last_used)
                .map(|(key, _)| key.clone())
            else {
                break;
            };

            if let Some(texture) = self.textures.remove(&key) {
                self.bytes_used = self.bytes_used.saturating_sub(texture.texture.bytes);
                delete_texture(texture.texture.gl_id);
            }
        }
    }
}

impl Drop for CoverArtCache {
    fn drop(&mut self) {
        let _ = self.request_tx.send(WorkerMessage::Shutdown);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
        for (_, texture) in self.textures.drain() {
            delete_texture(texture.texture.gl_id);
        }
    }
}

fn cover_worker(
    request_rx: Receiver<WorkerMessage>,
    result_tx: Sender<WorkResult>,
    playback_active: Arc<AtomicBool>,
) {
    set_current_thread_priority("artwork", ARTWORK_THREAD_PRIORITY);

    while let Ok(message) = request_rx.recv() {
        let WorkerMessage::Load(first_request) = message else {
            break;
        };

        let mut requests = vec![first_request];
        while let Ok(message) = request_rx.try_recv() {
            match message {
                WorkerMessage::Load(request) => requests.push(request),
                WorkerMessage::Shutdown => return,
            }
        }

        if requests.len() > MAX_WORK_BATCH {
            let discard_count = requests.len() - MAX_WORK_BATCH;
            let kept = requests.split_off(discard_count);
            for request in requests {
                let _ = result_tx.send(WorkResult {
                    key: request.key,
                    size: request.size,
                    outcome: WorkOutcome::Cancelled,
                });
            }
            requests = kept;
        }

        requests.sort_by_key(|request| match request.size {
            CoverSize::Large => 0,
            CoverSize::Small => 1,
        });

        for request in requests {
            thread::yield_now();
            let playback_was_active = playback_active.load(Ordering::Relaxed);
            let outcome = match load_or_resize_rgba(
                &request.key,
                &request.source_path,
                request.size,
                &request.cache_path,
                request.cache_only,
                &playback_active,
            ) {
                Ok(pixels) => WorkOutcome::Loaded(pixels),
                Err(_) => WorkOutcome::Failed,
            };
            let _ = result_tx.send(WorkResult {
                key: request.key,
                size: request.size,
                outcome,
            });
            if playback_was_active || playback_active.load(Ordering::Relaxed) {
                thread::sleep(PLAYBACK_ARTWORK_INTER_JOB_DELAY);
            }
        }
    }
}

pub fn draw_cover_art(
    ui: &Ui,
    cache: &mut CoverArtCache,
    art_path: Option<&str>,
    size: CoverSize,
    area: [f32; 2],
) {
    let cursor = ui.cursor_screen_pos();
    let texture = art_path
        .and_then(|path| cache.get_or_load(path, size))
        .or_else(|| cache.get_or_load_fallback(size));
    if let Some(texture) = texture {
        Image::new(texture.texture_id, area).build(ui);
    } else {
        ui.dummy(area);
        draw_cover_fallback(ui, cursor, area);
    }
}

pub fn draw_thumbnail_cover_art_at(
    ui: &Ui,
    cache: &mut CoverArtCache,
    art_path: Option<&str>,
    request_now: bool,
    pos: [f32; 2],
    area: [f32; 2],
) {
    let texture = art_path
        .and_then(|path| cache.get_or_load_thumbnail(path, request_now))
        .or_else(|| cache.get_or_load_fallback(CoverSize::Small));
    draw_cover_art_texture_or_fallback(ui, texture, pos, area);
}

fn draw_cover_art_texture_or_fallback(
    ui: &Ui,
    texture: Option<GlTexture>,
    pos: [f32; 2],
    area: [f32; 2],
) {
    if let Some(texture) = texture {
        ui.get_window_draw_list()
            .add_image(
                texture.texture_id,
                pos,
                [pos[0] + area[0], pos[1] + area[1]],
            )
            .build();
    } else {
        draw_cover_fallback(ui, pos, area);
    }
}

fn draw_cover_fallback(ui: &Ui, pos: [f32; 2], area: [f32; 2]) {
    let draw_list = ui.get_window_draw_list();
    let x = pos[0];
    let y = pos[1];
    let w = area[0];
    let h = area[1];
    let min = [x, y];
    let max = [x + w, y + h];
    let bg = [0.12, 0.15, 0.20, 1.0];
    let border = [0.34, 0.40, 0.50, 1.0];
    let accent = [0.42, 0.72, 1.0, 1.0];

    draw_list
        .add_rect(min, max, bg)
        .rounding(COVER_ROUNDING)
        .filled(true)
        .build();
    draw_list
        .add_rect(min, max, border)
        .rounding(COVER_ROUNDING)
        .thickness(1.0)
        .build();

    let s = w.min(h);
    let cx = x + w * 0.5;
    let cy = y + h * 0.5;
    draw_list
        .add_circle([cx - s * 0.10, cy + s * 0.08], s * 0.14, accent)
        .filled(true)
        .build();
    draw_list
        .add_line(
            [cx + s * 0.04, cy - s * 0.22],
            [cx + s * 0.04, cy + s * 0.10],
            accent,
        )
        .thickness((s * 0.07).max(2.0))
        .build();
    draw_list
        .add_line(
            [cx + s * 0.04, cy - s * 0.22],
            [cx + s * 0.26, cy - s * 0.12],
            accent,
        )
        .thickness((s * 0.07).max(2.0))
        .build();
}

fn fallback_asset_path(size: CoverSize) -> &'static str {
    match size {
        CoverSize::Small => "app0:/icons/cover_fallback_48.rgba",
        CoverSize::Large => "app0:/icons/cover_fallback_320.rgba",
    }
}

fn texture_key(source_path: &str, size: CoverSize) -> String {
    format!("{}:{}", size.cache_dir_name(), source_path)
}

fn resized_cache_path(source_path: &str, size: CoverSize) -> Option<PathBuf> {
    let signature = source_signature(source_path)?;
    let hash = hash_cache_key(source_path, signature, size);
    let dir = Path::new(ART_DIR)
        .join("resized")
        .join(size.cache_dir_name());
    Some(dir.join(format!("{hash}.rgba")))
}

fn source_signature(source_path: &str) -> Option<(u64, u64)> {
    let stat = fs::metadata(source_path).ok()?;
    let modified_at = stat
        .modified()
        .ok()
        .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
        .map(|duration| duration.as_secs())
        .unwrap_or(0);
    Some((stat.len(), modified_at))
}

fn load_or_resize_rgba(
    key: &str,
    source_path: &str,
    size: CoverSize,
    cache_path: &Path,
    cache_only: bool,
    playback_active: &AtomicBool,
) -> Result<Vec<u8>, String> {
    let total_started = Instant::now();
    let target = size.pixels();
    let expected_len = target as usize * target as usize * 4;

    if cache_path.exists() {
        let started = Instant::now();
        let pixels =
            read_file_chunked(cache_path, playback_active).map_err(|err| err.to_string())?;
        log_slow_art_step(key, "cache_read", target, started.elapsed());
        throttle_artwork_if_playing(playback_active);
        if pixels.len() == expected_len {
            log_slow_art_total(key, "cache", target, total_started.elapsed());
            return Ok(pixels);
        }
    }

    if cache_only {
        return Err("cached cover is unavailable".to_owned());
    }

    let started = Instant::now();
    let reader = image::ImageReader::open(source_path).map_err(|err| err.to_string())?;
    let reader = reader
        .with_guessed_format()
        .map_err(|err| err.to_string())?;
    log_slow_art_step(key, "open_guess", target, started.elapsed());
    throttle_artwork_if_playing(playback_active);

    let started = Instant::now();
    let dimensions = reader.into_dimensions().map_err(|err| err.to_string())?;
    log_slow_art_step(key, "dimensions", target, started.elapsed());
    throttle_artwork_if_playing(playback_active);
    if dimensions.0.saturating_mul(dimensions.1) > MAX_SOURCE_PIXELS {
        return Err("source image is too large".to_owned());
    }

    let started = Instant::now();
    let image = image::open(source_path).map_err(|err| err.to_string())?;
    log_slow_art_step(key, "decode", target, started.elapsed());
    throttle_artwork_if_playing(playback_active);

    let started = Instant::now();
    let resized = resize_square(image, target);
    log_slow_art_step(key, "resize", target, started.elapsed());
    throttle_artwork_if_playing(playback_active);

    let pixels = resized.into_raw();

    if let Some(parent) = cache_path.parent() {
        fs::create_dir_all(parent).map_err(|err| err.to_string())?;
    }
    let started = Instant::now();
    fs::write(cache_path, &pixels).map_err(|err| err.to_string())?;
    log_slow_art_step(key, "cache_write", target, started.elapsed());
    throttle_artwork_if_playing(playback_active);

    log_slow_art_total(key, "decode_resize", target, total_started.elapsed());
    Ok(pixels)
}

fn read_file_chunked(path: &Path, playback_active: &AtomicBool) -> std::io::Result<Vec<u8>> {
    let mut file = File::open(path)?;
    let capacity = file
        .metadata()
        .ok()
        .and_then(|metadata| metadata.len().try_into().ok())
        .unwrap_or(0);
    let mut output = Vec::with_capacity(capacity);
    let mut chunk = [0_u8; CACHE_READ_CHUNK_BYTES];

    loop {
        let read = file.read(&mut chunk)?;
        if read == 0 {
            break;
        }
        output.extend_from_slice(&chunk[..read]);
        throttle_artwork_if_playing(playback_active);
    }

    Ok(output)
}

fn throttle_artwork_if_playing(playback_active: &AtomicBool) {
    thread::yield_now();
    if playback_active.load(Ordering::Relaxed) {
        thread::sleep(PLAYBACK_ARTWORK_THROTTLE);
    }
}

fn log_slow_art_step(key: &str, step: &str, size: u32, elapsed: Duration) {
    if elapsed >= SLOW_ART_STEP {
        eprintln!(
            "mpvrs: cover worker slow key={key} size={size} step={step} elapsed_ms={}",
            elapsed.as_millis()
        );
    }
}

fn log_slow_art_total(key: &str, mode: &str, size: u32, elapsed: Duration) {
    if elapsed >= SLOW_ART_TOTAL {
        eprintln!(
            "mpvrs: cover worker total key={key} size={size} mode={mode} elapsed_ms={}",
            elapsed.as_millis()
        );
    }
}

fn resize_square(image: DynamicImage, target: u32) -> ImageBuffer<Rgba<u8>, Vec<u8>> {
    let rgba = image.into_rgba8();
    let (width, height) = rgba.dimensions();
    if width == 0 || height == 0 {
        return ImageBuffer::from_pixel(target, target, Rgba([0, 0, 0, 0]));
    }

    let scale = (target as f32 / width as f32).min(target as f32 / height as f32);
    let resized_width = ((width as f32 * scale).round() as u32).max(1);
    let resized_height = ((height as f32 * scale).round() as u32).max(1);
    let resized =
        image::imageops::resize(&rgba, resized_width, resized_height, FilterType::Triangle);

    let mut canvas = ImageBuffer::from_pixel(target, target, Rgba([18, 22, 30, 255]));
    let x = (target - resized_width) / 2;
    let y = (target - resized_height) / 2;
    let _ = canvas.copy_from(&resized, x, y);
    canvas
}

fn hash_cache_key(source_path: &str, signature: (u64, u64), size: CoverSize) -> String {
    let mut hasher = StableHasher::default();
    source_path.hash(&mut hasher);
    signature.0.hash(&mut hasher);
    signature.1.hash(&mut hasher);
    size.hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}

#[derive(Default)]
struct StableHasher(u64);

impl Hasher for StableHasher {
    fn write(&mut self, bytes: &[u8]) {
        let mut hash = if self.0 == 0 {
            0xcbf29ce484222325
        } else {
            self.0
        };
        for byte in bytes {
            hash ^= u64::from(*byte);
            hash = hash.wrapping_mul(0x100000001b3);
        }
        self.0 = hash;
    }

    fn finish(&self) -> u64 {
        self.0
    }
}

#[allow(dead_code)]
fn _keep_texture_id_sendable(_: TextureId) {}
