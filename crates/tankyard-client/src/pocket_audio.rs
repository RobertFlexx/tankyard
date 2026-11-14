//! Prepared room tone is read directly from embedded PCM in the audio callback.
//! No synthesis, allocation, loading or locking happens per audio sample.
const ROOM: &[u8] = include_bytes!("../../../assets/audio/pocket-room.pcm");
const ENTER: &[u8] = include_bytes!("../../../assets/audio/pocket-enter.pcm");

#[derive(Default)]
pub struct PocketAudio {
    active: bool,
    volume: f32,
    gain: f32,
    position: usize,
    entry: usize,
}

impl PocketAudio {
    pub fn set(&mut self, active: bool, volume: f32) {
        if active && !self.active {
            self.entry = 0;
            self.position = (self.position + 137_777) % (ROOM.len() / 2);
        }
        self.active = active;
        self.volume = volume.clamp(0., 1.);
    }

    pub fn audible(&self) -> bool {
        self.volume > 0. && (self.active || self.gain > 0.0001)
    }

    /// Called at the mixer's fixed 48 kHz rate before device resampling.
    pub fn sample(&mut self) -> f32 {
        self.gain += ((if self.active { 1. } else { 0. }) - self.gain)
            * if self.active { 0.00012 } else { 0.00006 };
        if !self.audible() {
            return 0.;
        }
        let pcm = |bytes: &[u8], index: usize| {
            f32::from(i16::from_le_bytes([bytes[index * 2], bytes[index * 2 + 1]])) / 32768.
        };
        let room = pcm(ROOM, self.position);
        self.position = (self.position + 1) % (ROOM.len() / 2);
        let entry = if self.active && self.entry < ENTER.len() / 2 {
            let sample = pcm(ENTER, self.entry);
            self.entry += 1;
            sample
        } else {
            0.
        };
        (room * self.gain + entry) * self.volume
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn room_fades_mutes_and_never_escapes_the_audio_range() {
        let mut audio = PocketAudio::default();
        assert_eq!(audio.sample(), 0.);
        audio.set(true, 0.7);
        let energy: f32 = (0..48000)
            .map(|_| {
                let sample = audio.sample();
                assert!(sample.is_finite() && sample.abs() < 0.8);
                sample * sample
            })
            .sum();
        assert!(energy > 0.01);
        audio.set(true, 0.);
        assert_eq!(audio.sample(), 0.);
        audio.set(false, 0.7);
        for _ in 0..240000 {
            audio.sample();
        }
        assert_eq!(audio.sample(), 0.);
        assert!(!audio.audible());
    }
}
