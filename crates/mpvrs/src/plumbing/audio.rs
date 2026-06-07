use std::fs::File;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};

use crate::plumbing::threading::{set_current_thread_priority, AUDIO_THREAD_PRIORITY};
use symphonia::core::audio::SampleBuffer;
use symphonia::core::codecs::{Decoder as SymphoniaCodecDecoder, DecoderOptions, CODEC_TYPE_NULL};
use symphonia::core::errors::Error as SymphoniaError;
use symphonia::core::formats::{FormatOptions, FormatReader, SeekMode, SeekTo};
use symphonia::core::io::{MediaSourceStream, MediaSourceStreamOptions};
use symphonia::core::meta::MetadataOptions;
use symphonia::core::probe::Hint;
use symphonia::core::units::Time;
use symphonia::default::{get_codecs, get_probe};

use vitasdk_sys::{
    sceAppMgrAcquireBgmPort, sceAppMgrReleaseBgmPort, sceAudioOutOpenPort, sceAudioOutOutput,
    sceAudioOutReleasePort, sceAudioOutSetVolume, sceKernelPowerLock, sceKernelPowerTick,
    sceKernelPowerUnlock, SCE_AUDIO_OUT_MODE_MONO, SCE_AUDIO_OUT_MODE_STEREO,
    SCE_AUDIO_OUT_PORT_TYPE_BGM, SCE_AUDIO_VOLUME_FLAG_L_CH, SCE_AUDIO_VOLUME_FLAG_R_CH,
    SCE_KERNEL_POWER_TICK_DISABLE_AUTO_SUSPEND, SCE_KERNEL_POWER_TICK_DISABLE_OLED_OFF,
};

const AUDIO_GRAIN: usize = 2048;
const VITA_VOLUME_MAX: i32 = 0x8000;
const PCM_GAIN_Q15: i32 = 28_672;

fn clean_display_part(value: &str) -> Option<String> {
    let value = value.trim().trim_matches('\0').trim();
    if value.is_empty() {
        None
    } else {
        Some(value.to_owned())
    }
}

fn is_unknown_artist(value: &str) -> bool {
    value.eq_ignore_ascii_case("unknown artist")
}

#[derive(Clone, Debug)]
pub struct PlaybackMetadata {
    pub title: String,
    pub artist: Option<String>,
    pub album: Option<String>,
    pub art_path: Option<String>,
}

impl PlaybackMetadata {
    pub fn new(
        title: String,
        artist: Option<String>,
        album: Option<String>,
        art_path: Option<String>,
    ) -> Self {
        Self {
            title: clean_display_part(&title).unwrap_or(title),
            artist: artist.and_then(|value| clean_display_part(&value)),
            album: album.and_then(|value| clean_display_part(&value)),
            art_path: art_path.and_then(|value| clean_display_part(&value)),
        }
    }

    pub fn from_path(path: &str) -> Self {
        let title = Path::new(path)
            .file_stem()
            .or_else(|| Path::new(path).file_name())
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| path.to_owned());
        Self::new(title, None, None, None)
    }

    pub fn display_title(&self) -> String {
        match self.artist.as_deref() {
            Some(artist) if !is_unknown_artist(artist) => format!("{artist} - {}", self.title),
            _ => self.title.clone(),
        }
    }
}

#[derive(Clone, Debug)]
pub struct PlaybackSnapshot {
    pub path: String,
    pub name: String,
    pub metadata: PlaybackMetadata,
    pub is_playing: bool,
    pub position_seconds: f32,
    pub duration_seconds: Option<f32>,
    pub status: String,
}

#[derive(Clone, Debug)]
struct SharedState {
    path: String,
    metadata: PlaybackMetadata,
    is_playing: bool,
    position_frames: u64,
    total_frames: Option<u64>,
    sample_rate: u32,
    status: String,
}

impl SharedState {
    fn snapshot(&self) -> PlaybackSnapshot {
        let sample_rate = self.sample_rate.max(1) as f32;
        PlaybackSnapshot {
            path: self.path.clone(),
            name: self.metadata.display_title(),
            metadata: self.metadata.clone(),
            is_playing: self.is_playing,
            position_seconds: self.position_frames as f32 / sample_rate,
            duration_seconds: self.total_frames.map(|frames| frames as f32 / sample_rate),
            status: self.status.clone(),
        }
    }
}

