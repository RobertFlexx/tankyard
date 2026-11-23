use crate::{DecodeError, decode};
use serde::{Deserialize, Serialize};

pub const VOICE_SAMPLE_RATE: u32 = 48_000;
pub const VOICE_FRAME_SAMPLES: usize = 960;
/// Opus speech target. Each nearby listener receives a copy of each frame.
pub const VOICE_BITRATE: i32 = 48_000;
pub const MAX_VOICE_PAYLOAD: usize = 400;
pub const MAX_VOICE_BYTES: usize = 512;
pub const VOICE_RADIUS: f32 = 900.;
/// The start of a microphone report. Read as an Opus packet it claims to hold
/// no frames, so a client that has never heard of reports decodes nothing,
/// plays nothing and shows nobody talking. Our encoder never writes it.
const MIC_REPORT: [u8; 4] = [0x03, 0x00, b'm', b'c'];

/// Bounded 20 ms mono Opus packets, on an independent unreliable channel.
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
    if !frame.valid() {
        return Err(DecodeError("invalid voice frame"));
    }
    Ok(frame)
}

pub fn decode_voice_packet(bytes: &[u8]) -> Result<VoicePacket, DecodeError> {
    let packet: VoicePacket = decode(bytes, MAX_VOICE_BYTES)?;
    if packet.player == 0 || !packet.frame.valid() {
        return Err(DecodeError("invalid voice packet"));
    }
    Ok(packet)
}

impl VoiceFrame {
    pub fn valid(&self) -> bool {
        self.round > 0 && (1..=MAX_VOICE_PAYLOAD).contains(&self.samples.len())
    }
}

/// A voice frame's payload saying whether its sender's microphone is on. It
/// rides the voice channel so that parties keep working across versions.
pub fn mic_report(open: bool) -> Vec<u8> {
    let mut samples = Vec::with_capacity(MIC_REPORT.len() + 1);
    samples.extend_from_slice(&MIC_REPORT);
    samples.push(u8::from(open));
    samples
}

/// What a microphone report says, or `None` for anything else.
pub fn read_mic_report(samples: &[u8]) -> Option<bool> {
    match samples.strip_prefix(&MIC_REPORT)? {
        [0] => Some(false),
        [1] => Some(true),
        _ => None,
    }
}

pub fn voice_gain(distance: f32) -> f32 {
    if !distance.is_finite() || distance < 0. {
        return 0.;
    }
    ((VOICE_RADIUS - distance) / (VOICE_RADIUS - 120.))
        .clamp(0., 1.)
        .powi(2)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::encode;

    #[test]
    fn voice_frames_are_bounded_and_proximity_fades_to_silence() {
        let mut frame = VoiceFrame {
            round: 1,
            sequence: u32::MAX,
            samples: vec![255; MAX_VOICE_PAYLOAD],
        };
        assert_eq!(
            decode_voice(&encode(&frame, MAX_VOICE_BYTES).unwrap())
                .unwrap()
                .sequence,
            u32::MAX
        );
        frame.samples.push(0);
        assert!(decode_voice(&encode(&frame, MAX_VOICE_BYTES).unwrap()).is_err());
        frame.samples.clear();
        assert!(decode_voice(&encode(&frame, MAX_VOICE_BYTES).unwrap()).is_err());
        assert!(decode_voice(&vec![0; MAX_VOICE_BYTES + 1]).is_err());
        assert_eq!(voice_gain(0.), 1.);
        assert!(voice_gain(400.) > voice_gain(700.));
        assert_eq!(voice_gain(VOICE_RADIUS), 0.);
        assert_eq!(voice_gain(f32::NAN), 0.);
    }

    #[test]
    fn mic_reports_are_valid_frames_that_say_one_thing() {
        for open in [false, true] {
            let frame = VoiceFrame {
                round: 3,
                sequence: 9,
                samples: mic_report(open),
            };
            let decoded = decode_voice(&encode(&frame, MAX_VOICE_BYTES).unwrap()).unwrap();
            assert_eq!(read_mic_report(&decoded.samples), Some(open));
        }
        for other in [
            &[][..],
            &[0x03, 0x00, b'm', b'c'],
            &[0x03, 0x00, b'm', b'c', 2],
            &[0x03, 0x00, b'm', b'c', 1, 0],
            &[0x78, 0x00, b'm', b'c', 1],
        ] {
            assert_eq!(read_mic_report(other), None, "{other:?}");
        }
    }
}
