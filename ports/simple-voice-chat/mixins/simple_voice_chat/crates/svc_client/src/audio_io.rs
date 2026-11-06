//! Real audio devices via cpal: microphone capture into 48 kHz mono
//! frames and playback of mixed 48 kHz mono frames. Pure conversion
//! helpers are unit tested; the stream paths need hardware and are
//! exercised on a real client.

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

pub const SAMPLE_RATE: u32 = 48_000;
pub const FRAME_SAMPLES: usize = 960;

type Queue = Arc<Mutex<VecDeque<i16>>>;

/// Average interleaved multi-channel samples down to mono.
pub fn mono_of(interleaved: &[i16], channels: usize) -> Vec<i16> {
    if channels <= 1 {
        return interleaved.to_vec();
    }
    interleaved
        .chunks(channels)
        .map(|ch| {
            let sum: i64 = ch.iter().map(|s| *s as i64).sum();
            (sum / ch.len() as i64) as i16
        })
        .collect()
}

/// Linear resampling to exactly `out_len` samples.
pub fn resample_exact(samples: &[i16], from: u32, to: u32, out_len: usize) -> Vec<i16> {
    if out_len == 0 {
        return Vec::new();
    }
    if samples.is_empty() {
        return vec![0i16; out_len];
    }
    if from == to && samples.len() == out_len {
        return samples.to_vec();
    }
    (0..out_len)
        .map(|i| {
            let t = i as f64 * from as f64 / to as f64;
            let i0 = (t.floor() as usize).min(samples.len() - 1);
            let i1 = (i0 + 1).min(samples.len() - 1);
            let f = (t - i0 as f64) as f32;
            let a = samples[i0] as f32;
            (a + (samples[i1] as f32 - a) * f) as i16
        })
        .collect()
}

fn open_in<T: cpal::SizedSample>(
    dev: &cpal::Device,
    cfg: &cpal::SupportedStreamConfig,
    q: Queue,
    conv: impl Fn(T) -> i16 + Send + 'static,
) -> Result<cpal::Stream, String> {
    dev.build_input_stream(
        &cfg.config(),
        move |data: &[T], _: &cpal::InputCallbackInfo| {
            let mut q = q.lock().unwrap();
            for s in data {
                q.push_back(conv(*s));
            }
        },
        move |_| {},
        None,
    )
    .map_err(|e| e.to_string())
}

fn open_out<T: cpal::SizedSample + Send + 'static>(
    dev: &cpal::Device,
    cfg: &cpal::SupportedStreamConfig,
    q: Queue,
    unconv: impl Fn(i16) -> T + Send + 'static + Copy,
    silence: T,
) -> Result<cpal::Stream, String> {
    dev.build_output_stream(
        &cfg.config(),
        move |data: &mut [T], _: &cpal::OutputCallbackInfo| {
            let mut q = q.lock().unwrap();
            for s in data.iter_mut() {
                *s = q.pop_front().map(unconv).unwrap_or(silence);
            }
        },
        move |_| {},
        None,
    )
    .map_err(|e| e.to_string())
}

/// The microphone: captures whatever format the device gives, converts to
/// mono i16, and hands out 20 ms frames at 48 kHz on demand.
pub struct MicStream {
    _stream: cpal::Stream,
    queue: Queue,
    src_rate: u32,
    src_ch: usize,
    mono: Vec<i16>,
    need: usize,
}

impl MicStream {
    pub fn open() -> Result<Self, String> {
        let host = cpal::default_host();
        let dev = host.default_input_device().ok_or("no input device found")?;
        let cfg = dev
            .default_input_config()
            .map_err(|e| format!("input config: {}", e))?;
        let queue: Queue = Arc::new(Mutex::new(VecDeque::new()));
        let stream = match cfg.sample_format() {
            cpal::SampleFormat::I16 => open_in::<i16>(&dev, &cfg, queue.clone(), |s| s)?,
            cpal::SampleFormat::F32 => {
                open_in::<f32>(&dev, &cfg, queue.clone(), |s| {
                    (s.clamp(-1.0, 1.0) * 32767.0) as i16
                })?
            }
            cpal::SampleFormat::U16 => {
                open_in::<u16>(&dev, &cfg, queue.clone(), |s| (s as i32 - 32768) as i16)?
            }
            other => return Err(format!("unsupported mic sample format {:?}", other)),
        };
        stream.play().map_err(|e| e.to_string())?;
        Ok(Self {
            _stream: stream,
            queue,
            src_rate: cfg.sample_rate().0,
            src_ch: cfg.channels() as usize,
            mono: Vec::new(),
            need: ((FRAME_SAMPLES as u64 * cfg.sample_rate().0 as u64 + 47_999) / 48_000) as usize,
        })
    }

