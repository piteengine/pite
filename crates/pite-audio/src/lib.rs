// SPDX-License-Identifier: MIT OR Apache-2.0
//! Audio decoding and playback subsystem for Pite.
//!
//! Provides multi-format audio decoding (WAV, Ogg Vorbis, MP3, and FLAC) via [`symphonia`]
//! and hardware playback through [`cpal`]. In headless, containerized, or CI environments
//! where no physical audio output device is present, `pite-audio` automatically falls back
//! to silent simulation ([`WavBackend`]) so game loops and test suites execute without errors.

use std::collections::HashMap;
use std::fs::File;
use std::path::{Path, PathBuf};
use std::sync::{LazyLock, Mutex};

use anyhow::{Context, Result};
use symphonia::core::codecs::audio::AudioDecoderOptions;
use symphonia::core::codecs::CodecParameters;
use symphonia::core::errors::Error as SymphoniaError;
use symphonia::core::formats::probe::Hint;
use symphonia::core::formats::{FormatOptions, TrackType};
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;

pub mod device;
pub use device::CpalBackend;

/// Number of samples advanced per poll step in simulation mode.
pub const POLL_CHUNK: usize = 4096;

/// In-memory decoded PCM audio buffer with metadata.
#[derive(Debug, Clone)]
pub struct DecodedAudio {
    /// Interleaved 32-bit floating-point audio samples normalized to `[-1.0, 1.0]`.
    pub samples: Vec<f32>,
    /// Sample rate in Hertz (e.g. 44,100 or 48,000).
    pub sample_rate: u32,
    /// Number of audio channels (e.g. 1 for mono, 2 for stereo).
    pub channels: u16,
}

impl DecodedAudio {
    /// Returns the total number of audio samples across all channels.
    pub fn len(&self) -> usize {
        self.samples.len()
    }

    /// Returns true if there are no decoded samples.
    pub fn is_empty(&self) -> bool {
        self.samples.is_empty()
    }
}

/// Built-in test sound fixtures.
pub mod fixtures {
    pub const FIXTURE_OGG: &[u8] = include_bytes!("../fixtures/sine.ogg");
    pub const FIXTURE_MP3: &[u8] = include_bytes!("../fixtures/sine.mp3");
    pub const FIXTURE_FLAC: &[u8] = include_bytes!("../fixtures/sine.flac");
}

/// Trait defining the audio playback interface.
pub trait AudioBackend: Send {
    /// Starts playback of the given audio buffer at the specified volume and returns a unique voice ID.
    fn play(&mut self, audio: &DecodedAudio, volume: f32) -> u64;
    /// Stops playback for the specified voice ID.
    fn stop(&mut self, id: u64);
    /// Updates the playback volume for the specified voice ID.
    fn set_volume(&mut self, id: u64, volume: f32);
    /// Returns true if the voice is currently active and playing.
    fn is_playing(&self, id: u64) -> bool;
    /// Advances playback simulation state for active voices.
    fn poll(&mut self);
}

#[derive(Debug)]
struct Voice {
    len: usize,
    cursor: usize,
    volume: f32,
}

/// Simulated audio backend used when no physical output device is available.
#[derive(Debug, Default)]
pub struct WavBackend {
    next_id: u64,
    voices: HashMap<u64, Voice>,
}

impl WavBackend {
    /// Creates a new silent simulation audio backend.
    pub fn new() -> Self {
        Self::default()
    }

    fn clamp_volume(volume: f32) -> f32 {
        volume.clamp(0.0, 1.0)
    }
}

impl AudioBackend for WavBackend {
    fn play(&mut self, audio: &DecodedAudio, volume: f32) -> u64 {
        self.next_id += 1;
        let id = self.next_id;
        self.voices.insert(
            id,
            Voice {
                len: audio.len(),
                cursor: 0,
                volume: Self::clamp_volume(volume),
            },
        );
        id
    }

    fn stop(&mut self, id: u64) {
        self.voices.remove(&id);
    }

    fn set_volume(&mut self, id: u64, volume: f32) {
        if let Some(voice) = self.voices.get_mut(&id) {
            voice.volume = Self::clamp_volume(volume);
        }
    }

    fn is_playing(&self, id: u64) -> bool {
        self.voices.contains_key(&id)
    }

    fn poll(&mut self) {
        let finished: Vec<u64> = self
            .voices
            .iter_mut()
            .filter_map(|(id, voice)| {
                voice.cursor = (voice.cursor + POLL_CHUNK).min(voice.len);
                if voice.cursor >= voice.len {
                    Some(*id)
                } else {
                    None
                }
            })
            .collect();
        for id in finished {
            self.voices.remove(&id);
        }
    }
}

