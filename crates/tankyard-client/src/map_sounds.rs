//! Synthesised map cues, cached independently and warmed on a background
//! worker. The audio callback only reads finished samples.
use std::sync::{Arc, OnceLock};
use tankyard_content::MapSound;

const RATE: f32 = 48_000.;

/// The samples for `sound`, peaking a little below full scale.
pub fn samples(sound: MapSound) -> Arc<[f32]> {
    static BANK: [OnceLock<Arc<[f32]>>; MapSound::ALL.len()] =
        [const { OnceLock::new() }; MapSound::ALL.len()];
    BANK[usize::from(sound.index())]
        .get_or_init(|| normalised(synth(sound)))
        .clone()
}

/// The sounds rifts make, all tones: no hiss.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RiftCue {
    /// A warm swell gliding up an octave, with bells climbing over it.
    Open,
    /// A quick warp up and back down, landing on a soft thump.
    Travel,
    /// A bubble bursting as a rift finishes collapsing.
    Pop,
    /// Two low notes stepping down when a rift shot finds no room.
    Fizzle,
}
impl RiftCue {
    const ALL: [Self; 4] = [Self::Open, Self::Travel, Self::Pop, Self::Fizzle];
}

/// The cue and its loudness for a rift event, if it makes one.
pub fn rift_cue(event: &tankyard_core::GameEvent) -> Option<(RiftCue, f32)> {
    use tankyard_core::EventKind;
    match event.kind {
        EventKind::RiftOpen => Some((RiftCue::Open, 0.55)),
        // Rounds going through stay quiet; tanks are heard at both ends.
        EventKind::RiftTravel if event.value == 0 => Some((RiftCue::Travel, 0.5)),
        EventKind::RiftClose => Some((RiftCue::Pop, 0.6)),
        EventKind::RiftFizzle => Some((RiftCue::Fizzle, 0.45)),
        _ => None,
    }
}

/// The samples for `cue`, made once.
pub fn rift_samples(cue: RiftCue) -> Arc<[f32]> {
    static BANK: [OnceLock<Arc<[f32]>>; RiftCue::ALL.len()] =
        [const { OnceLock::new() }; RiftCue::ALL.len()];
    BANK[cue as usize]
        .get_or_init(|| normalised(rift_synth(cue)))
        .clone()
}