    /// The next 20 ms frame at 48 kHz mono, or None until enough audio is in.
    pub fn poll_frame(&mut self) -> Option<Vec<i16>> {
        let raw: Vec<i16> = self.queue.lock().unwrap().drain(..).collect();
        self.mono.extend(mono_of(&raw, self.src_ch));
        // Drop stale audio so a paused reader never sends minutes-old voice.
        while self.mono.len() > self.need * 12 {
            self.mono.drain(..self.need);
        }
        if self.mono.len() < self.need {
            return None;
        }
        let chunk: Vec<i16> = self.mono.drain(..self.need).collect();
        Some(resample_exact(
            &chunk,
            self.src_rate,
            SAMPLE_RATE,
            FRAME_SAMPLES,
        ))
    }
}

/// The speaker: accepts 48 kHz mono frames, converts to the device's own
/// rate and channel count, and plays them; silence when the queue is empty.
pub struct Speaker {
    _stream: cpal::Stream,
    queue: Queue,
    dst_rate: u32,
    dst_ch: usize,
}

impl Speaker {
    pub fn open() -> Result<Self, String> {
        let host = cpal::default_host();
        let dev = host.default_output_device().ok_or("no output device found")?;
        let cfg = dev
            .default_output_config()
            .map_err(|e| format!("output config: {}", e))?;
        let queue: Queue = Arc::new(Mutex::new(VecDeque::new()));
        let stream = match cfg.sample_format() {
            cpal::SampleFormat::I16 => open_out::<i16>(&dev, &cfg, queue.clone(), |s| s, 0)?,
            cpal::SampleFormat::F32 => {
                open_out::<f32>(&dev, &cfg, queue.clone(), |s| s as f32 / 32767.0, 0.0)?
            }
            cpal::SampleFormat::U16 => open_out::<u16>(&dev, &cfg, queue.clone(), |s| {
                (s as i32 + 32768).clamp(0, u16::MAX as i32) as u16
            }, 32768)?,
            other => return Err(format!("unsupported speaker sample format {:?}", other)),
        };
        stream.play().map_err(|e| e.to_string())?;
        Ok(Self {
            _stream: stream,
            queue,
            dst_rate: cfg.sample_rate().0,
            dst_ch: cfg.channels() as usize,
        })
    }

    /// Queue one frame of 48 kHz mono PCM for playback.
    pub fn play(&self, frame: &[i16]) {
        let out_len =
            (frame.len() as u64 * self.dst_rate as u64 / SAMPLE_RATE as u64) as usize;
        let up = resample_exact(frame, SAMPLE_RATE, self.dst_rate, out_len);
        let mut q = self.queue.lock().unwrap();
        for s in up {
            for _ in 0..self.dst_ch {
                q.push_back(s);
            }
        }
        // Cap the backlog at about two seconds of audio.
        let cap = self.dst_rate as usize * 2 * self.dst_ch.max(1);
        while q.len() > cap {
            q.pop_front();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resampling_keeps_identity_at_native_rate() {
        let x: Vec<i16> = (0..960).map(|i| (i % 100) as i16).collect();
        assert_eq!(resample_exact(&x, 48_000, 48_000, 960), x);
    }

    #[test]
    fn resampling_halves_a_24k_stream() {
        let x: Vec<i16> = (0..960).map(|i| i as i16).collect();
        let down = resample_exact(&x, 48_000, 24_000, 480);
        assert_eq!(down.len(), 480);
        assert_eq!(down[0], 0);
        assert_eq!(down[1], 2);
    }

    #[test]
    fn mono_downmix_averages_channels() {
        let stereo = vec![100, -100, 200, -200];
        assert_eq!(mono_of(&stereo, 2), vec![0, 0]);
        assert_eq!(mono_of(&stereo, 1), stereo);
    }

    #[test]
    fn resample_of_empty_is_silence() {
        assert_eq!(resample_exact(&[], 48_000, 48_000, 10), vec![0i16; 10]);
    }
}