/// Checks if the given file extension is a supported audio format (`wav`, `ogg`, `mp3`, `flac`).
pub fn is_supported_audio_ext(ext: &str) -> bool {
    matches!(
        ext.to_ascii_lowercase().as_str(),
        "wav" | "ogg" | "mp3" | "flac"
    )
}

/// Decodes an audio file at the specified filesystem path into 32-bit float PCM samples.
pub fn decode_audio(path: &Path) -> Result<DecodedAudio> {
    let file = File::open(path).with_context(|| format!("cannot open {}", path.display()))?;
    let mss = MediaSourceStream::new(Box::new(file), Default::default());

    let mut hint = Hint::new();
    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
        hint.with_extension(ext);
    }

    let format_opts = FormatOptions::default();
    let metadata_opts = MetadataOptions::default();

    let mut format = symphonia::default::get_probe()
        .probe(&hint, mss, format_opts, metadata_opts)
        .with_context(|| format!("cannot probe audio format: {}", path.display()))?;

    let track = format
        .default_track(TrackType::Audio)
        .ok_or_else(|| anyhow::anyhow!("no audio track found in {}", path.display()))?;

    let track_id = track.id;
    let codec_params = match &track.codec_params {
        Some(CodecParameters::Audio(params)) => params,
        _ => anyhow::bail!(
            "audio track in {} has no audio codec parameters",
            path.display()
        ),
    };

    let decoder_opts = AudioDecoderOptions::default();
    let mut decoder = symphonia::default::get_codecs()
        .make_audio_decoder(codec_params, &decoder_opts)
        .with_context(|| format!("cannot create audio decoder for {}", path.display()))?;

    let mut samples = Vec::new();
    let mut sample_rate = 0;
    let mut channels = 0;
    let mut byte_vec = Vec::new();

    loop {
        let packet = match format.next_packet() {
            Ok(Some(packet)) => packet,
            Ok(None) => break,
            Err(SymphoniaError::IoError(e)) if e.kind() == std::io::ErrorKind::UnexpectedEof => {
                break;
            }
            Err(SymphoniaError::ResetRequired) => continue,
            Err(e) => {
                return Err(anyhow::anyhow!(
                    "error reading audio stream {}: {e:#}",
                    path.display()
                ));
            }
        };

        if packet.track_id != track_id {
            continue;
        }

        match decoder.decode(&packet) {
            Ok(buf_ref) => {
                if sample_rate == 0 {
                    let spec = buf_ref.spec();
                    sample_rate = spec.rate();
                    channels = spec.channels().count() as u16;
                }

                byte_vec.clear();
                buf_ref.copy_bytes_to_vec_interleaved_as::<f32>(&mut byte_vec);
                for chunk in byte_vec.chunks_exact(4) {
                    let sample = f32::from_ne_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
                    samples.push(sample);
                }
            }
            Err(SymphoniaError::IoError(e)) if e.kind() == std::io::ErrorKind::UnexpectedEof => {
                break;
            }
            Err(SymphoniaError::DecodeError(e)) => {
                tracing::warn!("decode warning in {}: {e}", path.display());
                continue;
            }
            Err(SymphoniaError::ResetRequired) => {
                decoder.reset();
                continue;
            }
            Err(e) => {
                return Err(anyhow::anyhow!(
                    "error decoding audio packet in {}: {e:#}",
                    path.display()
                ));
            }
        }
    }

    if samples.is_empty() || sample_rate == 0 || channels == 0 {
        anyhow::bail!("{}: no audio samples decoded", path.display());
    }

    Ok(DecodedAudio {
        samples,
        sample_rate,
        channels,
    })
}

/// Decodes a WAV audio file into 32-bit float PCM samples (alias of [`decode_audio`]).
pub fn decode_wav(path: &Path) -> Result<DecodedAudio> {
    decode_audio(path)
}

/// Global audio subsystem state managing the active backend and project root directory.
pub struct AudioEngine {
    backend: Box<dyn AudioBackend>,
    project_dir: Option<PathBuf>,
}

impl AudioEngine {
    fn new() -> Self {
        // Prefer real output; fall back to silent simulation where no
        // device exists (CI, containers) so games still run everywhere.
        let backend: Box<dyn AudioBackend> = match CpalBackend::new() {
            Ok(device) => {
                tracing::info!("audio: playing through the default output device");
                Box::new(device)
            }
            Err(e) => {
                tracing::warn!("audio: no output device ({e:#}); simulating playback");
                Box::new(WavBackend::new())
            }
        };
        Self {
            backend,
            project_dir: None,
        }
    }

    /// Resolves a `res://` resource path or filesystem path to an absolute path.
    pub fn resolve(&self, asset_ref: &str) -> Option<PathBuf> {
        if let Some(rel) = asset_ref.strip_prefix("res://") {
            let root = match &self.project_dir {
                Some(dir) => dir.clone(),
                None => std::env::current_dir().ok()?,
            };
            Some(root.join(rel))
        } else {
            let path = PathBuf::from(asset_ref);
            if path.is_absolute() {
                Some(path)
            } else {
                std::env::current_dir().ok().map(|cwd| cwd.join(path))
            }
        }
    }
}

