//! Opus at the settings Simple Voice Chat uses: 48 kHz mono, 20 ms frames.

pub const SAMPLE_RATE: u32 = 48_000;
pub const FRAME_SAMPLES: usize = 960;

pub struct VoiceCodec {
    encoder: crate::opus_ffi::Encoder,
    decoder: crate::opus_ffi::Decoder,
}

impl VoiceCodec {
    pub fn new() -> Result<Self, crate::opus_ffi::Error> {
        Ok(Self {
            encoder: crate::opus_ffi::Encoder::new(SAMPLE_RATE, crate::opus_ffi::Channels::Mono, crate::opus_ffi::Application::Voip)?,
            decoder: crate::opus_ffi::Decoder::new(SAMPLE_RATE, crate::opus_ffi::Channels::Mono)?,
        })
    }

    /// 960 mono samples to one Opus packet.
    pub fn encode(&mut self, pcm: &[i16]) -> Result<Vec<u8>, crate::opus_ffi::Error> {
        debug_assert_eq!(pcm.len(), FRAME_SAMPLES);
        let mut out = [0u8; 400];
        let n = self.encoder.encode(pcm, &mut out)?;
        Ok(out[..n].to_vec())
    }

    /// One Opus packet to samples; pass None to conceal a lost packet.
    pub fn decode(&mut self, packet: Option<&[u8]>) -> Result<Vec<i16>, crate::opus_ffi::Error> {
        let mut out = vec![0i16; SAMPLE_RATE as usize / 4];
        let (input, fec) = match packet {
            Some(p) => (p, false),
            None => (&[][..], true),
        };
        let n = self.decoder.decode(Some(input), &mut out, fec)?;
        Ok(out[..n].to_vec())
    }
}

/// Scale samples by a 0..=1 volume.
pub fn mix(samples: &mut [i16], volume: f32) {
    for s in samples {
        *s = (*s as f32 * volume).clamp(-32768.0, 32767.0) as i16;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn opus_round_trip_keeps_the_tone() {
        let mut codec = VoiceCodec::new().unwrap();
        let pcm: Vec<i16> = (0..FRAME_SAMPLES)
            .map(|i| {
                (2000.0
                    * (2.0 * std::f32::consts::PI * 440.0 * i as f32 / SAMPLE_RATE as f32).sin())
                    as i16
            })
            .collect();
        let packet = codec.encode(&pcm).unwrap();
        assert!(!packet.is_empty() && packet.len() < 400);
        let back = codec.decode(Some(&packet)).unwrap();
        assert_eq!(back.len(), FRAME_SAMPLES);
        let (rms_in, rms_out) = (rms(&pcm), rms(&back));
        assert!(
            rms_out > rms_in * 0.7 && rms_out < rms_in * 1.3,
            "{rms_in} vs {rms_out}"
        );
    }

    #[test]
    fn a_lost_packet_still_decodes() {
        let mut codec = VoiceCodec::new().unwrap();
        let back = codec.decode(None).unwrap();
        assert!(!back.is_empty() && back.len() <= SAMPLE_RATE as usize / 4);
    }

    #[test]
    fn mixing_attenuates() {
        let mut s = vec![1000i16; 10];
        mix(&mut s, 0.5);
        assert_eq!(s[0], 500);
    }

    fn rms(x: &[i16]) -> f32 {
        (x.iter().map(|&v| (v as f64).powi(2)).sum::<f64>() / x.len() as f64).sqrt() as f32
    }
}
