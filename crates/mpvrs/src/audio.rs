use std::fs::File;
use std::io::{BufReader, Read};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};

use claxon::frame::Block;
use claxon::FlacReader;
use lewton::inside_ogg::OggStreamReader;
use minimp3_sys::{
    mp3dec_decode_frame, mp3dec_frame_info_t, mp3dec_init, mp3dec_t, MINIMP3_MAX_SAMPLES_PER_FRAME,
};
use vitasdk_sys::{
    sceAppMgrAcquireBgmPort, sceAppMgrReleaseBgmPort, sceAudioOutOpenPort, sceAudioOutOutput,
    sceAudioOutReleasePort, sceAudioOutSetVolume, sceKernelPowerTick, SCE_AUDIO_OUT_MODE_MONO,
    SCE_AUDIO_OUT_MODE_STEREO, SCE_AUDIO_OUT_PORT_TYPE_BGM, SCE_AUDIO_VOLUME_FLAG_L_CH,
    SCE_AUDIO_VOLUME_FLAG_R_CH, SCE_KERNEL_POWER_TICK_DISABLE_AUTO_SUSPEND,
    SCE_KERNEL_POWER_TICK_DISABLE_OLED_OFF,
};

const AUDIO_GRAIN: usize = 960;
const VITA_VOLUME_MAX: i32 = 0x8000;

#[derive(Clone, Debug)]
pub struct PlaybackSnapshot {
    pub path: String,
    pub name: String,
    pub is_playing: bool,
    pub position_seconds: f32,
    pub duration_seconds: Option<f32>,
    pub status: String,
}

#[derive(Clone, Debug)]
struct SharedState {
    path: String,
    name: String,
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
            name: self.name.clone(),
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
}