#[derive(Debug, Default)]
struct Commands {
    seek_to: Option<f32>,
    seek_generation: u64,
}

pub struct AudioPlayer {
    state: Arc<Mutex<SharedState>>,
    commands: Arc<Mutex<Commands>>,
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl AudioPlayer {
    pub fn open_with_metadata(path: String, metadata: PlaybackMetadata) -> Result<Self, String> {
        if !is_supported_audio_file(&path) {
            return Err("Unsupported audio file".to_owned());
        }

        let mut decoder = Decoder::open(&path)?;
        let info = decoder.info();

        let state = Arc::new(Mutex::new(SharedState {
            path: path.clone(),
            metadata,
            is_playing: true,
            position_frames: 0,
            total_frames: info.total_frames,
            sample_rate: info.sample_rate,
            status: "Playing".to_owned(),
        }));
        let commands = Arc::new(Mutex::new(Commands::default()));
        let stop = Arc::new(AtomicBool::new(false));

        let thread_state = Arc::clone(&state);
        let thread_commands = Arc::clone(&commands);
        let thread_stop = Arc::clone(&stop);
        let thread_path = path.clone();

        let handle = thread::spawn(move || {
            run_audio_thread(
                thread_path,
                &mut decoder,
                thread_state,
                thread_commands,
                thread_stop,
            );
        });

        Ok(Self {
            state,
            commands,
            stop,
            thread: Some(handle),
        })
    }

    pub fn snapshot(&self) -> PlaybackSnapshot {
        self.state.lock().unwrap().snapshot()
    }

    pub fn toggle_play_pause(&self) {
        let mut state = self.state.lock().unwrap();
        state.is_playing = !state.is_playing;
        state.status = if state.is_playing {
            "Playing"
        } else {
            "Paused"
        }
        .to_owned();
    }

    pub fn seek_percent(&self, percent: f32) {
        let percent = percent.clamp(0.0, 1.0);
        let mut commands = self.commands.lock().unwrap();
        commands.seek_to = Some(percent);
        commands.seek_generation = commands.seek_generation.wrapping_add(1);
    }

    pub fn seek_relative_seconds(&self, seconds: f32) {
        let snapshot = self.snapshot();
        let Some(duration) = snapshot.duration_seconds else {
            return;
        };
        if duration <= 0.0 {
            return;
        }

        let target = (snapshot.position_seconds + seconds).clamp(0.0, duration);
        self.seek_percent(target / duration);
    }
}

impl Drop for AudioPlayer {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(handle) = self.thread.take() {
            let _ = handle.join();
        }
    }
}

struct PowerInhibit {
    enabled: bool,
}

impl PowerInhibit {
    fn new() -> Self {
        Self { enabled: false }
    }

    fn set_enabled(&mut self, enabled: bool) {
        if self.enabled == enabled {
            return;
        }

        unsafe {
            if enabled {
                sceKernelPowerLock(SCE_KERNEL_POWER_TICK_DISABLE_AUTO_SUSPEND);
                sceKernelPowerLock(SCE_KERNEL_POWER_TICK_DISABLE_OLED_OFF);
            } else {
                sceKernelPowerUnlock(SCE_KERNEL_POWER_TICK_DISABLE_OLED_OFF);
                sceKernelPowerUnlock(SCE_KERNEL_POWER_TICK_DISABLE_AUTO_SUSPEND);
            }
        }

        self.enabled = enabled;
    }

    fn tick(&self) {
        if !self.enabled {
            return;
        }

        unsafe {
            sceKernelPowerTick(SCE_KERNEL_POWER_TICK_DISABLE_AUTO_SUSPEND);
            sceKernelPowerTick(SCE_KERNEL_POWER_TICK_DISABLE_OLED_OFF);
        }
    }
}

impl Drop for PowerInhibit {
    fn drop(&mut self) {
        self.set_enabled(false);
    }
}

pub fn is_supported_audio_file(path: &str) -> bool {
    matches!(
        Path::new(path)
            .extension()
            .and_then(|ext| ext.to_str())
            .map(|ext| ext.to_ascii_lowercase())
            .as_deref(),
        Some(
            "aac"
                | "aif"
                | "aiff"
                | "alac"
                | "caf"
                | "flac"
                | "m4a"
                | "m4b"
                | "m4p"
                | "mka"
                | "mkv"
                | "mp1"
                | "mp2"
                | "mp3"
                | "mp4"
                | "mpa"
                | "oga"
                | "ogg"
                | "wav"
                | "webm",
        )
    )
}