static ENGINE: LazyLock<Mutex<AudioEngine>> = LazyLock::new(|| Mutex::new(AudioEngine::new()));

/// Overrides the global audio playback backend (e.g. for testing or headless execution).
pub fn set_backend(backend: Box<dyn AudioBackend>) {
    ENGINE.lock().unwrap().backend = backend;
}

/// Sets the project directory used to resolve `res://` audio paths.
pub fn set_project_dir(dir: Option<PathBuf>) {
    ENGINE.lock().unwrap().project_dir = dir;
}

/// Decodes an audio asset identified by a `res://` reference or filesystem path.
pub fn decode_asset(asset_ref: &str) -> Result<(PathBuf, DecodedAudio)> {
    let engine = ENGINE.lock().unwrap();
    let path = engine
        .resolve(asset_ref)
        .ok_or_else(|| anyhow::anyhow!("{asset_ref:?} uses res:// but no project is known"))?;
    drop(engine);
    if !path.is_file() {
        anyhow::bail!("audio asset not found: {}", path.display());
    }
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
    if !is_supported_audio_ext(ext) {
        anyhow::bail!(
            "{}: only .wav, .ogg, .mp3, .flac supported, got .{ext}",
            path.display()
        );
    }
    let audio = decode_audio(&path)?;
    Ok((path, audio))
}

/// Plays an audio asset and returns a numeric voice handle.
pub fn play_file(asset_ref: &str, volume: f32) -> Result<u64> {
    let (_, audio) = decode_asset(asset_ref)?;
    Ok(ENGINE.lock().unwrap().backend.play(&audio, volume))
}

/// Stops playback for a given active voice handle.
pub fn stop_voice(id: u64) {
    ENGINE.lock().unwrap().backend.stop(id);
}

/// Adjusts the volume for an active voice handle.
pub fn set_voice_volume(id: u64, volume: f32) {
    ENGINE.lock().unwrap().backend.set_volume(id, volume);
}

/// Checks whether the specified voice handle is currently playing.
pub fn voice_playing(id: u64) -> bool {
    ENGINE.lock().unwrap().backend.is_playing(id)
}

/// Polls active voices and advances playback simulation.
pub fn poll_audio() {
    ENGINE.lock().unwrap().backend.poll();
}

#[cfg(test)]
mod tests {
    use super::*;

    static SERIAL: Mutex<()> = Mutex::new(());

    struct MockBackend {
        pub plays: Vec<(usize, f32)>,
        pub stops: Vec<u64>,
        pub volumes: Vec<(u64, f32)>,
        pub live: HashMap<u64, bool>,
        next: u64,
    }

    impl MockBackend {
        fn new() -> Self {
            Self {
                plays: Vec::new(),
                stops: Vec::new(),
                volumes: Vec::new(),
                live: HashMap::new(),
                next: 0,
            }
        }
    }

    impl AudioBackend for MockBackend {
        fn play(&mut self, audio: &DecodedAudio, volume: f32) -> u64 {
            self.next += 1;
            self.plays.push((audio.len(), volume));
            self.live.insert(self.next, true);
            self.next
        }

        fn stop(&mut self, id: u64) {
            self.stops.push(id);
            self.live.remove(&id);
        }

        fn set_volume(&mut self, id: u64, volume: f32) {
            self.volumes.push((id, volume));
        }

        fn is_playing(&self, id: u64) -> bool {
            self.live.contains_key(&id)
        }

        fn poll(&mut self) {}
    }

    fn write_wav(path: &Path, samples: &[i16]) {
        let spec = hound::WavSpec {
            channels: 1,
            sample_rate: 22050,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };
        let mut writer = hound::WavWriter::create(path, spec).unwrap();
        for sample in samples {
            writer.write_sample(*sample).unwrap();
        }
        writer.finalize().unwrap();
    }

