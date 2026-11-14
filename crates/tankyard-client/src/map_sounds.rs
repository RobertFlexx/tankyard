//! The sounds a map's Play sound triggers make. Each is synthesised once, at
//! the mixer's 48 kHz, the first time any map sound plays; the audio callback
//! only reads the finished samples.
use std::sync::{Arc, OnceLock};
use tankyard_content::MapSound;

const RATE: f32 = 48_000.;

/// The samples for `sound`, peaking a little below full scale.
pub fn samples(sound: MapSound) -> Arc<[f32]> {
    static BANK: OnceLock<Vec<Arc<[f32]>>> = OnceLock::new();
    BANK.get_or_init(|| {
        MapSound::ALL
            .iter()
            .map(|&s| normalised(synth(s)))
            .collect()
    })[usize::from(sound.index())]
    .clone()
}

/// How loud a sound `distance` world units away is: full within a screen's
/// middle, gone a little past its edge.
pub fn falloff(distance: f32) -> f32 {
    const NEAR: f32 = 420.;
    const FAR: f32 = 2200.;
    (1. - (distance - NEAR) / (FAR - NEAR))
        .clamp(0., 1.)
        .powf(1.6)
}

fn normalised(mut samples: Vec<f32>) -> Arc<[f32]> {
    let peak = samples.iter().fold(0f32, |peak, s| peak.max(s.abs()));
    if peak > 0. {
        let gain = 0.8 / peak;
        samples.iter_mut().for_each(|s| *s *= gain);
    }
    samples.into()
}

/// Quick rise, exponential fall: a struck or plucked note.
fn strike(t: f32, attack: f32, decay: f32) -> f32 {
    if t < 0. {
        0.
    } else if t < attack {
        t / attack
    } else {
        (-(t - attack) / decay).exp()
    }
}

/// A bell-like note: a few slightly inharmonic partials that die away at
/// different rates, the high ones first.
fn bell(t: f32, pitch: f32, decay: f32) -> f32 {
    use std::f32::consts::TAU;
    [
        (1_f32, 1_f32),
        (2.0, 0.5),
        (2.76, 0.32),
        (4.07, 0.18),
        (5.4, 0.1),
    ]
    .iter()
    .map(|&(ratio, level)| {
        let fade = strike(t, 0.004, decay / ratio.sqrt());
        (TAU * pitch * ratio * t).sin() * level * fade
    })
    .sum()
}

/// Repeatable white noise, so every player hears the same whoosh.
struct Noise(u32);
impl Noise {
    fn next(&mut self) -> f32 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 17;
        self.0 ^= self.0 << 5;
        self.0 as f32 / u32::MAX as f32 * 2. - 1.
    }
}

fn synth(sound: MapSound) -> Vec<f32> {
    use std::f32::consts::TAU;
    let seconds = match sound {
        MapSound::Chime => 1.4,
        MapSound::Bell => 2.4,
        MapSound::Alarm => 1.5,
        MapSound::Switch => 0.3,
        MapSound::Whoosh => 0.8,
        MapSound::Rumble => 1.8,
        MapSound::Fanfare => 1.3,
        MapSound::Thump => 0.7,
    };
    let count = (seconds * RATE) as usize;
    let mut noise = Noise(0x2545_f491);
    let mut low = 0.;
    let mut band = 0.;
    (0..count)
        .map(|i| {
            let t = i as f32 / RATE;
            // A short fade at the very end, so no sound stops with a click.
            let tail = ((seconds - t) / 0.02).min(1.);
            let value = match sound {
                MapSound::Chime => bell(t, 784., 0.55) + bell(t - 0.13, 1046.5, 0.7),
                MapSound::Bell => bell(t, 880., 1.3),
                MapSound::Alarm => {
                    // Two tones taking turns, a slightly squared sine each.
                    let pitch = if ((t / 0.19) as u32).is_multiple_of(2) {
                        880.
                    } else {
                        660.
                    };
                    let envelope = (t / 0.02).min(1.) * ((1.5 - t) / 0.12).clamp(0., 1.);
                    ((TAU * pitch * t).sin() * 2.2).tanh() * envelope * 0.6
                }
                MapSound::Switch => {
                    let click = noise.next() * strike(t, 0.0005, 0.004);
                    let clunk = (TAU * 140. * t).sin() * strike(t - 0.045, 0.002, 0.05);
                    click * 0.7 + clunk
                }
                MapSound::Whoosh => {
                    // Noise through a filter that opens and closes again.
                    let shape = (t / 0.8 * std::f32::consts::PI).sin();
                    let cutoff = 0.02 + 0.25 * shape;
                    low += (noise.next() - low) * cutoff;
                    band += (low - band) * 0.08;
                    (low - band) * shape * shape
                }
                MapSound::Rumble => {
                    low += (noise.next() - low) * 0.012;
                    let swell = (t / 0.25).min(1.) * ((1.8 - t) / 1.1).clamp(0., 1.);
                    (low * 6. + (TAU * 44. * t).sin() * 0.35) * swell
                }
                MapSound::Fanfare => [
                    (0., 523.25, 0.18),
                    (0.11, 659.25, 0.18),
                    (0.22, 783.99, 0.75),
                ]
                .iter()
                .map(|&(start, pitch, decay)| {
                    let t = t - start;
                    // Brassy: odd and even harmonics falling off gently.
                    let tone: f32 = (1..=5)
                        .map(|n| (TAU * pitch * n as f32 * t).sin() / n as f32)
                        .sum();
                    tone * strike(t, 0.012, decay) * 0.5
                })
                .sum(),
                MapSound::Thump => [0., 0.28]
                    .iter()
                    .enumerate()
                    .map(|(beat, &start)| {
                        let t = t - start;
                        // The pitch drops as the beat lands.
                        let pitch = 52. + 40. * (-t / 0.03).exp();
                        (TAU * pitch * t).sin()
                            * strike(t, 0.003, 0.09)
                            * if beat == 0 { 1. } else { 0.6 }
                    })
                    .sum(),
            };
            value * tail
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_sound_is_short_finite_and_ends_in_silence() {
        for sound in MapSound::ALL {
            let samples = samples(sound);
            assert!(
                samples.len() > 4_000 && samples.len() < 3 * 48_000,
                "{sound:?}"
            );
            assert!(samples.iter().all(|s| s.is_finite() && s.abs() <= 0.8001));
            assert!(samples.iter().any(|s| s.abs() > 0.5), "{sound:?} is silent");
            assert!(
                samples.last().unwrap().abs() < 0.01,
                "{sound:?} ends in a click"
            );
        }
        assert_eq!(falloff(0.), 1.);
        assert_eq!(falloff(5000.), 0.);
        assert!(falloff(1200.) > 0. && falloff(1200.) < 1.);
    }
}
