//! Recorded and layered map cues, cached independently and warmed on a background
//! worker. The audio callback only reads finished samples.
use std::sync::{Arc, OnceLock};
use tankyard_content::{MapSound, SoundRange};
use tankyard_core::{EntityId, SoundCue};

/// How much further a sound set to carry far reaches than a nearby one.
const FAR_REACH: f32 = 2.5;
/// The most plays waiting their turn at once. A map that asks for more
/// loses its latest repeats, not its first plays.
const MOST_WAITING: usize = 64;

/// How loud `cue` is for player `me`, `distance` away from it.
pub fn reach(cue: &SoundCue, distance: f32, me: EntityId) -> f32 {
    let reach = match cue.range {
        SoundRange::Near => falloff(distance),
        SoundRange::Far => falloff(distance / FAR_REACH),
        SoundRange::Everywhere => 1.,
        // Only the tank that set it off hears it. Servers send it to that
        // player alone; this keeps offline games the same.
        SoundRange::Cause if cue.listener != 0 && cue.listener == me => 1.,
        SoundRange::Cause => 0.,
    };
    cue.volume * reach
}

/// Plays map sound cues: picks each play's pitch, and holds repeats back
/// until it's their turn.
#[derive(Default)]
pub struct CuePlayer {
    waiting: Vec<Waiting>,
    dice: u32,
}
#[derive(Clone, Copy)]
struct Waiting {
    at: f64,
    first: bool,
    sound: MapSound,
    gain: f32,
    /// Semitones.
    pitch: f32,
    /// Semitones either way.
    vary: f32,
}
impl CuePlayer {
    /// Queue `cue`'s plays at `gain`, the first one now.
    pub fn play(&mut self, cue: &SoundCue, gain: f32, now: f64) {
        if !gain.is_finite() || gain <= 0.001 {
            return;
        }
        for play in 0..cue.plays.max(1) {
            if self.waiting.len() >= MOST_WAITING {
                // A long repeat sequence must not swallow a new button or
                // pickup sound. Sacrifice its latest repeat for this cue.
                let repeat = if play == 0 {
                    self.waiting
                        .iter()
                        .enumerate()
                        .filter(|(_, pending)| !pending.first)
                        .max_by(|(_, a), (_, b)| a.at.total_cmp(&b.at))
                        .map(|(index, _)| index)
                } else {
                    None
                };
                let Some(repeat) = repeat else { break };
                self.waiting.swap_remove(repeat);
            }
            self.waiting.push(Waiting {
                at: now + f64::from(play) * f64::from(cue.gap),
                first: play == 0,
                sound: cue.sound,
                gain,
                pitch: f32::from(cue.pitch),
                vary: f32::from(cue.vary) / 10.,
            });
        }
    }
    /// The plays due by `now`, each as its samples, gain and playback rate.
    pub fn due(&mut self, now: f64) -> Vec<(Arc<[f32]>, f32, f32)> {
        let mut due = Vec::new();
        let mut index = 0;
        while index < self.waiting.len() {
            if self.waiting[index].at > now {
                index += 1;
                continue;
            }
            let play = self.waiting.swap_remove(index);
            let wander = if play.vary > 0. {
                (self.roll() * 2. - 1.) * play.vary
            } else {
                0.
            };
            let rate = 2f32.powf((play.pitch + wander) / 12.);
            due.push((samples(play.sound), play.gain, rate));
        }
        due
    }
    /// From 0 to 1.
    fn roll(&mut self) -> f32 {
        if self.dice == 0 {
            self.dice = 0x2545_f491;
        }
        self.dice ^= self.dice << 13;
        self.dice ^= self.dice >> 17;
        self.dice ^= self.dice << 5;
        (self.dice >> 8) as f32 / (1 << 24) as f32
    }
}

/// The samples for `sound`, peaking a little below full scale.
pub fn samples(sound: MapSound) -> Arc<[f32]> {
    static BANK: [OnceLock<Arc<[f32]>>; MapSound::ALL.len()] =
        [const { OnceLock::new() }; MapSound::ALL.len()];
    BANK[usize::from(sound.index())]
        .get_or_init(|| normalised(crate::recorded_sounds::map(sound)))
        .clone()
}

/// The sounds rifts make: short, layered swells and impacts.
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
        .get_or_init(|| normalised(crate::recorded_sounds::rift(cue)))
        .clone()
}