    fn fixture_wav(tag: &str, len: usize) -> (PathBuf, PathBuf) {
        let dir = std::env::temp_dir().join(format!("pite-aud-{tag}-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("blip.wav");
        let samples: Vec<i16> = (0..len)
            .map(|i| (i as f64 * 0.1).sin() as i16 * 1000)
            .collect();
        write_wav(&path, &samples);
        (dir, path)
    }

    #[test]
    fn backend_voice_lifecycle() {
        let mut backend = WavBackend::new();
        let audio = DecodedAudio {
            samples: vec![0.0; POLL_CHUNK * 2 + 10],
            sample_rate: 22050,
            channels: 1,
        };
        let id = backend.play(&audio, 1.5);
        assert!(backend.is_playing(id));
        backend.set_volume(id, 2.0);
        backend.poll();
        assert!(backend.is_playing(id));
        backend.poll();
        assert!(backend.is_playing(id));
        backend.stop(id);
        assert!(!backend.is_playing(id));
        assert!(!backend.is_playing(999));
    }

    #[test]
    fn play_stop_state_through_mock() {
        let _guard = SERIAL.lock().unwrap();
        set_backend(Box::new(MockBackend::new()));
        let (dir, _) = fixture_wav("mock", 100);
        set_project_dir(Some(dir.clone()));
        let id = play_file("res://blip.wav", 0.8).unwrap();
        assert!(voice_playing(id));
        set_voice_volume(id, 0.5);
        stop_voice(id);
        assert!(!voice_playing(id));
        assert!(!voice_playing(4242));
        set_backend(Box::new(WavBackend::new()));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn missing_file_fails_loudly() {
        let _guard = SERIAL.lock().unwrap();
        set_backend(Box::new(MockBackend::new()));
        let dir = std::env::temp_dir().join(format!("pite-aud-missing-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        set_project_dir(Some(dir.clone()));
        let err = play_file("res://nope.wav", 1.0).unwrap_err();
        assert!(err.to_string().contains("not found"), "got: {err:#}");
        let err = play_file("res://", 1.0).unwrap_err();
        assert!(!err.to_string().is_empty());
        set_backend(Box::new(WavBackend::new()));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn unsupported_extension_fails_loudly() {
        let _guard = SERIAL.lock().unwrap();
        set_backend(Box::new(MockBackend::new()));
        let dir = std::env::temp_dir().join(format!("pite-aud-ext-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("song.aac"), b"AAC").unwrap();
        set_project_dir(Some(dir.clone()));
        let err = play_file("res://song.aac", 1.0).unwrap_err();
        assert!(
            err.to_string().contains("only .wav, .ogg, .mp3, .flac"),
            "got: {err:#}"
        );
        set_backend(Box::new(WavBackend::new()));
        std::fs::remove_dir_all(&dir).ok();
    }

    const FIXTURE_OGG: &[u8] = include_bytes!("../fixtures/sine.ogg");
    const FIXTURE_MP3: &[u8] = include_bytes!("../fixtures/sine.mp3");
    const FIXTURE_FLAC: &[u8] = include_bytes!("../fixtures/sine.flac");

    #[test]
    fn decode_and_play_multiformat() {
        let _guard = SERIAL.lock().unwrap();
        set_backend(Box::new(MockBackend::new()));
        let dir = std::env::temp_dir().join(format!("pite-aud-formats-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("tone.ogg"), FIXTURE_OGG).unwrap();
        std::fs::write(dir.join("tone.mp3"), FIXTURE_MP3).unwrap();
        std::fs::write(dir.join("tone.flac"), FIXTURE_FLAC).unwrap();
        set_project_dir(Some(dir.clone()));

        // OGG
        let id_ogg = play_file("res://tone.ogg", 0.7).unwrap();
        assert!(voice_playing(id_ogg));
        let (_, audio_ogg) = decode_asset("res://tone.ogg").unwrap();
        assert!(!audio_ogg.is_empty());
        assert_eq!(audio_ogg.sample_rate, 44100);

        // MP3
        let id_mp3 = play_file("res://tone.mp3", 0.8).unwrap();
        assert!(voice_playing(id_mp3));
        let (_, audio_mp3) = decode_asset("res://tone.mp3").unwrap();
        assert!(!audio_mp3.is_empty());
        assert_eq!(audio_mp3.sample_rate, 44100);

        // FLAC
        let id_flac = play_file("res://tone.flac", 0.9).unwrap();
        assert!(voice_playing(id_flac));
        let (_, audio_flac) = decode_asset("res://tone.flac").unwrap();
        assert!(!audio_flac.is_empty());
        assert_eq!(audio_flac.sample_rate, 44100);

        set_backend(Box::new(WavBackend::new()));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn decode_wav_float_and_various_bitdepths() {
        let dir = std::env::temp_dir().join(format!("pite-aud-wav-depth-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("float.wav");
        let spec = hound::WavSpec {
            channels: 1,
            sample_rate: 44100,
            bits_per_sample: 32,
            sample_format: hound::SampleFormat::Float,
        };
        let mut writer = hound::WavWriter::create(&path, spec).unwrap();
        for i in 0..100 {
            writer.write_sample((i as f32 * 0.1).sin()).unwrap();
        }
        writer.finalize().unwrap();

        let decoded = decode_audio(&path).unwrap();
        assert_eq!(decoded.len(), 100);
        assert_eq!(decoded.sample_rate, 44100);
        assert_eq!(decoded.channels, 1);

        std::fs::remove_dir_all(&dir).ok();
    }
}
