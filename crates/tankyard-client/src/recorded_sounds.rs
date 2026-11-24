//! Recorded foley and creature performances, decoded once during sound-bank
//! preparation. Source licenses and processing are in `assets/audio/foley`.
use std::sync::OnceLock;

#[derive(Clone, Copy, Debug)]
pub enum Foley {
    MetalHit,
    MetalSlam,
    MetalSheet,
    MetalSpring,
    MetalFall,
    StoneBreak,
    StoneFall,
    Crunch,
    Growl,
    Roar,
    Scream,
    Breath,
    Spit,
    Wet,
    Splash,
    Bubble,
    Steam,
    Click,
    Bell,
    Gong,
    Horn,
}

impl Foley {
    const ALL: [Self; 21] = [
        Self::MetalHit,
        Self::MetalSlam,
        Self::MetalSheet,
        Self::MetalSpring,
        Self::MetalFall,
        Self::StoneBreak,
        Self::StoneFall,
        Self::Crunch,
        Self::Growl,
        Self::Roar,
        Self::Scream,
        Self::Breath,
        Self::Spit,
        Self::Wet,
        Self::Splash,
        Self::Bubble,
        Self::Steam,
        Self::Click,
        Self::Bell,
        Self::Gong,
        Self::Horn,
    ];

    fn bytes(self) -> &'static [u8] {
        macro_rules! clip {
            ($name:literal) => {
                include_bytes!(concat!("../../../assets/audio/foley/", $name, ".pcm"))
            };
        }
        match self {
            Self::MetalHit => clip!("metal-hit"),
            Self::MetalSlam => clip!("metal-slam"),
            Self::MetalSheet => clip!("metal-sheet"),
            Self::MetalSpring => clip!("metal-spring"),
            Self::MetalFall => clip!("metal-fall"),
            Self::StoneBreak => clip!("stone-break"),
            Self::StoneFall => clip!("stone-fall"),
            Self::Crunch => clip!("crunch"),
            Self::Growl => clip!("growl"),
            Self::Roar => clip!("roar"),
            Self::Scream => clip!("scream"),
            Self::Breath => clip!("breath"),
            Self::Spit => clip!("spit"),
            Self::Wet => clip!("wet"),
            Self::Splash => clip!("splash"),
            Self::Bubble => clip!("bubble"),
            Self::Steam => clip!("steam"),
            Self::Click => clip!("click"),
            Self::Bell => clip!("bell"),
            Self::Gong => clip!("gong"),
            Self::Horn => clip!("horn"),
        }
    }

    fn samples(self) -> &'static [f32] {
        static BANK: [OnceLock<Box<[f32]>>; Foley::ALL.len()] =
            [const { OnceLock::new() }; Foley::ALL.len()];
        BANK[self as usize].get_or_init(|| {
            self.bytes()
                .as_chunks::<2>()
                .0
                .iter()
                .map(|b| f32::from(i16::from_le_bytes([b[0], b[1]])) / 32768.)
                .collect()
        })
    }
}

/// Lay a performance into a cue, with a short crossfade at either end. All
/// mixing and pitch changes happen off the audio callback, once per cue.
pub fn layer(out: &mut [f32], clip: Foley, at: f32, rate: f32, gain: f32) {
    mix(out, clip, at, rate, gain, false);
}

pub fn reverse(out: &mut [f32], clip: Foley, at: f32, rate: f32, gain: f32) {
    mix(out, clip, at, rate, gain, true);
}

fn mix(out: &mut [f32], clip: Foley, at: f32, rate: f32, gain: f32, reverse: bool) {
    if !at.is_finite()
        || at < 0.
        || !rate.is_finite()
        || !(0.25..=2.).contains(&rate)
        || !gain.is_finite()
        || gain <= 0.
    {
        return;
    }
    let start = crate::sound_kit::samples_in(at).min(out.len());
    let source = clip.samples();
    let count = ((source.len() - 1) as f32 / rate) as usize;
    let count = count.min(out.len() - start);
    let fade = 240.min(count / 3).max(1);
    for (i, sample) in out[start..start + count].iter_mut().enumerate() {
        let read = i as f32 * rate;
        let read = if reverse {
            (source.len() - 1) as f32 - read
        } else {
            read
        };
        let index = read.floor() as usize;
        let fraction = read - index as f32;
        let a = source[index.min(source.len() - 1)];
        let b = source[(index + 1).min(source.len() - 1)];
        let envelope =
            (i as f32 / fade as f32).min(1.) * ((count - 1 - i) as f32 / fade as f32).min(1.);
        *sample += (a + (b - a) * fraction) * gain * envelope;
    }
}