/// The bosses' own sounds, made in [`crate::boss_sounds`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BossCue {
    /// A rising whine as the coils charge.
    Charge,
    /// Two hard pips the moment its aim stops following.
    Lock,
    /// The crack of the shot, with a thud under it and a ringing barrel.
    Shot,
    /// The plasma stream's buzzing hum.
    Beam,
    /// Pressure venting off the fins.
    Vent,
    /// The whole thing going up.
    Fall,
    /// A parry stopping the rail round dead: the biggest sound there is.
    ClashIon,
    /// A parry splitting the plasma stream.
    ClashBeam,
    /// A parry throwing a burning shell back: a whump and a tail of crackle.
    ClashFire,
    /// A parry turning a steel slug: an anvil, struck once.
    ClashSteel,
    /// A parried strike landing back on the boss that fired it.
    Backlash,
    /// The ground working loose over something huge.
    WormRumble,
    /// The floor giving way under it as it breaks out.
    WormBurst,
    /// Its scream.
    WormRoar,
    /// A rattling hiss as it rears to strike.
    WormHiss,
    /// Its jaws slamming shut.
    WormSnap,
    /// Splinters of bone heaved up and spat.
    WormSpit,
    /// Bile coming up its throat.
    WormGurgle,
    /// Bile or blood sprayed in spurts.
    WormSpray,
    /// Ploughing back into the floor.
    WormDive,
    /// Its sides splitting and larvae spilling out.
    WormBrood,
    /// Its spines rattling as it lashes.
    WormThrash,
    /// Barbs flicked off its tail.
    WormSting,
    /// Its body stretched past what it can take.
    WormStrain,
    /// It tears in two.
    WormTear,
    /// Its last.
    WormDeath,
    JesterArrival,
    MaskChange,
    CarillonArrival,
    HammerLift,
    PressureRelease,
    BastionLatch,
}
impl BossCue {
    const ALL: [Self; 32] = [
        Self::Charge,
        Self::Lock,
        Self::Shot,
        Self::Beam,
        Self::Vent,
        Self::Fall,
        Self::ClashIon,
        Self::ClashBeam,
        Self::ClashFire,
        Self::ClashSteel,
        Self::Backlash,
        Self::WormRumble,
        Self::WormBurst,
        Self::WormRoar,
        Self::WormHiss,
        Self::WormSnap,
        Self::WormSpit,
        Self::WormGurgle,
        Self::WormSpray,
        Self::WormDive,
        Self::WormBrood,
        Self::WormThrash,
        Self::WormSting,
        Self::WormStrain,
        Self::WormTear,
        Self::WormDeath,
        Self::JesterArrival,
        Self::MaskChange,
        Self::CarillonArrival,
        Self::HammerLift,
        Self::PressureRelease,
        Self::BastionLatch,
    ];
    /// How loud it is before distance. A shot carries further than the rest.
    pub fn gain(self) -> f32 {
        match self {
            Self::Charge => 0.42,
            Self::Lock => 0.5,
            Self::Shot => 0.95,
            Self::Beam => 0.42,
            Self::Vent => 0.38,
            Self::Fall => 1.,
            Self::ClashIon | Self::ClashBeam | Self::ClashFire => 1.,
            Self::ClashSteel => 0.86,
            Self::Backlash => 0.82,
            Self::WormRumble => 0.7,
            Self::WormBurst | Self::WormRoar | Self::WormTear | Self::WormDeath => 1.,
            Self::WormHiss | Self::WormGurgle => 0.55,
            Self::WormSnap => 0.85,
            Self::WormSpit | Self::WormSpray | Self::WormThrash | Self::WormSting => 0.6,
            Self::WormDive | Self::WormStrain => 0.75,
            Self::WormBrood => 0.6,
            Self::JesterArrival | Self::CarillonArrival => 1.,
            Self::MaskChange => 0.5,
            Self::HammerLift => 0.72,
            Self::PressureRelease => 0.81,
            Self::BastionLatch => 0.65,
        }
    }
    /// How far away it is treated as being, as a share of the real distance.
    pub fn carry(self) -> f32 {
        match self {
            Self::Shot
            | Self::Fall
            | Self::ClashIon
            | Self::ClashBeam
            | Self::ClashFire
            | Self::ClashSteel => 0.62,
            Self::Backlash => 0.7,
            // The ground shaking and its scream carry across the arena.
            Self::WormRumble
            | Self::WormBurst
            | Self::WormRoar
            | Self::WormTear
            | Self::WormDeath => 0.45,
            _ => 0.85,
        }
    }
}

/// The samples for `cue`, made once.
pub fn boss_samples(cue: BossCue) -> Arc<[f32]> {
    static BANK: [OnceLock<Arc<[f32]>>; BossCue::ALL.len()] =
        [const { OnceLock::new() }; BossCue::ALL.len()];
    BANK[cue as usize]
        .get_or_init(|| normalised(crate::boss_sounds::make(cue)))
        .clone()
}