fn run_audio_thread(
    path: String,
    decoder: &mut Decoder,
    state: Arc<Mutex<SharedState>>,
    commands: Arc<Mutex<Commands>>,
    stop: Arc<AtomicBool>,
) {
    set_current_thread_priority("audio", AUDIO_THREAD_PRIORITY);

    unsafe {
        sceAppMgrAcquireBgmPort();
    }

    let info = decoder.info();
    let mode = if info.channels == 1 {
        SCE_AUDIO_OUT_MODE_MONO
    } else {
        SCE_AUDIO_OUT_MODE_STEREO
    };
    let port = unsafe {
        sceAudioOutOpenPort(
            SCE_AUDIO_OUT_PORT_TYPE_BGM,
            AUDIO_GRAIN as i32,
            info.sample_rate as i32,
            mode,
        )
    };

    if port < 0 {
        state.lock().unwrap().status = format!("Failed to open BGM audio port: {port}");
        unsafe {
            sceAppMgrReleaseBgmPort();
        }
        return;
    }

    let mut volume = [VITA_VOLUME_MAX, VITA_VOLUME_MAX];
    unsafe {
        sceAudioOutSetVolume(
            port,
            SCE_AUDIO_VOLUME_FLAG_L_CH | SCE_AUDIO_VOLUME_FLAG_R_CH,
            volume.as_mut_ptr(),
        );
    }

    let mut power_inhibit = PowerInhibit::new();

    let output_channels = info.channels.max(1).min(2) as usize;
    let mut buffer = vec![0_i16; AUDIO_GRAIN * output_channels];

    while !stop.load(Ordering::SeqCst) {
        let is_playing = state.lock().unwrap().is_playing;
        power_inhibit.set_enabled(is_playing);
        power_inhibit.tick();

        let seek_to = {
            let mut commands = commands.lock().unwrap();
            commands
                .seek_to
                .take()
                .map(|percent| (percent, commands.seek_generation))
        };

        if let Some((percent, generation)) = seek_to {
            eprintln!("mpvrs: seek start percent={percent:.3} generation={generation}");
            state.lock().unwrap().status = "Seeking...".to_owned();
            let seek_started = std::time::Instant::now();

            let mut should_cancel = || {
                commands
                    .lock()
                    .map(|commands| commands.seek_generation != generation)
                    .unwrap_or(false)
            };

            match decoder.seek_percent(&path, percent, &mut should_cancel) {
                Ok(SeekOutcome::Complete(position_frames)) => {
                    let elapsed = seek_started.elapsed();
                    eprintln!(
                        "mpvrs: seek decoder complete percent={percent:.3} generation={generation} frames={position_frames} elapsed_ms={}",
                        elapsed.as_millis()
                    );
                    let mut state = state.lock().unwrap();
                    state.position_frames = position_frames;
                    state.status = if state.is_playing {
                        "Playing"
                    } else {
                        "Paused"
                    }
                    .to_owned();
                }
                Ok(SeekOutcome::Cancelled) => {
                    let elapsed = seek_started.elapsed();
                    eprintln!(
                        "mpvrs: seek cancelled percent={percent:.3} generation={generation} elapsed_ms={}",
                        elapsed.as_millis()
                    );
                }
                Err(err) => {
                    eprintln!(
                        "mpvrs: seek failed percent={percent:.3} generation={generation}: {err}"
                    );
                    state.lock().unwrap().status = format!("Seek failed: {err}");
                }
            }
        }

        let is_playing = state.lock().unwrap().is_playing;
        if is_playing {
            match decoder.fill_frames(&mut buffer, AUDIO_GRAIN) {
                Ok(frames_read) => {
                    if frames_read < AUDIO_GRAIN {
                        buffer[frames_read * output_channels..].fill(0);
                        let mut state = state.lock().unwrap();
                        state.is_playing = false;
                        state.status = "Finished".to_owned();
                        power_inhibit.set_enabled(false);
                    }
                    state.lock().unwrap().position_frames = decoder.position_frames();
                }
                Err(err) => {
                    buffer.fill(0);
                    let mut state = state.lock().unwrap();
                    state.is_playing = false;
                    state.status = format!("Decode failed: {err}");
                    power_inhibit.set_enabled(false);
                }
            }
        } else {
            buffer.fill(0);
        }

        unsafe {
            sceAudioOutOutput(port, buffer.as_ptr().cast());
        }
    }

    power_inhibit.set_enabled(false);

    unsafe {
        sceAudioOutReleasePort(port);
        sceAppMgrReleaseBgmPort();
    }
}

