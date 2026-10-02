//! Real audio output through the OS default device (cpal).
//!
//! Written against the cpal 0.17 docs without a local build (this container
//! has no ALSA headers, no crates access, and no audio device). The cpal
//! surface used is deliberately small and long-stable: `default_host`,
//! `default_output_device`, `default_output_config`, `build_output_stream`,
//! `Stream::play`. If the compiler disagrees anywhere, that line is the bug.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use anyhow::{Context, Result};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

use super::{AudioBackend, DecodedAudio};

/// One live voice, resampled at `play()` time to the device rate with
/// device-channel interleaved samples. `cursor` counts frames.
struct Voice {
    samples: Vec<f32>,
    frames: usize,
    cursor: usize,
    volume: f32,
}

struct Shared {
    voices: HashMap<u64, Voice>,
    next_id: u64,
}

impl Shared {
    fn play(&mut self, samples: Vec<f32>, frames: usize, volume: f32) -> u64 {
        self.next_id += 1;
        let id = self.next_id;
        self.voices.insert(
            id,
            Voice {
                samples,
                frames,
                cursor: 0,
                volume: volume.clamp(0.0, 1.0),
            },
        );
        id
    }

    fn reap(&mut self) {
        self.voices.retain(|_, v| v.cursor < v.frames);
    }
}

/// Linear-resample `audio` to (`out_rate`, `out_channels`), interleaved.
/// Output channel `c` reads source channel `c % in_channels`, so mono
/// duplicates onto stereo and anything wider folds round-robin.
fn resample(audio: &DecodedAudio, out_rate: u32, out_channels: u16) -> Vec<f32> {
    let in_rate = audio.sample_rate.max(1);
    let in_ch = audio.channels.max(1) as usize;
    let out_ch = out_channels.max(1) as usize;
    let frames_in = audio.samples.len() / in_ch;
    if frames_in == 0 {
        return Vec::new();
    }
    let frames_out = ((frames_in as u64 * out_rate as u64) / in_rate as u64) as usize;
    let mut out = Vec::with_capacity(frames_out * out_ch);
    for f in 0..frames_out {
        let pos = f as f64 * in_rate as f64 / out_rate as f64;
        let i0 = (pos.floor() as usize).min(frames_in - 1);
        let i1 = (i0 + 1).min(frames_in - 1);
        let frac = (pos - pos.floor()) as f32;
        for c in 0..out_ch {
            let src = c % in_ch;
            let a = audio.samples[i0 * in_ch + src];
            let b = audio.samples[i1 * in_ch + src];
            out.push(a + (b - a) * frac);
        }
    }
    out
}

/// Mix one callback buffer from `shared`. The cpal data callbacks for every
/// sample format funnel through here, which keeps this unit-testable with
/// no audio hardware present.
fn mix_into(shared: &Mutex<Shared>, out: &mut [f32], channels: usize) {
    let ch = channels.max(1);
    let mut state = shared.lock().unwrap();
    for frame in out.chunks_mut(ch) {
        for slot in frame.iter_mut() {
            *slot = 0.0;
        }
        for voice in state.voices.values_mut() {
            if voice.cursor >= voice.frames {
                continue;
            }
            let base = voice.cursor * ch;
            for (i, slot) in frame.iter_mut().enumerate() {
                *slot += voice.samples[base + i] * voice.volume;
            }
            voice.cursor += 1;
        }
        for slot in frame.iter_mut() {
            *slot = slot.clamp(-1.0, 1.0);
        }
    }
}

pub struct CpalBackend {
    shared: Arc<Mutex<Shared>>,
    channels: u16,
    rate: u32,
    _stream: cpal::Stream,
}

impl CpalBackend {
    pub fn new() -> Result<Self> {
        let host = cpal::default_host();
        let device = host
            .default_output_device()
            .context("no default output device")?;
        let supported = device
            .default_output_config()
            .context("no default output config")?;
        let format = supported.sample_format();
        if !matches!(
            format,
            cpal::SampleFormat::F32 | cpal::SampleFormat::I16 | cpal::SampleFormat::U16
        ) {
            anyhow::bail!("unsupported sample format {format}");
        }
        let config: cpal::StreamConfig = supported.into();
        let (channels, rate) = (config.channels, config.sample_rate);
        let shared = Arc::new(Mutex::new(Shared {
            voices: HashMap::new(),
            next_id: 0,
        }));
        let err_fn = |err| eprintln!("audio output stream error: {err}");
        let stream = match format {
            cpal::SampleFormat::F32 => device.build_output_stream(
                config.clone(),
                Self::data_fn::<f32>(shared.clone(), channels),
                err_fn,
                None,
            ),
            cpal::SampleFormat::I16 => device.build_output_stream(
                config.clone(),
                Self::data_fn::<i16>(shared.clone(), channels),
                err_fn,
                None,
            ),
            _ => device.build_output_stream(
                config.clone(),
                Self::data_fn::<u16>(shared.clone(), channels),
                err_fn,
                None,
            ),
        }
        .context("cannot open output stream")?;
        stream.play().context("cannot start output stream")?;
        Ok(Self {
            shared,
            channels,
            rate,
            _stream: stream,
        })
    }