type MixLayer = (Foley, f32, f32, f32, bool);

fn cue(seconds: f32, layers: &[MixLayer]) -> Vec<f32> {
    let mut out = vec![0.; crate::sound_kit::samples_in(seconds)];
    for &(clip, at, rate, gain, backwards) in layers {
        mix(&mut out, clip, at, rate, gain, backwards);
    }
    crate::sound_kit::low_shelf(&mut out, 140., 0.8);
    crate::sound_kit::finish(
        crate::sound_kit::in_room(&out, crate::sound_kit::Room::SMALL, 0.12),
        0.06,
    )
}

/// Map cues keep their purpose, using physical attacks and natural decays.
pub fn map(sound: tankyard_content::MapSound) -> Vec<f32> {
    use Foley::*;
    use tankyard_content::MapSound as M;
    let (seconds, layers): (f32, &[MixLayer]) = match sound {
        M::Chime => (
            0.95,
            &[(Bell, 0., 1.3, 0.7, false), (Bell, 0.13, 1.75, 0.55, false)],
        ),
        M::Bell => (
            1.1,
            &[(Bell, 0., 0.6, 0.8, false), (Bell, 0., 0.9, 0.4, false)],
        ),
        M::Alarm => (
            1.5,
            &[
                (MetalSpring, 0., 1.4, 0.7, true),
                (MetalSpring, 0.38, 1.15, 0.7, true),
                (MetalHit, 0.75, 1.6, 0.6, false),
            ],
        ),
        M::Switch => (
            0.4,
            &[
                (Click, 0., 1.3, 0.8, false),
                (MetalHit, 0.04, 1.2, 0.2, false),
            ],
        ),
        M::Whoosh => (
            0.85,
            &[
                (Steam, 0., 2., 0.65, false),
                (MetalSpring, 0., 1.4, 0.4, true),
            ],
        ),
        M::Rumble => (
            1.85,
            &[
                (StoneFall, 0., 0.4, 0.8, false),
                (StoneBreak, 0.35, 0.65, 0.5, false),
            ],
        ),
        M::Fanfare => (
            1.2,
            &[
                (Bell, 0., 1., 0.65, false),
                (Bell, 0.16, 1.25, 0.65, false),
                (Bell, 0.32, 1.5, 0.7, false),
                (Bell, 0.32, 1.5, 0.35, false),
            ],
        ),
        M::Thump => (
            0.65,
            &[
                (MetalSlam, 0., 0.7, 0.6, false),
                (StoneFall, 0., 0.7, 0.35, false),
            ],
        ),
        M::Coin => (
            0.65,
            &[(Bell, 0., 1.6, 0.7, false), (Bell, 0.09, 2., 0.5, false)],
        ),
        M::Pop => (
            0.45,
            &[(Bubble, 0., 1.8, 0.8, false), (Spit, 0., 1.6, 0.2, false)],
        ),
        M::Beep => (0.28, &[(Bell, 0., 2., 0.65, false)]),
        M::Click => (0.22, &[(Click, 0., 1.8, 0.65, false)]),
        M::Tick => (
            0.25,
            &[(Click, 0., 1.5, 0.45, false), (Crunch, 0., 2., 0.25, false)],
        ),
        M::Laser => (
            0.6,
            &[
                (MetalSpring, 0., 1.5, 0.8, false),
                (Steam, 0., 2., 0.15, false),
            ],
        ),
        M::PowerUp => (
            0.95,
            &[
                (MetalSpring, 0., 0.8, 0.7, true),
                (MetalHit, 0.4, 1.3, 0.45, false),
                (MetalHit, 0.55, 1.8, 0.5, false),
            ],
        ),
        M::PowerDown => (
            0.95,
            &[
                (MetalSpring, 0., 0.8, 0.7, false),
                (MetalHit, 0.1, 1.3, 0.3, false),
                (MetalHit, 0.24, 0.7, 0.3, false),
            ],
        ),
        M::Buzzer => (
            0.65,
            &[
                (MetalSheet, 0., 0.7, 0.7, false),
                (MetalSpring, 0., 1.1, 0.55, false),
            ],
        ),
        M::Success => (
            1.05,
            &[
                (Bell, 0., 1.2, 0.7, false),
                (Bell, 0.12, 1.5, 0.5, false),
                (Bell, 0.25, 1.8, 0.7, false),
            ],
        ),
        M::Door => (
            1.3,
            &[
                (MetalSheet, 0., 0.65, 0.55, false),
                (Click, 0., 0.7, 0.5, false),
                (MetalSlam, 0.65, 0.8, 0.8, false),
            ],
        ),
        M::Explosion => (
            1.8,
            &[
                (StoneBreak, 0., 0.6, 0.85, false),
                (MetalSlam, 0., 0.65, 0.65, false),
                (StoneFall, 0.14, 0.7, 0.7, false),
            ],
        ),
        M::Teleport => (
            0.85,
            &[
                (MetalSpring, 0., 0.75, 0.6, true),
                (Steam, 0., 2., 0.35, false),
                (Bubble, 0.32, 1.7, 0.5, false),
            ],
        ),
        M::Magic => (
            1.4,
            &[
                (Bell, 0., 1., 0.35, true),
                (Bell, 0.12, 1.2, 0.65, false),
                (Bell, 0.25, 1.5, 0.55, false),
                (Bell, 0.4, 1.8, 0.5, false),
            ],
        ),
        M::Splash => (
            1.,
            &[
                (Splash, 0., 0.95, 0.85, false),
                (Bubble, 0.22, 1.25, 0.35, false),
            ],
        ),
        M::Gong => (
            2.25,
            &[
                (Gong, 0., 0.65, 0.85, false),
                (MetalSheet, 0.02, 0.7, 0.15, false),
            ],
        ),
        M::Thunder => (
            2.45,
            &[
                (StoneBreak, 0., 0.45, 0.8, false),
                (StoneFall, 0.12, 0.4, 0.7, false),
                (StoneBreak, 0.65, 0.6, 0.4, false),
            ],
        ),
        M::Countdown => (
            1.4,
            &[
                (Bell, 0., 1.25, 0.6, false),
                (Bell, 0.3, 1.25, 0.6, false),
                (Bell, 0.6, 1.25, 0.6, false),
                (Bell, 0.9, 1.8, 0.85, false),
            ],
        ),
        M::Drum => (
            0.65,
            &[
                (StoneFall, 0., 0.7, 0.6, false),
                (MetalSlam, 0., 1.1, 0.45, false),
            ],
        ),
        M::Horn => (
            2.75,
            &[(Horn, 0., 0.85, 0.85, false), (Steam, 0., 1.5, 0.1, false)],
        ),
    };
    cue(seconds, layers)
}