#[derive(Clone, Copy, Debug)]
struct StreamInfo {
    sample_rate: u32,
    channels: u32,
    total_frames: Option<u64>,
}

enum SeekOutcome {
    Complete(u64),
    Cancelled,
}

struct Decoder {
    format: Box<dyn FormatReader>,
    decoder: Box<dyn SymphoniaCodecDecoder>,
    track_id: u32,
    current: Vec<i16>,
    current_channels: usize,
    current_offset: usize,
    info: StreamInfo,
    position_frames: u64,
}

impl Decoder {
    fn open(path: &str) -> Result<Self, String> {
        let file = File::open(path).map_err(|err| err.to_string())?;
        let mss = MediaSourceStream::new(Box::new(file), MediaSourceStreamOptions::default());

        let mut hint = Hint::new();
        if let Some(ext) = Path::new(path).extension().and_then(|ext| ext.to_str()) {
            hint.with_extension(ext);
        }

        let format_options = FormatOptions {
            // Keep startup quick while allowing Symphonia to build denser seek indexes as it reads.
            prebuild_seek_index: false,
            seek_index_fill_rate: 2,
            enable_gapless: true,
        };
        let metadata_options = MetadataOptions::default();
        let probed = get_probe()
            .format(&hint, mss, &format_options, &metadata_options)
            .map_err(|err| err.to_string())?;
        let format = probed.format;

        let track = format
            .default_track()
            .or_else(|| {
                format
                    .tracks()
                    .iter()
                    .find(|track| track.codec_params.codec != CODEC_TYPE_NULL)
            })
            .ok_or_else(|| "No supported audio track found".to_owned())?;

        if track.codec_params.codec == CODEC_TYPE_NULL {
            return Err("No supported audio track found".to_owned());
        }

        let track_id = track.id;
        let codec_params = track.codec_params.clone();
        let decoder = get_codecs()
            .make(&codec_params, &DecoderOptions::default())
            .map_err(|err| err.to_string())?;

        let sample_rate = codec_params.sample_rate.unwrap_or(44_100).max(1);
        let channels = codec_params
            .channels
            .map(|channels| channels.count() as u32)
            .unwrap_or(2)
            .max(1)
            .min(2);

        let mut decoder = Self {
            format,
            decoder,
            track_id,
            current: Vec::new(),
            current_channels: channels as usize,
            current_offset: 0,
            info: StreamInfo {
                sample_rate,
                channels,
                total_frames: codec_params.n_frames,
            },
            position_frames: 0,
        };

        decoder.prime()?;
        Ok(decoder)
    }

    fn info(&self) -> StreamInfo {
        self.info
    }

    fn position_frames(&self) -> u64 {
        self.position_frames
    }

    fn fill_frames(&mut self, out: &mut [i16], frames: usize) -> Result<usize, String> {
        let output_channels = self.info.channels as usize;
        let mut written_frames = 0;

        while written_frames < frames {
            if self.current_offset >= self.current.len() / self.current_channels {
                if !self.decode_next_chunk()? {
                    break;
                }
            }

            let current_frames = self.current.len() / self.current_channels;
            let available = current_frames.saturating_sub(self.current_offset);
            let to_copy = available.min(frames - written_frames);

            for i in 0..to_copy {
                write_channels(
                    out,
                    written_frames + i,
                    output_channels,
                    sample_from_interleaved(
                        &self.current,
                        self.current_offset + i,
                        self.current_channels,
                        0,
                    ),
                    sample_from_interleaved(
                        &self.current,
                        self.current_offset + i,
                        self.current_channels,
                        1,
                    ),
                );
            }

            written_frames += to_copy;
            self.current_offset += to_copy;
            self.position_frames += to_copy as u64;
        }

        Ok(written_frames)
    }