    fn data_fn<T>(
        shared: Arc<Mutex<Shared>>,
        channels: u16,
    ) -> impl FnMut(&mut [T], &cpal::OutputCallbackInfo) + Send + 'static
    where
        T: cpal::Sample + cpal::FromSample<f32>,
    {
        move |data: &mut [T], _| {
            let mut mixed = vec![0.0f32; data.len()];
            mix_into(&shared, &mut mixed, channels as usize);
            for (slot, value) in data.iter_mut().zip(mixed) {
                *slot = T::from_sample(value);
            }
        }
    }
}

impl AudioBackend for CpalBackend {
    fn play(&mut self, audio: &DecodedAudio, volume: f32) -> u64 {
        let samples = resample(audio, self.rate, self.channels);
        let frames = samples.len() / self.channels.max(1) as usize;
        self.shared.lock().unwrap().play(samples, frames, volume)
    }

    fn stop(&mut self, id: u64) {
        self.shared.lock().unwrap().voices.remove(&id);
    }

    fn set_volume(&mut self, id: u64, volume: f32) {
        if let Some(voice) = self.shared.lock().unwrap().voices.get_mut(&id) {
            voice.volume = volume.clamp(0.0, 1.0);
        }
    }

    fn is_playing(&self, id: u64) -> bool {
        self.shared
            .lock()
            .unwrap()
            .voices
            .get(&id)
            .map(|v| v.cursor < v.frames)
            .unwrap_or(false)
    }

    fn poll(&mut self) {
        self.shared.lock().unwrap().reap();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tone(rate: u32, channels: u16, frames: usize) -> DecodedAudio {
        let mut samples = Vec::with_capacity(frames * channels as usize);
        for f in 0..frames {
            for c in 0..channels as usize {
                samples.push(f as f32 * 0.01 * (c as f32 + 1.0));
            }
        }
        DecodedAudio {
            samples,
            sample_rate: rate,
            channels,
        }
    }

    #[test]
    fn resample_same_rate_is_transparent() {
        let audio = tone(44100, 1, 8);
        assert_eq!(resample(&audio, 44100, 1), audio.samples);
    }

    #[test]
    fn resample_mono_duplicates_onto_stereo() {
        let audio = tone(44100, 1, 4);
        let stereo = resample(&audio, 44100, 2);
        assert_eq!(stereo.len(), 8);
        for (i, chunk) in stereo.chunks_exact(2).enumerate() {
            assert_eq!(chunk, &[audio.samples[i], audio.samples[i]]);
        }
    }

    #[test]
    fn mixer_adds_voices_at_full_volume() {
        let shared = Mutex::new(Shared {
            voices: HashMap::new(),
            next_id: 0,
        });
        let audio = tone(44100, 1, 4);
        shared.lock().unwrap().play(audio.samples.clone(), 4, 1.0);
        let mut out = vec![0.0f32; 4];
        mix_into(&shared, &mut out, 1);
        assert_eq!(out, audio.samples);
        // Voice is spent: next buffer is silence until poll reaps it.
        let mut out2 = vec![9.0f32; 4];
        mix_into(&shared, &mut out2, 1);
        assert_eq!(out2, vec![0.0f32; 4]);
        shared.lock().unwrap().reap();
        assert!(shared.lock().unwrap().voices.is_empty());
    }

    #[test]
    fn mixer_scales_by_volume() {
        let shared = Mutex::new(Shared {
            voices: HashMap::new(),
            next_id: 0,
        });
        shared.lock().unwrap().play(vec![1.0, 1.0], 2, 0.5);
        let mut out = vec![0.0f32; 2];
        mix_into(&shared, &mut out, 1);
        assert_eq!(out, vec![0.5, 0.5]);
    }

    #[test]
    fn device_opens_when_present() {
        let Ok(mut backend) = CpalBackend::new() else {
            eprintln!("skipped: no output device in this environment");
            return;
        };
        // Volume 0 stays silent on real speakers; this only proves the
        // open/play/poll path works against actual hardware.
        let audio = tone(22050, 1, 2205);
        let id = backend.play(&audio, 0.0);
        assert!(backend.is_playing(id));
        backend.set_volume(id, 0.5);
        backend.stop(id);
        assert!(!backend.is_playing(id));
    }
}