pub fn rift(sound: crate::map_sounds::RiftCue) -> Vec<f32> {
    use crate::map_sounds::RiftCue::*;
    use Foley::*;
    match sound {
        Open => cue(
            0.75,
            &[
                (MetalSpring, 0., 0.8, 0.6, true),
                (Bubble, 0.1, 1.5, 0.4, false),
            ],
        ),
        Travel => cue(
            0.5,
            &[
                (Steam, 0., 2., 0.3, true),
                (MetalSpring, 0., 1.4, 0.8, false),
            ],
        ),
        Pop => cue(
            0.4,
            &[
                (Bubble, 0., 1.8, 0.8, false),
                (MetalHit, 0.02, 1.8, 0.25, false),
            ],
        ),
        Fizzle => cue(
            0.5,
            &[
                (MetalSpring, 0., 1.2, 0.6, false),
                (Click, 0., 1., 0.25, false),
            ],
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recordings_are_bounded_nonempty_and_reused() {
        for clip in Foley::ALL {
            assert_eq!(clip.bytes().len() % 2, 0);
            let a = clip.samples();
            assert!(a.len() > 4800 && a.len() <= 192000);
            assert!(a.iter().all(|x| x.is_finite() && x.abs() < 1.));
            assert!(a.iter().any(|x| x.abs() > 0.2));
            assert!(std::ptr::eq(a, clip.samples()));
        }
    }

    #[test]
    fn layering_is_repeatable_and_fades_at_cuts() {
        for rate in [0.25, 0.65, 1., 1.7, 2.] {
            let mut a = vec![0.; 16000];
            let mut b = a.clone();
            layer(&mut a, Foley::MetalHit, 0., rate, 0.5);
            layer(&mut b, Foley::MetalHit, 0., rate, 0.5);
            assert_eq!(a, b);
            assert_eq!(a[0], 0.);
            assert_eq!(a[a.len() - 1], 0.);
            assert!(a.iter().all(|x| x.is_finite() && x.abs() < 1.));
        }
    }
}
