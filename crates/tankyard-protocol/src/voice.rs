use crate::{DecodeError, decode};
use serde::{Deserialize, Serialize};

pub const VOICE_SAMPLE_RATE: u32 = 16_000;
pub const VOICE_FRAME_SAMPLES: usize = 320;
pub const MAX_VOICE_BYTES: usize = 384;
pub const VOICE_RADIUS: f32 = 900.;

/// Independent 20 ms, mono G.711 mu-law frames. Lost packets never block input
/// or later audio and each packet can be decoded without earlier frames.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct VoiceFrame {
    pub round: u32,
    pub sequence: u32,
    pub samples: Vec<u8>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct VoicePacket {
    pub player: u64,
    pub volume: u8,
    pub frame: VoiceFrame,
}

pub fn decode_voice(bytes: &[u8]) -> Result<VoiceFrame, DecodeError> {
    let frame: VoiceFrame = decode(bytes, MAX_VOICE_BYTES)?;
    if frame.round == 0 || frame.samples.len() != VOICE_FRAME_SAMPLES {
        return Err(DecodeError("invalid voice frame"));
    }
    Ok(frame)
}

pub fn decode_voice_packet(bytes: &[u8]) -> Result<VoicePacket, DecodeError> {
    let packet: VoicePacket = decode(bytes, MAX_VOICE_BYTES)?;
    if packet.player == 0
        || packet.frame.round == 0
        || packet.frame.samples.len() != VOICE_FRAME_SAMPLES
    {
        return Err(DecodeError("invalid voice packet"));
    }
    Ok(packet)
}

pub fn voice_gain(distance: f32) -> f32 {
    if !distance.is_finite() || distance < 0. {
        return 0.;
    }
    ((VOICE_RADIUS - distance) / (VOICE_RADIUS - 120.))
        .clamp(0., 1.)
        .powi(2)
}

pub fn encode_voice_sample(sample: f32) -> u8 {
    let sample = (sample.clamp(-1., 1.) * 32767.) as i32;
    let sign = if sample < 0 { 0x80 } else { 0 };
    let magnitude = (sample.abs().min(32635) + 132) as u32;
    let exponent = (31 - magnitude.leading_zeros()).saturating_sub(7).min(7);
    let mantissa = (magnitude >> (exponent + 3)) & 0x0f;
    !(sign | (exponent << 4) | mantissa) as u8
}

pub fn decode_voice_sample(sample: u8) -> f32 {
    let sample = !sample;
    let magnitude = ((i32::from(sample & 0x0f) << 3) + 132) << ((sample >> 4) & 7);
    let linear = magnitude - 132;
    (if sample & 0x80 != 0 { -linear } else { linear }) as f32 / 32768.
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::encode;

    #[test]
    fn speech_codec_preserves_sign_amplitude_and_silence() {
        for sample in [-1., -0.75, -0.1, -0.001, 0., 0.001, 0.1, 0.75, 1.] {
            let decoded = decode_voice_sample(encode_voice_sample(sample));
            assert!((decoded - sample).abs() < 0.035, "{sample} -> {decoded}");
        }
        assert_eq!(decode_voice_sample(encode_voice_sample(0.)), 0.);
    }

    #[test]
    fn voice_frames_are_bounded_and_proximity_fades_to_silence() {
        let mut frame = VoiceFrame {
            round: 1,
            sequence: u32::MAX,
            samples: vec![255; VOICE_FRAME_SAMPLES],
        };
        assert_eq!(
            decode_voice(&encode(&frame, MAX_VOICE_BYTES).unwrap())
                .unwrap()
                .sequence,
            u32::MAX
        );
        frame.samples.pop();
        assert!(decode_voice(&encode(&frame, MAX_VOICE_BYTES).unwrap()).is_err());
        assert!(decode_voice(&vec![0; MAX_VOICE_BYTES + 1]).is_err());
        assert_eq!(voice_gain(0.), 1.);
        assert!(voice_gain(400.) > voice_gain(700.));
        assert_eq!(voice_gain(VOICE_RADIUS), 0.);
        assert_eq!(voice_gain(f32::NAN), 0.);
    }
}
