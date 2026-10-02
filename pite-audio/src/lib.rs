use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{LazyLock, Mutex};

use anyhow::{Context, Result};

pub const POLL_CHUNK: usize = 4096;

#[derive(Debug, Clone)]
pub struct DecodedAudio {
    pub samples: Vec<f32>,
    pub sample_rate: u32,
    pub channels: u16,
}

impl DecodedAudio {
    pub fn len(&self) -> usize {
        self.samples.len()
    }

    pub fn is_empty(&self) -> bool {
        self.samples.is_empty()
    }
}

pub trait AudioBackend: Send {
    fn play(&mut self, audio: &DecodedAudio, volume: f32) -> u64;
    fn stop(&mut self, id: u64);
    fn set_volume(&mut self, id: u64, volume: f32);
    fn is_playing(&self, id: u64) -> bool;
    fn poll(&mut self);
}

#[derive(Debug)]
struct Voice {
    len: usize,
    cursor: usize,
    volume: f32,
}

#[derive(Debug, Default)]
pub struct WavBackend {
    next_id: u64,
    voices: HashMap<u64, Voice>,
}

impl WavBackend {
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

pub fn decode_wav(path: &Path) -> Result<DecodedAudio> {
    let mut reader =
        hound::WavReader::open(path).with_context(|| format!("cannot open {}", path.display()))?;
    let spec = reader.spec();
    if spec.bits_per_sample != 16 {
        anyhow::bail!("{}: only 16-bit WAV supported, got {}-bit", path.display(), spec.bits_per_sample);
    }
    let mut samples = Vec::with_capacity(reader.len() as usize);
    for sample in reader.samples::<i16>() {
        let sample = sample.with_context(|| format!("cannot decode {}", path.display()))?;
        samples.push(sample as f32 / 32768.0);
    }
    Ok(DecodedAudio {
        samples,
        sample_rate: spec.sample_rate,
        channels: spec.channels,
    })
}

pub struct AudioEngine {
    backend: Box<dyn AudioBackend>,
    project_dir: Option<PathBuf>,
}

impl AudioEngine {
    fn new() -> Self {
        Self {
            backend: Box::new(WavBackend::new()),
            project_dir: None,
        }
    }

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

static ENGINE: LazyLock<Mutex<AudioEngine>> =
    LazyLock::new(|| Mutex::new(AudioEngine::new()));

pub fn set_backend(backend: Box<dyn AudioBackend>) {
    ENGINE.lock().unwrap().backend = backend;
}

pub fn set_project_dir(dir: Option<PathBuf>) {
    ENGINE.lock().unwrap().project_dir = dir;
}

pub fn decode_asset(asset_ref: &str) -> Result<(PathBuf, DecodedAudio)> {
    let engine = ENGINE.lock().unwrap();
    let path = engine.resolve(asset_ref).ok_or_else(|| {
        anyhow::anyhow!("{asset_ref:?} uses res:// but no project is known")
    })?;
    drop(engine);
    if !path.is_file() {
        anyhow::bail!("audio asset not found: {}", path.display());
    }
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
    if !ext.eq_ignore_ascii_case("wav") {
        anyhow::bail!("{}: only .wav supported, got .{ext}", path.display());
    }
    let audio = decode_wav(&path)?;
    Ok((path, audio))
}

pub fn play_file(asset_ref: &str, volume: f32) -> Result<u64> {
    let (_, audio) = decode_asset(asset_ref)?;
    Ok(ENGINE.lock().unwrap().backend.play(&audio, volume))
}

pub fn stop_voice(id: u64) {
    ENGINE.lock().unwrap().backend.stop(id);
}

pub fn set_voice_volume(id: u64, volume: f32) {
    ENGINE.lock().unwrap().backend.set_volume(id, volume);
}

pub fn voice_playing(id: u64) -> bool {
    ENGINE.lock().unwrap().backend.is_playing(id)
}

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
        let samples: Vec<i16> = (0..len).map(|i| (i as f64 * 0.1).sin() as i16 * 1000).collect();
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
        let (_dir, path) = fixture_wav("mock", 100);
        set_project_dir(Some(_dir.clone()));
        let id = play_file("res://blip.wav", 0.8).unwrap();
        assert!(voice_playing(id));
        set_voice_volume(id, 0.5);
        stop_voice(id);
        assert!(!voice_playing(id));
        assert!(!voice_playing(4242));
        set_backend(Box::new(WavBackend::new()));
        std::fs::remove_dir_all(&_dir).ok();
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
        std::fs::write(dir.join("song.mp3"), b"ID3").unwrap();
        set_project_dir(Some(dir.clone()));
        let err = play_file("res://song.mp3", 1.0).unwrap_err();
        assert!(err.to_string().contains("only .wav"), "got: {err:#}");
        set_backend(Box::new(WavBackend::new()));
        std::fs::remove_dir_all(&dir).ok();
    }
}
