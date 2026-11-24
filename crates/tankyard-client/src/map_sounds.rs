//! Synthesised map cues, cached independently and warmed on a background
//! worker. The audio callback only reads finished samples.
use std::sync::{Arc, OnceLock};
use tankyard_content::{MapSound, SoundRange};
use tankyard_core::{EntityId, SoundCue};

const RATE: f32 = 48_000.;

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

/// The Railgunner's sounds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RailCue {
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
    /// A parry stopping ions dead: a crack, a shriek sliding down, a thump.
    ClashIon,
    /// A parry throwing a burning shell back: a whump and a tail of crackle.
    ClashFire,
    /// A parry turning a steel slug: an anvil, struck once.
    ClashSteel,
    /// A parried strike landing back on the boss that fired it.
    Backlash,
}
impl RailCue {
    const ALL: [Self; 10] = [
        Self::Charge,
        Self::Lock,
        Self::Shot,
        Self::Beam,
        Self::Vent,
        Self::Fall,
        Self::ClashIon,
        Self::ClashFire,
        Self::ClashSteel,
        Self::Backlash,
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
            Self::ClashIon | Self::ClashFire => 1.,
            Self::ClashSteel => 0.86,
            Self::Backlash => 0.82,
        }
    }
    /// How far away it is treated as being, as a share of the real distance.
    pub fn carry(self) -> f32 {
        match self {
            Self::Shot | Self::Fall | Self::ClashIon | Self::ClashFire | Self::ClashSteel => 0.62,
            Self::Backlash => 0.7,
            _ => 0.85,
        }
    }
}

/// The samples for `cue`, made once.
pub fn rail_samples(cue: RailCue) -> Arc<[f32]> {
    static BANK: [OnceLock<Arc<[f32]>>; RailCue::ALL.len()] =
        [const { OnceLock::new() }; RailCue::ALL.len()];
    BANK[cue as usize]
        .get_or_init(|| normalised(rail_synth(cue)))
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
                for cue in RailCue::ALL {
                    let _ = rail_samples(cue);
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
        newer => return more_synth(newer),
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
                _ => 0.,
            };
            value * tail
        })
        .collect()
}

/// A phase-continuous oscillator: its pitch can glide without a click.
#[derive(Clone, Copy, Default)]
struct Osc(f32);
impl Osc {
    fn step(&mut self, pitch: f32) -> f32 {
        self.0 = (self.0 + pitch.max(0.) / RATE).fract();
        self.0
    }
    fn sine(&mut self, pitch: f32) -> f32 {
        (self.step(pitch) * std::f32::consts::TAU).sin()
    }
    /// The first `count` harmonics, each at 1/n: a soft, bright saw.
    fn buzz(&mut self, pitch: f32, count: u32) -> f32 {
        let phase = self.step(pitch) * std::f32::consts::TAU;
        (1..=count)
            .map(|n| (phase * n as f32).sin() / n as f32)
            .sum()
    }
}

/// Smoothly from 0 to 1 as `t` goes from 0 to 1.
fn smooth(t: f32) -> f32 {
    let t = t.clamp(0., 1.);
    t * t * (3. - 2. * t)
}

/// A held note's loudness: up over `attack`, held, and down over `release`
/// at the end of `length`.
fn held(t: f32, attack: f32, length: f32, release: f32) -> f32 {
    if t < 0. || t > length {
        0.
    } else {
        smooth(t / attack.max(1e-4)) * smooth((length - t) / release.max(1e-4))
    }
}