pub struct AudioPlayer {
    state: Arc<Mutex<SharedState>>,
    commands: Arc<Mutex<Commands>>,
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl AudioPlayer {
    pub fn open(path: String) -> Result<Self, String> {
        if !is_supported_audio_file(&path) {
            return Err("Unsupported audio file".to_owned());
        }

        let name = Path::new(&path)
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| path.clone());

        let mut decoder = Decoder::open(&path)?;
        let info = decoder.info();

        let state = Arc::new(Mutex::new(SharedState {
            path: path.clone(),
            name,
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
        self.commands.lock().unwrap().seek_to = Some(percent);
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

pub fn is_supported_audio_file(path: &str) -> bool {
    matches!(
        Path::new(path)
            .extension()
            .and_then(|ext| ext.to_str())
            .map(|ext| ext.to_ascii_lowercase())
            .as_deref(),
        Some("mp3" | "flac" | "ogg")
    )
}

fn run_audio_thread(
    path: String,
    decoder: &mut Decoder,
    state: Arc<Mutex<SharedState>>,
    commands: Arc<Mutex<Commands>>,
    stop: Arc<AtomicBool>,
) {
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

    let output_channels = info.channels.max(1).min(2) as usize;
    let mut buffer = vec![0_i16; AUDIO_GRAIN * output_channels];

    while !stop.load(Ordering::SeqCst) {
        unsafe {
            sceKernelPowerTick(SCE_KERNEL_POWER_TICK_DISABLE_AUTO_SUSPEND);
            sceKernelPowerTick(SCE_KERNEL_POWER_TICK_DISABLE_OLED_OFF);
        }

        if let Some(percent) = commands.lock().unwrap().seek_to.take() {
            match decoder.seek_percent(&path, percent) {
                Ok(position_frames) => {
                    let mut state = state.lock().unwrap();
                    state.position_frames = position_frames;
                    state.status = if state.is_playing {
                        "Playing"
                    } else {
                        "Paused"
                    }
                    .to_owned();
                }
                Err(err) => state.lock().unwrap().status = format!("Seek failed: {err}"),
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
                    }
                    state.lock().unwrap().position_frames = decoder.position_frames();
                }
                Err(err) => {
                    buffer.fill(0);
                    let mut state = state.lock().unwrap();
                    state.is_playing = false;
                    state.status = format!("Decode failed: {err}");
                }
            }
        } else {
            buffer.fill(0);
        }

        unsafe {
            sceAudioOutOutput(port, buffer.as_ptr().cast());
        }
    }

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

enum Decoder {
    Mp3(Mp3Decoder),
    Flac(FlacDecoder),
    Ogg(OggDecoder),
}

impl Decoder {
    fn open(path: &str) -> Result<Self, String> {
        let ext = Path::new(path)
            .extension()
            .and_then(|ext| ext.to_str())
            .map(|ext| ext.to_ascii_lowercase())
            .unwrap_or_default();

        match ext.as_str() {
            "mp3" => Mp3Decoder::open(path).map(Self::Mp3),
            "flac" => FlacDecoder::open(path).map(Self::Flac),
            "ogg" => OggDecoder::open(path).map(Self::Ogg),
            _ => Err("Unsupported audio file".to_owned()),
        }
    }

    fn info(&self) -> StreamInfo {
        match self {
            Self::Mp3(decoder) => decoder.info,
            Self::Flac(decoder) => decoder.info,
            Self::Ogg(decoder) => decoder.info,
        }
    }

    fn position_frames(&self) -> u64 {
        match self {
            Self::Mp3(decoder) => decoder.position_frames,
            Self::Flac(decoder) => decoder.position_frames,
            Self::Ogg(decoder) => decoder.position_frames,
        }
    }

    fn fill_frames(&mut self, out: &mut [i16], frames: usize) -> Result<usize, String> {
        match self {
            Self::Mp3(decoder) => decoder.fill_frames(out, frames),
            Self::Flac(decoder) => decoder.fill_frames(out, frames),
            Self::Ogg(decoder) => decoder.fill_frames(out, frames),
        }
    }

    fn seek_percent(&mut self, path: &str, percent: f32) -> Result<u64, String> {
        match self {
            Self::Mp3(decoder) => decoder.seek_percent(path, percent),
            Self::Flac(decoder) => decoder.seek_percent(path, percent),
            Self::Ogg(decoder) => decoder.seek_percent(percent),
        }
    }
}

struct Mp3Decoder {
    data: Vec<u8>,
    offset: usize,
    decoder: Box<mp3dec_t>,
    current: Vec<i16>,
    current_channels: usize,
    current_offset: usize,
    info: StreamInfo,
    position_frames: u64,
}

impl Mp3Decoder {
    fn open(path: &str) -> Result<Self, String> {
        let mut file = File::open(path).map_err(|err| err.to_string())?;
        let file_size = file.metadata().ok().map(|meta| meta.len()).unwrap_or(0);
        let mut data = Vec::new();
        file.read_to_end(&mut data).map_err(|err| err.to_string())?;

        let mut decoder = new_mp3_decoder();
        let mut offset = 0;
        let first = decode_next_mp3_frame(&data, &mut offset, &mut *decoder)?
            .ok_or_else(|| "No MP3 frames found".to_owned())?;

        let channels = first.channels.max(1).min(2) as u32;
        let sample_rate = first.sample_rate.max(1) as u32;
        let total_frames = if first.bitrate > 0 && file_size > 0 {
            let seconds = file_size as f64 * 8.0 / (first.bitrate as f64 * 1000.0);
            Some((seconds * sample_rate as f64) as u64)
        } else {
            None
        };

        Ok(Self {
            data,
            offset: first.next_offset,
            decoder,
            current: first.data,
            current_channels: first.channels.max(1),
            current_offset: 0,
            info: StreamInfo {
                sample_rate,
                channels,
                total_frames,
            },
            position_frames: 0,
        })
    }

    fn fill_frames(&mut self, out: &mut [i16], frames: usize) -> Result<usize, String> {
        let channels = self.info.channels as usize;
        let mut written_frames = 0;

        while written_frames < frames {
            if self.current_offset >= self.current.len() / self.current_channels {
                if let Some(frame) =
                    decode_next_mp3_frame(&self.data, &mut self.offset, &mut *self.decoder)?
                {
                    self.current = frame.data;
                    self.current_channels = frame.channels.max(1);
                    self.current_offset = 0;
                } else {
                    break;
                }
            }

            let frame_frames = self.current.len() / self.current_channels;
            let available = frame_frames.saturating_sub(self.current_offset);
            let to_copy = available.min(frames - written_frames);

            for i in 0..to_copy {
                write_channels(
                    out,
                    written_frames + i,
                    channels,
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

    fn seek_percent(&mut self, path: &str, percent: f32) -> Result<u64, String> {
        let Some(total_frames) = self.info.total_frames else {
            return Ok(self.position_frames);
        };
        let target = (total_frames as f32 * percent.clamp(0.0, 1.0)) as u64;
        *self = Self::open(path)?;
        skip_frames(self, target)?;
        Ok(self.position_frames)
    }
}

struct DecodedMp3Frame {
    data: Vec<i16>,
    sample_rate: i32,
    channels: usize,
    bitrate: i32,
    next_offset: usize,
}

fn new_mp3_decoder() -> Box<mp3dec_t> {
    let mut decoder = unsafe { Box::new(std::mem::zeroed::<mp3dec_t>()) };
    unsafe {
        mp3dec_init(&mut *decoder);
    }
    decoder
}

fn decode_next_mp3_frame(
    data: &[u8],
    offset: &mut usize,
    decoder: &mut mp3dec_t,
) -> Result<Option<DecodedMp3Frame>, String> {
    while *offset < data.len() {
        let available = data.len() - *offset;
        let mut pcm = vec![0_i16; MINIMP3_MAX_SAMPLES_PER_FRAME as usize];
        let mut info = unsafe { std::mem::zeroed::<mp3dec_frame_info_t>() };
        let samples = unsafe {
            mp3dec_decode_frame(
                decoder,
                data[*offset..].as_ptr(),
                available.min(i32::MAX as usize) as i32,
                pcm.as_mut_ptr(),
                &mut info,
            )
        };

        if info.frame_bytes > 0 {
            *offset += info.frame_bytes as usize;
        } else {
            break;
        }

        if samples > 0 {
            let channels = info.channels.max(1) as usize;
            let len = samples as usize * channels;
            pcm.truncate(len);
            return Ok(Some(DecodedMp3Frame {
                data: pcm,
                sample_rate: info.hz,
                channels,
                bitrate: info.bitrate_kbps,
                next_offset: *offset,
            }));
        }
    }

    Ok(None)
}

struct FlacDecoder {
    reader: FlacReader<File>,
    block: Option<Block>,
    block_offset: u32,
    info: StreamInfo,
    position_frames: u64,
}

impl FlacDecoder {
    fn open(path: &str) -> Result<Self, String> {
        let reader = FlacReader::open(path).map_err(|err| err.to_string())?;
        let streaminfo = reader.streaminfo();
        Ok(Self {
            reader,
            block: None,
            block_offset: 0,
            info: StreamInfo {
                sample_rate: streaminfo.sample_rate,
                channels: (streaminfo.channels as u32).max(1).min(2),
                total_frames: streaminfo.samples,
            },
            position_frames: 0,
        })
    }

    fn fill_frames(&mut self, out: &mut [i16], frames: usize) -> Result<usize, String> {
        let channels = self.info.channels as usize;
        let shift = flac_shift(self.reader.streaminfo().bits_per_sample);
        let mut written_frames = 0;

        while written_frames < frames {
            if self.block.is_none() {
                let next = self
                    .reader
                    .blocks()
                    .read_next_or_eof(Vec::new())
                    .map_err(|err| err.to_string())?;
                self.block = next;
                self.block_offset = 0;
                if self.block.is_none() {
                    break;
                }
            }

            let block = self.block.as_ref().unwrap();
            let available = block.duration().saturating_sub(self.block_offset) as usize;
            let to_copy = available.min(frames - written_frames);

            for i in 0..to_copy {
                let sample = self.block_offset + i as u32;
                let left = scale_flac_sample(block.sample(0, sample), shift);
                let right = if block.channels() > 1 {
                    scale_flac_sample(block.sample(1, sample), shift)
                } else {
                    left
                };
                write_channels(out, written_frames + i, channels, left, right);
            }

            written_frames += to_copy;
            self.block_offset += to_copy as u32;
            self.position_frames += to_copy as u64;

            if self.block_offset >= block.duration() {
                self.block = None;
                self.block_offset = 0;
            }
        }

        Ok(written_frames)
    }

    fn seek_percent(&mut self, path: &str, percent: f32) -> Result<u64, String> {
        let target = self
            .info
            .total_frames
            .map(|total| (total as f32 * percent.clamp(0.0, 1.0)) as u64)
            .unwrap_or(0);
        *self = Self::open(path)?;
        skip_frames(self, target)?;
        Ok(self.position_frames)
    }
}

struct OggDecoder {
    reader: OggStreamReader<BufReader<File>>,
    packet: Vec<i16>,
    packet_offset: usize,
    info: StreamInfo,
    position_frames: u64,
}

impl OggDecoder {
    fn open(path: &str) -> Result<Self, String> {
        let file = File::open(path).map_err(|err| err.to_string())?;
        let reader = OggStreamReader::new(BufReader::new(file)).map_err(|err| err.to_string())?;
        let sample_rate = reader.ident_hdr.audio_sample_rate;
        let channels = (reader.ident_hdr.audio_channels as u32).max(1).min(2);
        Ok(Self {
            reader,
            packet: Vec::new(),
            packet_offset: 0,
            info: StreamInfo {
                sample_rate,
                channels,
                total_frames: None,
            },
            position_frames: 0,
        })
    }

    fn fill_frames(&mut self, out: &mut [i16], frames: usize) -> Result<usize, String> {
        let channels = self.info.channels as usize;
        let mut written_frames = 0;

        while written_frames < frames {
            if self.packet_offset >= self.packet.len() {
                match self
                    .reader
                    .read_dec_packet_itl()
                    .map_err(|err| err.to_string())?
                {
                    Some(packet) => {
                        self.packet = packet;
                        self.packet_offset = 0;
                    }
                    None => break,
                }
            }

            let packet_channels = channels.max(1);
            let available =
                (self.packet.len().saturating_sub(self.packet_offset)) / packet_channels;
            let to_copy = available.min(frames - written_frames);

            for i in 0..to_copy {
                write_channels(
                    out,
                    written_frames + i,
                    channels,
                    sample_from_interleaved(
                        &self.packet,
                        self.packet_offset / packet_channels + i,
                        packet_channels,
                        0,
                    ),
                    sample_from_interleaved(
                        &self.packet,
                        self.packet_offset / packet_channels + i,
                        packet_channels,
                        1,
                    ),
                );
            }

            self.packet_offset += to_copy * packet_channels;
            written_frames += to_copy;
            self.position_frames += to_copy as u64;
        }

        Ok(written_frames)
    }

    fn seek_percent(&mut self, percent: f32) -> Result<u64, String> {
        if let Some(total_frames) = self.info.total_frames {
            let target = (total_frames as f32 * percent.clamp(0.0, 1.0)) as u64;
            self.reader
                .seek_absgp_pg(target)
                .map_err(|err| err.to_string())?;
            self.packet.clear();
            self.packet_offset = 0;
            self.position_frames = target;
        }
        Ok(self.position_frames)
    }
}

trait SeekableDecoder {
    fn fill_frames(&mut self, out: &mut [i16], frames: usize) -> Result<usize, String>;
    fn position_frames(&self) -> u64;
}

impl SeekableDecoder for Mp3Decoder {
    fn fill_frames(&mut self, out: &mut [i16], frames: usize) -> Result<usize, String> {
        self.fill_frames(out, frames)
    }

    fn position_frames(&self) -> u64 {
        self.position_frames
    }
}

impl SeekableDecoder for FlacDecoder {
    fn fill_frames(&mut self, out: &mut [i16], frames: usize) -> Result<usize, String> {
        self.fill_frames(out, frames)
    }

    fn position_frames(&self) -> u64 {
        self.position_frames
    }
}

fn skip_frames<D: SeekableDecoder>(decoder: &mut D, target: u64) -> Result<(), String> {
    let channels = 2;
    let mut scratch = vec![0_i16; AUDIO_GRAIN * channels];
    while decoder.position_frames() < target {
        let remaining = (target - decoder.position_frames()) as usize;
        let to_read = remaining.min(AUDIO_GRAIN);
        if decoder.fill_frames(&mut scratch, to_read)? == 0 {
            break;
        }
    }
    Ok(())
}

fn write_channels(out: &mut [i16], frame: usize, channels: usize, left: i16, right: i16) {
    let index = frame * channels;
    if channels == 1 {
        out[index] = left;
    } else {
        out[index] = left;
        out[index + 1] = right;
    }
}

fn sample_from_interleaved(data: &[i16], frame: usize, channels: usize, channel: usize) -> i16 {
    let channel = channel.min(channels.saturating_sub(1));
    data.get(frame * channels + channel).copied().unwrap_or(0)
}

fn flac_shift(bits_per_sample: u32) -> u32 {
    bits_per_sample.saturating_sub(16)
}

fn scale_flac_sample(sample: i32, shift: u32) -> i16 {
    (sample >> shift).clamp(i16::MIN as i32, i16::MAX as i32) as i16
}