    fn seek_percent(
        &mut self,
        _path: &str,
        percent: f32,
        should_cancel: &mut dyn FnMut() -> bool,
    ) -> Result<SeekOutcome, String> {
        let Some(total_frames) = self.info.total_frames else {
            return Ok(SeekOutcome::Complete(self.position_frames));
        };

        if should_cancel() {
            return Ok(SeekOutcome::Cancelled);
        }

        let percent = percent.clamp(0.0, 1.0);
        let target = (total_frames as f64 * percent as f64) as u64;
        let seconds = target / self.info.sample_rate as u64;
        let frac = (target % self.info.sample_rate as u64) as f64 / self.info.sample_rate as f64;

        let seeked = self
            .format
            .seek(
                SeekMode::Accurate,
                SeekTo::Time {
                    time: Time::new(seconds, frac),
                    track_id: Some(self.track_id),
                },
            )
            .map_err(|err| err.to_string())?;

        if should_cancel() {
            return Ok(SeekOutcome::Cancelled);
        }

        self.decoder.reset();
        self.current.clear();
        self.current_offset = 0;
        self.position_frames = seeked.actual_ts.min(target);

        // Symphonia's accurate seek lands on or before the requested packet. Decode and discard
        // the small remainder so the UI and playback position align with the requested frame.
        let mut scratch = vec![0_i16; AUDIO_GRAIN * self.info.channels as usize];
        while self.position_frames < target {
            if should_cancel() {
                return Ok(SeekOutcome::Cancelled);
            }

            let remaining = (target - self.position_frames) as usize;
            let to_read = remaining.min(AUDIO_GRAIN);
            if self.fill_frames(&mut scratch, to_read)? == 0 {
                break;
            }
            thread::yield_now();
        }

        Ok(SeekOutcome::Complete(self.position_frames))
    }

    fn prime(&mut self) -> Result<(), String> {
        if self.decode_next_chunk()? {
            self.info.channels = (self.current_channels as u32).max(1).min(2);
        }
        Ok(())
    }

    fn decode_next_chunk(&mut self) -> Result<bool, String> {
        loop {
            let packet = match self.format.next_packet() {
                Ok(packet) => packet,
                Err(SymphoniaError::IoError(err))
                    if err.kind() == std::io::ErrorKind::UnexpectedEof =>
                {
                    return Ok(false)
                }
                Err(SymphoniaError::ResetRequired) => {
                    return Err("Format reset required while decoding".to_owned())
                }
                Err(err) => return Err(err.to_string()),
            };

            if packet.track_id() != self.track_id {
                continue;
            }

            let decoded = match self.decoder.decode(&packet) {
                Ok(decoded) => decoded,
                Err(SymphoniaError::DecodeError(_)) => continue,
                Err(SymphoniaError::IoError(err))
                    if err.kind() == std::io::ErrorKind::UnexpectedEof =>
                {
                    return Ok(false)
                }
                Err(err) => return Err(err.to_string()),
            };

            let spec = *decoded.spec();
            let channels = spec.channels.count().max(1);
            let mut sample_buffer = SampleBuffer::<i16>::new(decoded.capacity() as u64, spec);
            sample_buffer.copy_interleaved_ref(decoded);

            self.current.clear();
            self.current.extend_from_slice(sample_buffer.samples());
            self.current_channels = channels;
            self.current_offset = 0;

            if self.info.sample_rate != spec.rate {
                self.info.sample_rate = spec.rate.max(1);
            }
            self.info.channels = (channels as u32).max(1).min(2);

            if !self.current.is_empty() {
                return Ok(true);
            }
        }
    }
}

fn write_channels(out: &mut [i16], frame: usize, channels: usize, left: i16, right: i16) {
    let index = frame * channels;
    if channels == 1 {
        out[index] = apply_pcm_gain(left);
    } else {
        out[index] = apply_pcm_gain(left);
        out[index + 1] = apply_pcm_gain(right);
    }
}

fn apply_pcm_gain(sample: i16) -> i16 {
    ((sample as i32 * PCM_GAIN_Q15) >> 15).clamp(i16::MIN as i32, i16::MAX as i32) as i16
}

fn sample_from_interleaved(data: &[i16], frame: usize, channels: usize, channel: usize) -> i16 {
    let channel = channel.min(channels.saturating_sub(1));
    data.get(frame * channels + channel).copied().unwrap_or(0)
}