/// The sounds added in 2.0.5.
fn more_synth(sound: MapSound) -> Vec<f32> {
    let seconds = match sound {
        MapSound::Coin => 0.6,
        MapSound::Pop => 0.2,
        MapSound::Beep => 0.24,
        MapSound::Click => 0.1,
        MapSound::Tick => 0.1,
        MapSound::Laser => 0.5,
        MapSound::PowerUp => 1.,
        MapSound::PowerDown => 1.1,
        MapSound::Buzzer => 0.62,
        MapSound::Success => 1.3,
        MapSound::Door => 1.3,
        MapSound::Explosion => 2.,
        MapSound::Teleport => 1.,
        MapSound::Magic => 1.5,
        MapSound::Splash => 1.1,
        MapSound::Gong => 2.9,
        MapSound::Thunder => 2.8,
        MapSound::Countdown => 2.9,
        MapSound::Drum => 0.65,
        MapSound::Horn => 1.5,
        _ => 0.1,
    };
    let count = (seconds * RATE) as usize;
    let mut noise = Noise(0x9e37_79b9 ^ u32::from(sound.index()));
    let mut osc = [Osc::default(); 6];
    let (mut low, mut band, mut deep) = (0f32, 0f32, 0f32);
    // Bubbles and debris land at fixed moments, so every player hears the
    // same splash and the same explosion.
    let mut scatter = Noise(0x5bd1_e995 ^ u32::from(sound.index()));
    let bubbles: Vec<(f32, f32)> = (0..6)
        .map(|_| {
            (
                0.16 + 0.55 * (scatter.next() * 0.5 + 0.5),
                420. + 380. * (scatter.next() * 0.5 + 0.5),
            )
        })
        .collect();
    let crackles: Vec<f32> = (0..26)
        .map(|_| 0.08 + 1.4 * (scatter.next() * 0.5 + 0.5).powi(2))
        .collect();
    (0..count)
        .map(|i| {
            let t = i as f32 / RATE;
            let tail = ((seconds - t) / 0.02).min(1.);
            let value = match sound {
                MapSound::Coin => {
                    // B5 then E6: a sine with a little of its odd harmonics.
                    let note = |osc: &mut Osc, t: f32, pitch: f32, decay: f32| {
                        let phase = osc.step(pitch) * std::f32::consts::TAU;
                        (phase.sin() + 0.28 * (phase * 3.).sin() + 0.12 * (phase * 5.).sin())
                            * strike(t, 0.002, decay)
                    };
                    // The first note lets go just as the second rings.
                    let first = note(&mut osc[0], t, 987.77, 0.04) * smooth((0.082 - t) / 0.01);
                    first + note(&mut osc[1], t - 0.075, 1318.51, 0.2)
                }
                MapSound::Pop => {
                    let pitch = 240. + 700. * (-t / 0.011).exp();
                    osc[0].sine(pitch) * strike(t, 0.0008, 0.032)
                        + noise.next() * strike(t, 0.0002, 0.0025) * 0.35
                }
                MapSound::Beep => osc[0].sine(1046.5) * held(t, 0.005, 0.2, 0.03),
                MapSound::Click => {
                    // A crisp edge of noise, high-passed, over a tiny blip.
                    let n = noise.next();
                    low += (n - low) * 0.3;
                    (n - low) * strike(t, 0.0002, 0.003)
                        + osc[0].sine(2200.) * strike(t, 0.0005, 0.007) * 0.5
                }
                MapSound::Tick => {
                    osc[0].sine(2600.) * strike(t, 0.0004, 0.011)
                        + osc[1].sine(4100.) * strike(t, 0.0003, 0.006) * 0.5
                        + noise.next() * strike(t, 0.0002, 0.0015) * 0.3
                }
                MapSound::Laser => {
                    let pitch = 220. + 1700. * (-t / 0.07).exp();
                    osc[0].buzz(pitch, 6) * strike(t, 0.002, 0.16)
                        + osc[1].sine(pitch * 2.01) * strike(t, 0.001, 0.05) * 0.3
                }
                MapSound::PowerUp => {
                    // A rising sweep under a quick C major arpeggio.
                    let sweep = osc[0].buzz(260. * 2f32.powf(2.2 * smooth(t / 0.8)), 4)
                        * held(t, 0.02, 0.9, 0.2)
                        * 0.3;
                    let notes: f32 = [523.25, 659.25, 783.99, 1046.5]
                        .iter()
                        .enumerate()
                        .map(|(n, &pitch)| {
                            let t = t - n as f32 * 0.09;
                            osc[n + 1].sine(pitch)
                                * strike(t, 0.003, if n == 3 { 0.35 } else { 0.08 })
                        })
                        .sum();
                    sweep + notes * 0.7
                }
                MapSound::PowerDown => {
                    // A falling sweep that loses its footing, wobbling more
                    // as it goes, and a dull knock at the bottom.
                    let wobble = 1. + 0.04 * (t * 6.).min(1.) * (t * 38.).sin();
                    let pitch = 140. + 760. * (1. - smooth(t / 0.85)) * wobble;
                    osc[0].buzz(pitch, 5) * held(t, 0.01, 0.95, 0.25) * 0.6
                        + osc[1].sine(70. + 40. * (-(t - 0.9).max(0.) / 0.03).exp())
                            * strike(t - 0.9, 0.002, 0.08)
                }
                MapSound::Buzzer => {
                    // Two flat tones a touch apart, squared off and rasping.
                    // The second is quieter, so the pair roughens without
                    // ever cancelling out.
                    let a = (osc[0].sine(146.) * 4.).tanh();
                    let b = (osc[1].sine(147.5) * 4.).tanh();
                    (a + 0.55 * b) / 1.55 * held(t, 0.01, 0.6, 0.05)
                }
                MapSound::Success => {
                    let arpeggio: f32 = [523.25, 659.25, 783.99]
                        .iter()
                        .enumerate()
                        .map(|(n, &pitch)| {
                            osc[n].sine(pitch) * strike(t - n as f32 * 0.07, 0.003, 0.09)
                        })
                        .sum();
                    let chord: f32 = [1046.5, 1318.51, 1567.98]
                        .iter()
                        .map(|&pitch| bell(t - 0.22, pitch, 0.55))
                        .sum();
                    arpeggio * 0.6 + chord * 0.4 + bell(t - 0.34, 3135.96, 0.3) * 0.12
                }
                MapSound::Door => {
                    // Grinding along on its runners, then shutting hard.
                    let n = noise.next();
                    low += (n - low) * 0.04;
                    band += (low - band) * 0.01;
                    let grind =
                        (low - band) * 3. * held(t, 0.08, 0.9, 0.08) * (1. + 0.4 * (t * 23.).sin());
                    let slam = osc[0].sine(62. + 50. * (-(t - 0.88).max(0.) / 0.02).exp())
                        * strike(t - 0.88, 0.002, 0.14);
                    let clank = bell(t - 0.88, 420., 0.12) * 0.15;
                    grind + slam + clank + n * strike(t - 0.88, 0.0005, 0.01) * 0.4
                }
                MapSound::Explosion => {
                    let n = noise.next();
                    // The cutoff closes as the blast rolls away.
                    let open = 0.02 + 0.4 * (-t / 0.09).exp();
                    low += (n - low) * open;
                    deep += (low - deep) * 0.05;
                    let body = deep * 3.2 * strike(t, 0.003, 0.5);
                    let sub = osc[0].sine(30. + 30. * (-t / 0.15).exp()) * strike(t, 0.004, 0.45);
                    let crack = n * strike(t, 0.0003, 0.012) * 0.8;
                    let debris: f32 = crackles
                        .iter()
                        .map(|&at| noise_tick(t - at) * 0.25 * (-(at) / 0.9).exp())
                        .sum();
                    body + sub * 0.9 + crack + debris
                }
                MapSound::Teleport => {
                    let pitch = 380. * 2f32.powf(2. * smooth(t / 0.85));
                    let warble = 1. + 0.06 * osc[2].sine(24.);
                    (osc[0].sine(pitch * warble) + 0.5 * osc[1].sine(pitch * 1.5 * warble))
                        * held(t, 0.03, 0.95, 0.4)
                        + bell(t - 0.6, 2093., 0.25) * 0.15
                }
                MapSound::Magic => {
                    let run: f32 = [1046.5, 1174.66, 1318.51, 1567.98, 1760., 2093.]
                        .iter()
                        .enumerate()
                        .map(|(n, &pitch)| {
                            bell(t - n as f32 * 0.055, pitch, 0.45) * (1. - n as f32 * 0.08)
                        })
                        .sum();
                    let n = noise.next();
                    low += (n - low) * 0.5;
                    let shimmer = (n - low) * held(t, 0.1, 1.3, 0.6) * 0.08;
                    run * 0.5 + shimmer
                }
                MapSound::Splash => {
                    let n = noise.next();
                    low += (n - low) * 0.35;
                    band += (low - band) * 0.06;
                    let hiss = (low - band) * strike(t, 0.002, 0.22) * 1.6;
                    let plunk =
                        osc[0].sine(90. + 120. * (-t / 0.03).exp()) * strike(t, 0.002, 0.07);
                    let bubbling: f32 = bubbles
                        .iter()
                        .map(|&(at, pitch)| {
                            let t = t - at;
                            if (0. ..0.05).contains(&t) {
                                let rise = pitch * (1. + 1.2 * t / 0.05);
                                (std::f32::consts::TAU * rise * t).sin() * strike(t, 0.001, 0.015)
                            } else {
                                0.
                            }
                        })
                        .sum();
                    hiss + plunk * 0.8 + bubbling * 0.35
                }
                MapSound::Gong => {
                    // Inharmonic partials, each one a pair a hair apart so
                    // it shimmers as it rings.
                    let partials = [
                        (1., 1., 1.6),
                        (1.47, 0.6, 1.2),
                        (1.94, 0.45, 0.95),
                        (2.59, 0.35, 0.75),
                        (3.13, 0.25, 0.55),
                        (4.1, 0.15, 0.4),
                    ];
                    // Struck, and let go of gently over its last second.
                    let swell = smooth(t / 0.012) * smooth((seconds - t) / 1.);
                    partials
                        .iter()
                        .map(|&(ratio, level, decay)| {
                            let pitch = 98. * ratio;
                            let beat = (std::f32::consts::TAU * pitch * t).sin()
                                + (std::f32::consts::TAU * pitch * 1.004 * t).sin();
                            beat * 0.5 * level * (-t / decay).exp()
                        })
                        .sum::<f32>()
                        * swell
                }
                MapSound::Thunder => {
                    let n = noise.next();
                    low += (n - low) * 0.025;
                    deep += (low - deep) * 0.08;
                    // Slow, uneven swells as it rolls away.
                    let roll = 0.6 + 0.4 * ((t * 3.1).sin() * (t * 1.7 + 1.).sin()).abs();
                    let rumble = deep * 9. * roll * held(t, 0.05, 2.75, 1.6);
                    let crack = n * strike(t, 0.0003, 0.05);
                    rumble + crack * 0.9
                }
                MapSound::Countdown => {
                    let beep = |t: f32, length: f32| {
                        if (0. ..length).contains(&t) {
                            held(t, 0.004, length, 0.02)
                        } else {
                            0.
                        }
                    };
                    let short: f32 = [0., 0.75, 1.5].iter().map(|&at| beep(t - at, 0.13)).sum();
                    osc[0].sine(880.) * short + osc[1].sine(1760.) * beep(t - 2.25, 0.55)
                }
                MapSound::Drum => {
                    let pitch = 85. + 95. * (-t / 0.04).exp();
                    let n = noise.next();
                    low += (n - low) * 0.2;
                    osc[0].sine(pitch) * strike(t, 0.002, 0.2)
                        + low * strike(t, 0.0005, 0.03) * 0.8
                        + osc[1].sine(pitch * 1.6) * strike(t, 0.001, 0.06) * 0.25
                }
                MapSound::Horn => {
                    // Scooping up into A2, with the brassy middle harmonics
                    // strongest and a slow wobble as it holds.
                    let scoop = 1. - 0.08 * (-t / 0.05).exp();
                    let vibrato = 1.
                        + 0.006 * (t * 5.5 * std::f32::consts::TAU).sin() * smooth((t - 0.3) / 0.4);
                    let phase = osc[0].step(110. * scoop * vibrato) * std::f32::consts::TAU;
                    let brass: f32 = [1., 0.8, 0.9, 0.7, 0.5, 0.35, 0.22, 0.14]
                        .iter()
                        .enumerate()
                        .map(|(n, level)| (phase * (n + 1) as f32).sin() * level)
                        .sum();
                    brass * held(t, 0.05, 1.45, 0.18) * 0.35
                }
                _ => 0.,
            };
            value * tail
        })
        .collect()
}