/// Prepare fixed-size clips without holding up the first map trigger.
pub fn prepare() {
    static STARTED: OnceLock<()> = OnceLock::new();
    if STARTED.set(()).is_ok() {
        let _ = std::thread::Builder::new()
            .name("map-sounds".into())
            .spawn(|| {
                for sound in MapSound::ALL {
                    let _ = samples(sound);
                }
                for cue in RiftCue::ALL {
                    let _ = rift_samples(cue);
                }
            });
    }
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
    let mut alarm_phase = 0.;
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
                    // Keep phase continuous when the pitch changes; switching
                    // sin(pitch * time) jumps the waveform and clicks.
                    alarm_phase = (alarm_phase + TAU * pitch / RATE) % TAU;
                    let envelope = (t / 0.02).min(1.) * ((1.5 - t) / 0.12).clamp(0., 1.);
                    (alarm_phase.sin() * 2.2).tanh() * envelope * 0.6
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

fn rift_synth(cue: RiftCue) -> Vec<f32> {
    use std::f32::consts::TAU;
    let seconds = match cue {
        RiftCue::Open => 0.9,
        RiftCue::Travel => 0.5,
        RiftCue::Pop => 0.4,
        RiftCue::Fizzle => 0.35,
    };
    let count = (seconds * RATE) as usize;
    // Phases advance sample by sample, so gliding pitches never jump.
    let mut phases = [0f32; 4];
    let mut turn = |voice: usize, pitch: f32| {
        phases[voice] = (phases[voice] + TAU * pitch / RATE) % TAU;
        phases[voice]
    };
    // Smoothly from 0 to 1 as `t` goes from 0 to 1.
    let smooth = |t: f32| {
        let t = t.clamp(0., 1.);
        t * t * (3. - 2. * t)
    };
    (0..count)
        .map(|i| {
            let t = i as f32 / RATE;
            let tail = ((seconds - t) / 0.02).min(1.);
            let value = match cue {
                RiftCue::Open => {
                    // D3 rising to D4, easing out, with a fifth above it and
                    // a glassy partial that fades as it settles.
                    let pitch = 146.83 * 2f32.powf(1. - (1. - (t / 0.45).min(1.)).powi(2));
                    let body = turn(0, pitch).sin() + 0.35 * turn(1, pitch * 1.5 * 1.003).sin();
                    let glass = turn(2, pitch * 4.).sin() * 0.12 * (-t / 0.4).exp();
                    let swell = smooth(t / 0.12) * smooth((seconds - t) / 0.4);
                    let thump =
                        turn(3, 60. + 60. * (-t / 0.05).exp()).sin() * strike(t, 0.004, 0.09);
                    let bells = bell(t - 0.18, 587.33, 0.35) * 0.22
                        + bell(t - 0.27, 880., 0.3) * 0.18
                        + bell(t - 0.36, 1174.66, 0.28) * 0.14;
                    (body + glass) * swell * 0.5 + thump * 0.45 + bells
                }
                RiftCue::Travel => {
                    // Up two octaves in a blink and back down one, bright at
                    // first and mellowing, over a low thump.
                    let rise = smooth(t / 0.09);
                    let fall = smooth((t - 0.09) / 0.26);
                    let pitch = 261.63 * 2f32.powf(2. * rise - fall);
                    let bright = 1.5 * (-t / 0.08).exp();
                    let tone = (turn(0, pitch) + bright * turn(1, pitch * 2.).sin()).sin();
                    let thump =
                        turn(2, 90. + 50. * (-t / 0.03).exp()).sin() * strike(t, 0.003, 0.07);
                    tone * strike(t, 0.005, 0.16) * 0.7
                        + thump * 0.4
                        + bell(t - 0.06, 1567.98, 0.18) * 0.12
                }
                RiftCue::Pop => {
                    // A pitch dropping fast, then a few high glints.
                    let pitch = 110. + 760. * (-t / 0.03).exp();
                    let glints =
                        bell(t - 0.05, 2093., 0.12) * 0.22 + bell(t - 0.11, 2637., 0.1) * 0.15;
                    turn(0, pitch).sin() * strike(t, 0.002, 0.07) + glints
                }
                RiftCue::Fizzle => {
                    // E4, then B3, soft and hollow: odd partials only.
                    let mut note = |voice: usize, start: f32, pitch: f32| {
                        let local = t - start;
                        if local < 0. {
                            return 0.;
                        }
                        let phase = turn(voice, pitch);
                        (phase.sin() + (phase * 3.).sin() * 0.25 + (phase * 5.).sin() * 0.08)
                            * strike(local, 0.005, 0.07)
                    };
                    note(0, 0., 329.63) + note(1, 0.11, 246.94)
                }
            };
            value * tail
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rift_cues_are_short_finite_and_end_in_silence() {
        for cue in RiftCue::ALL {
            let samples = rift_samples(cue);
            assert!(samples.len() > 1000 && samples.len() < RATE as usize);
            assert!(samples.iter().all(|s| s.is_finite() && s.abs() <= 0.81));
            assert!(samples[samples.len() - 1].abs() < 0.01);
        }
    }

    #[test]
    fn rift_cues_are_tones_without_hiss_or_clicks() {
        for cue in RiftCue::ALL {
            let samples = rift_samples(cue);
            // How much of the sound changes from one sample to the next:
            // about 2 for white noise, tiny for the tones these are made of.
            let energy: f32 = samples.iter().map(|s| s * s).sum();
            let change: f32 = samples.windows(2).map(|p| (p[1] - p[0]).powi(2)).sum();
            assert!(
                change / energy < 0.05,
                "{cue:?} hisses: {}",
                change / energy
            );
            let jump = samples
                .windows(2)
                .map(|p| (p[1] - p[0]).abs())
                .fold(0., f32::max);
            assert!(jump < 0.3, "{cue:?} clicks: {jump}");
        }
    }

    #[test]
    fn alarm_pitch_changes_do_not_jump_the_waveform() {
        let alarm = samples(MapSound::Alarm);
        for beat in 1..7 {
            let at = (beat as f32 * 0.19 * RATE) as usize;
            for pair in alarm[at - 3..at + 4].windows(2) {
                assert!((pair[1] - pair[0]).abs() < 0.25);
            }
        }
    }

    #[test]
    fn every_sound_is_short_finite_and_ends_in_silence() {
        for sound in MapSound::ALL {
            let samples = samples(sound);
            assert!(Arc::ptr_eq(&samples, &super::samples(sound)));
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