/// The alarm that plays when a boss arrives, made once.
pub fn arrival_samples() -> Arc<[f32]> {
    static ALARM: OnceLock<Arc<[f32]>> = OnceLock::new();
    ALARM
        .get_or_init(|| normalised(crate::boss_sounds::arrival()))
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
                let _ = arrival_samples();
                for cue in BossCue::ALL {
                    let _ = boss_samples(cue);
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sound_kit::RATE;

    /// Writes every made sound as a WAV file, to listen to while tuning
    /// them: `TANKYARD_SOUND_DUMP=<folder> cargo test -p tankyard-client
    /// --bin tankyard write_sounds -- --ignored`.
    #[test]
    #[ignore = "writes files; run by hand to listen"]
    fn write_sounds() {
        let Some(folder) = std::env::var_os("TANKYARD_SOUND_DUMP") else {
            return;
        };
        let folder = std::path::PathBuf::from(folder);
        std::fs::create_dir_all(&folder).unwrap();
        let write = |name: String, samples: &[f32]| {
            let mut bytes = Vec::with_capacity(44 + samples.len() * 2);
            let data = (samples.len() * 2) as u32;
            bytes.extend_from_slice(b"RIFF");
            bytes.extend_from_slice(&(36 + data).to_le_bytes());
            bytes.extend_from_slice(b"WAVEfmt ");
            bytes.extend_from_slice(&16u32.to_le_bytes());
            bytes.extend_from_slice(&1u16.to_le_bytes());
            bytes.extend_from_slice(&1u16.to_le_bytes());
            bytes.extend_from_slice(&48_000u32.to_le_bytes());
            bytes.extend_from_slice(&96_000u32.to_le_bytes());
            bytes.extend_from_slice(&2u16.to_le_bytes());
            bytes.extend_from_slice(&16u16.to_le_bytes());
            bytes.extend_from_slice(b"data");
            bytes.extend_from_slice(&data.to_le_bytes());
            for sample in samples {
                bytes.extend_from_slice(&((sample.clamp(-1., 1.) * 32767.) as i16).to_le_bytes());
            }
            std::fs::write(folder.join(name), bytes).unwrap();
        };
        for cue in BossCue::ALL {
            write(format!("boss-{cue:?}.wav"), &boss_samples(cue));
        }
        write("boss-arrival.wav".into(), &arrival_samples());
        for cue in RiftCue::ALL {
            write(format!("rift-{cue:?}.wav"), &rift_samples(cue));
        }
        for sound in MapSound::ALL {
            write(format!("map-{sound:?}.wav"), &samples(sound));
        }
    }

    #[test]
    fn repeat_sequences_leave_room_for_new_cues() {
        let mut player = CuePlayer::default();
        let repeating = SoundCue {
            plays: 8,
            gap: 1.,
            ..SoundCue::new(MapSound::Chime, 0.5)
        };
        for _ in 0..MOST_WAITING / 8 {
            player.play(&repeating, 0.5, 10.);
        }
        assert_eq!(player.waiting.len(), MOST_WAITING);
        player.play(&SoundCue::new(MapSound::Thump, 0.8), 0.8, 10.);
        assert_eq!(player.waiting.len(), MOST_WAITING);
        assert!(
            player
                .waiting
                .iter()
                .any(|play| play.sound == MapSound::Thump && play.first)
        );
        assert_eq!(
            player.waiting.iter().filter(|play| play.first).count(),
            MOST_WAITING / 8 + 1
        );
        assert_eq!(player.due(10.).len(), MOST_WAITING / 8 + 1);
        assert!(player.waiting.iter().all(|play| play.at > 10.));
        assert!(!player.due(100.).is_empty());
        assert!(player.waiting.is_empty());
    }

    #[test]
    fn boss_cues_are_finite_bounded_and_end_in_silence() {
        for cue in BossCue::ALL {
            let samples = boss_samples(cue);
            assert!(samples.len() > 1000, "{cue:?}");
            assert!(samples.iter().all(|s| s.is_finite() && s.abs() <= 0.81));
            assert!(samples[samples.len() - 1].abs() < 0.02, "{cue:?} clicks");
            assert!(cue.gain() > 0. && cue.gain() <= 1. && cue.carry() > 0.);
        }
    }

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
    fn recording_based_rifts_start_and_finish_without_clicks() {
        for cue in RiftCue::ALL {
            let samples = rift_samples(cue);
            assert_eq!(samples[0], 0.);
            assert_eq!(*samples.last().unwrap(), 0.);
            let energy: f32 = samples.iter().map(|s| s * s).sum();
            assert!(energy > 1., "{cue:?} is silent");
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