/// A single grain of crackle: a few samples of a hard click.
fn noise_tick(t: f32) -> f32 {
    if (0. ..0.002).contains(&t) {
        let phase = t / 0.002;
        (phase * 37.).sin() * (1. - phase)
    } else {
        0.
    }
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

fn rail_synth(cue: RailCue) -> Vec<f32> {
    use std::f32::consts::TAU;
    let seconds = match cue {
        RailCue::Charge => 1.5,
        RailCue::Lock => 0.3,
        RailCue::Shot => 1.5,
        RailCue::Beam => 3.1,
        RailCue::Vent => 1.5,
        RailCue::Fall => 2.6,
        RailCue::ClashIon => 1.25,
        RailCue::ClashFire => 1.5,
        RailCue::ClashSteel => 1.05,
        RailCue::Backlash => 1.3,
    };
    let count = (seconds * RATE) as usize;
    let mut noise = Noise(0x51ed_270b);
    let (mut low, mut band) = (0_f32, 0_f32);
    // Phases advance sample by sample, so gliding pitches never jump.
    let mut phases = [0f32; 5];
    let mut turn = |voice: usize, pitch: f32| {
        phases[voice] = (phases[voice] + TAU * pitch / RATE) % TAU;
        phases[voice]
    };
    (0..count)
        .map(|i| {
            let t = i as f32 / RATE;
            let tail = ((seconds - t) / 0.03).min(1.);
            let value = match cue {
                RailCue::Charge => {
                    // Up two and a half octaves, faster toward the end.
                    let u = t / seconds;
                    let pitch = 150. * 2f32.powf(2.6 * u * u);
                    let wobble = 1. + 0.28 * (TAU * (9. + 30. * u) * t).sin() * u;
                    let tone = turn(0, pitch).sin() + 0.4 * turn(1, pitch * 2.01).sin();
                    let edge = (turn(2, pitch * 3.).sin() * 2.).tanh() * 0.2 * u;
                    (tone + edge) * wobble * (0.1 + 0.9 * u * u)
                }
                RailCue::Lock => [0., 0.14]
                    .iter()
                    .map(|&start| {
                        let local = t - start;
                        if local < 0. {
                            return 0.;
                        }
                        ((TAU * 1760. * local).sin() * 2.4).tanh() * strike(local, 0.002, 0.04)
                    })
                    .sum(),
                RailCue::Shot => {
                    let crack = noise.next() * strike(t, 0.0006, 0.018);
                    low += (noise.next() - low) * (0.03 + 0.4 * (-t / 0.05).exp());
                    let blast = low * strike(t, 0.001, 0.2) * 2.4;
                    let thud =
                        turn(0, 34. + 150. * (-t / 0.05).exp()).sin() * strike(t, 0.002, 0.34);
                    // The barrel ringing afterward.
                    let ring = turn(1, 1318.).sin() * strike(t, 0.001, 0.32) * 0.09
                        + turn(2, 2093.7).sin() * strike(t, 0.001, 0.2) * 0.07
                        + turn(3, 3322.).sin() * strike(t, 0.001, 0.1) * 0.05;
                    crack * 0.5 + blast + thud * 1.2 + ring
                }
                RailCue::Beam => {
                    let swell = (t / 0.12).min(1.) * ((seconds - t) / 0.35).clamp(0., 1.);
                    let flutter = 1. + 0.18 * (TAU * 31. * t).sin();
                    let drone = (turn(0, 82.4).sin() * 2.6).tanh()
                        + 0.6 * (turn(1, 123.6 * 1.004).sin() * 2.).tanh()
                        + 0.28 * turn(2, 329.6).sin();
                    low += (noise.next() - low) * 0.05;
                    (drone * flutter + low * 1.2) * swell
                }
                RailCue::Vent => {
                    let swell = (t / 0.03).min(1.) * ((seconds - t) / 1.1).clamp(0., 1.).powi(2);
                    low += (noise.next() - low) * (0.5 - 0.3 * t / seconds);
                    band += (low - band) * 0.03;
                    let thunk =
                        turn(0, 62. + 60. * (-t / 0.04).exp()).sin() * strike(t, 0.002, 0.1);
                    (low - band) * swell * 0.7 + thunk * 0.7
                }
                RailCue::Fall => {
                    low += (noise.next() - low) * (0.012 + 0.2 * (-t / 0.12).exp());
                    let roar = low * 4.2 * strike(t, 0.004, 0.75);
                    let thud =
                        turn(0, 28. + 110. * (-t / 0.09).exp()).sin() * strike(t, 0.003, 0.6);
                    let echo = turn(1, 41. + 50. * (-(t - 0.34).max(0.) / 0.07).exp()).sin()
                        * strike(t - 0.34, 0.004, 0.45)
                        * 0.6;
                    let glass = bell(t - 0.02, 1975.5, 0.5) * 0.08;
                    roar + thud * 1.2 + echo + glass
                }
                RailCue::ClashIon => {
                    let crack = noise.next() * strike(t, 0.0004, 0.014);
                    let pitch = 420. + 2400. * (-t / 0.22).exp();
                    let shriek = (turn(0, pitch).sin() * 3.).tanh() * strike(t, 0.001, 0.2) * 0.5;
                    // Two tones beating against each other, as the plasma tears.
                    let tear = turn(1, pitch * 1.503).sin()
                        * turn(2, 83.).sin()
                        * strike(t, 0.002, 0.3)
                        * 0.42;
                    let thump =
                        turn(3, 44. + 130. * (-t / 0.04).exp()).sin() * strike(t, 0.002, 0.26);
                    low += (noise.next() - low) * (0.08 + 0.4 * (-t / 0.04).exp());
                    let air = low * strike(t, 0.002, 0.42) * 1.4;
                    let ring = bell(t, 2637., 0.3) * 0.1;
                    crack * 0.85 + shriek + tear + thump * 1.2 + air + ring
                }
                RailCue::ClashFire => {
                    let crack = noise.next() * strike(t, 0.0005, 0.02) * 0.7;
                    low += (noise.next() - low) * (0.018 + 0.34 * (-t / 0.06).exp());
                    let roar = low * 4. * strike(t, 0.003, 0.48);
                    let whump =
                        turn(0, 30. + 140. * (-t / 0.055).exp()).sin() * strike(t, 0.002, 0.44);
                    // A burning tail: bursts of noise that thin out.
                    band += (noise.next() - band) * 0.7;
                    let crackle = if band.abs() > 0.58 {
                        noise.next() * strike(t - 0.05, 0.01, 0.42) * 0.42
                    } else {
                        0.
                    };
                    crack + roar + whump * 1.3 + crackle
                }
                RailCue::ClashSteel => {
                    let crack = noise.next() * strike(t, 0.0004, 0.01) * 0.8;
                    let clang = bell(t, 1174.7, 0.38) * 0.52 + bell(t - 0.003, 1760., 0.24) * 0.3;
                    let thud =
                        turn(0, 58. + 120. * (-t / 0.03).exp()).sin() * strike(t, 0.002, 0.16);
                    low += (noise.next() - low) * 0.26;
                    crack + clang + thud * 0.9 + low * strike(t, 0.002, 0.12) * 0.9
                }
                RailCue::Backlash => {
                    low += (noise.next() - low) * (0.022 + 0.26 * (-t / 0.08).exp());
                    let roar = low * 3.8 * strike(t, 0.004, 0.42);
                    let boom =
                        turn(0, 32. + 100. * (-t / 0.07).exp()).sin() * strike(t, 0.003, 0.46);
                    let second = turn(1, 40. + 60. * (-(t - 0.12).max(0.) / 0.05).exp()).sin()
                        * strike(t - 0.12, 0.003, 0.3)
                        * 0.6;
                    let metal = bell(t - 0.01, 932.3, 0.34) * 0.14;
                    roar + boom * 1.3 + second + metal
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
    fn rail_cues_are_finite_bounded_and_end_in_silence() {
        for cue in RailCue::ALL {
            let samples = rail_samples(cue);
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
