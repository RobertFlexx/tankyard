//! Offline processing for recorded sound effects: a damped room, gentle
//! equalization, a look-ahead compressor and short moving delays. Clips are
//! prepared and cached before playback; the audio callback only mixes samples.
use std::collections::VecDeque;

pub const RATE: f32 = 48_000.;
pub fn samples_in(seconds: f32) -> usize {
    (seconds.max(0.) * RATE) as usize
}

struct LowPass {
    a: f32,
    b: f32,
    c1: f32,
    c2: f32,
    c3: f32,
}
impl LowPass {
    fn new(hz: f32) -> Self {
        let g = (std::f32::consts::PI * hz.clamp(10., RATE * 0.48) / RATE).tan();
        let c1 = 1. / (1. + g * (g + std::f32::consts::SQRT_2));
        Self {
            a: 0.,
            b: 0.,
            c1,
            c2: g * c1,
            c3: g * g * c1,
        }
    }
    fn run(&mut self, x: f32) -> f32 {
        let v3 = x - self.b;
        let v1 = self.c1 * self.a + self.c2 * v3;
        let v2 = self.b + self.c2 * self.a + self.c3 * v3;
        self.a = 2. * v1 - self.a;
        self.b = 2. * v2 - self.b;
        v2
    }
}

/// A delay line for the room.
struct Delay {
    line: Vec<f32>,
    at: usize,
}
impl Delay {
    fn new(length: usize) -> Self {
        Self {
            line: vec![0.; length.max(1)],
            at: 0,
        }
    }
    fn read(&self) -> f32 {
        self.line[self.at]
    }
    fn write(&mut self, value: f32) {
        self.line[self.at] = value;
        self.at = (self.at + 1) % self.line.len();
    }
}

/// The space a sound happens in.
#[derive(Clone, Copy, Debug)]
pub struct Room {
    /// How long the tail rings, about: 0.3 a small room, 0.9 a hall, more
    /// a cavern.
    pub size: f32,
    /// How much the walls swallow the highs, 0 to 1.
    pub damp: f32,
    /// How much of the room is heard against the sound itself.
    pub wet: f32,
    /// Seconds before the first reflection comes back.
    pub gap: f32,
}
impl Room {
    pub const SMALL: Self = Self {
        size: 0.45,
        damp: 0.5,
        wet: 0.16,
        gap: 0.008,
    };
    pub const HALL: Self = Self {
        size: 0.8,
        damp: 0.42,
        wet: 0.26,
        gap: 0.02,
    };
    pub const ARENA: Self = Self {
        size: 0.88,
        damp: 0.5,
        wet: 0.3,
        gap: 0.035,
    };
    pub const CAVERN: Self = Self {
        size: 0.93,
        damp: 0.62,
        wet: 0.36,
        gap: 0.05,
    };
}

/// Play `dry` in `room`, adding `tail` seconds for the room to die away. A
/// mono room: combs in parallel for the body of the reverberation, allpasses
/// in series to smear it, and a few early reflections in front.
pub fn in_room(dry: &[f32], room: Room, tail: f32) -> Vec<f32> {
    const COMBS: [usize; 8] = [1557, 1617, 1491, 1422, 1277, 1356, 1188, 1116];
    const PASSES: [usize; 4] = [225, 556, 441, 341];
    const EARLY: [(f32, f32); 5] = [
        (0.0, 0.6),
        (0.0071, 0.42),
        (0.0113, 0.33),
        (0.0173, 0.24),
        (0.0249, 0.17),
    ];
    // Delay lengths are tuned for 44.1 kHz; scale them to ours.
    let scale = RATE / 44_100.;
    let mut combs: Vec<(Delay, f32)> = COMBS
        .iter()
        .map(|&length| (Delay::new((length as f32 * scale) as usize), 0.))
        .collect();
    let mut passes: Vec<Delay> = PASSES
        .iter()
        .map(|&length| Delay::new((length as f32 * scale) as usize))
        .collect();
    let feedback = 0.7 + 0.28 * room.size.clamp(0., 1.);
    let damp = room.damp.clamp(0., 0.95);
    let gap = (room.gap * RATE) as usize;
    let total = dry.len() + samples_in(tail);
    let mut out = Vec::with_capacity(total);
    let input = |i: usize| -> f32 { if i < dry.len() { dry[i] } else { 0. } };
    for i in 0..total {
        // Early reflections off near walls.
        let mut early = 0.;
        for &(delay, level) in &EARLY {
            let back = gap + (delay * RATE) as usize;
            if i >= back {
                early += input(i - back) * level;
            }
        }
        let feed = if i >= gap { input(i - gap) } else { 0. } * 0.015;
        let mut wet = 0.;
        for (line, low) in combs.iter_mut() {
            let echo = line.read();
            *low = echo * (1. - damp) + *low * damp;
            line.write(feed + *low * feedback);
            wet += echo;
        }
        for line in passes.iter_mut() {
            let echo = line.read();
            let value = -wet + echo;
            line.write(wet + echo * 0.5);
            wet = value;
        }
        let dry_now = input(i);
        out.push(dry_now * (1. - room.wet * 0.5) + (wet * 1.4 + early * 0.25) * room.wet);
    }
    out
}

/// Pulls the loudest moments down toward the rest of the sound, the way a
/// finished mix is, so a hit reads as dense and heavy rather than as one
/// spike. `squeeze` is how many decibels the peaks come down.
pub fn limit(samples: &mut [f32], squeeze: f32) {
    let peak = samples.iter().fold(0f32, |peak, s| peak.max(s.abs()));
    if peak <= 0. || squeeze <= 0. {
        return;
    }
    let ceiling = peak * 10f32.powf(-squeeze / 20.);
    // The loudest sample in the next two milliseconds, for each sample, so
    // the gain is already down when a peak arrives.
    let ahead = samples_in(0.002).max(1);
    let mut loudest = vec![0f32; samples.len()];
    let mut window: VecDeque<(usize, f32)> = VecDeque::new();
    for index in (0..samples.len()).rev() {
        let level = samples[index].abs();
        while window.back().is_some_and(|&(_, back)| back <= level) {
            window.pop_back();
        }
        window.push_back((index, level));
        while window.front().is_some_and(|&(at, _)| at > index + ahead) {
            window.pop_front();
        }
        loudest[index] = window.front().map_or(level, |&(_, front)| front);
    }
    let attack = (-3. / ahead as f32).exp();
    let release = (-1. / (0.08 * RATE)).exp();
    let mut gain = 1f32;
    for (sample, &level) in samples.iter_mut().zip(&loudest) {
        let want = if level > ceiling { ceiling / level } else { 1. };
        let rate = if want < gain { attack } else { release };
        gain = want + (gain - want) * rate;
        *sample *= gain;
    }
}

/// Turns everything below `hz` up or down by `gain` (0.5 halves it), so a
/// sound meant for laptop speakers too isn't all rumble they can't play.
pub fn low_shelf(samples: &mut [f32], hz: f32, gain: f32) {
    let mut filter = LowPass::new(hz);
    for sample in samples.iter_mut() {
        let low = filter.run(*sample);
        *sample += low * (gain - 1.);
    }
}

/// A short delay that moves, mixed back with what went in: the sweeping
/// sheen of a jet going over, or of a beam.
pub struct Flanger {
    line: Vec<f32>,
    at: usize,
}
impl Flanger {
    pub fn new() -> Self {
        Self {
            line: vec![0.; 1024],
            at: 0,
        }
    }
    /// `delay` in seconds, up to about 20 milliseconds; `feedback` from 0
    /// to 0.9 sharpens the sweep.
    pub fn run(&mut self, x: f32, delay: f32, feedback: f32) -> f32 {
        let length = self.line.len();
        let back = (delay * RATE).clamp(1., length as f32 - 2.);
        let read = (self.at as f32 + length as f32 - back) % length as f32;
        let index = read.floor() as usize;
        let frac = read - index as f32;
        let delayed =
            self.line[index % length] * (1. - frac) + self.line[(index + 1) % length] * frac;
        self.line[self.at] = x + delayed * feedback.clamp(0., 0.9);
        self.at = (self.at + 1) % length;
        (x + delayed) * 0.5
    }
}

/// Ready a sound to keep: its DC taken out, a little rumble below hearing
/// trimmed, its end faded so it never clicks off.
pub fn finish(mut samples: Vec<f32>, fade: f32) -> Vec<f32> {
    let (mut last_in, mut last_out) = (0., 0.);
    for sample in samples.iter_mut() {
        let value = *sample - last_in + 0.9985 * last_out;
        last_in = *sample;
        last_out = value;
        *sample = if value.is_finite() { value } else { 0. };
    }
    let count = samples.len();
    let fade = samples_in(fade).clamp(1, count.max(1));
    for (n, sample) in samples.iter_mut().rev().take(fade).enumerate() {
        let u = n as f32 / fade as f32;
        *sample *= u * u;
    }
    // And a fade in over the first two milliseconds, against a click.
    for (n, sample) in samples.iter_mut().take(96).enumerate() {
        *sample *= n as f32 / 96.;
    }
    samples
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn a_room_tail_dies_away_and_finishing_removes_edge_clicks() {
        let mut dry = vec![0.; 4800];
        dry[0] = 1.;
        let wet = in_room(&dry, Room::HALL, 2.);
        let middle: f32 = wet[24_000..30_000].iter().map(|s| s.abs()).sum();
        let end: f32 = wet[wet.len() - 6000..].iter().map(|s| s.abs()).sum();
        assert!(middle > 0. && end < middle * 0.2);
        assert!(wet.iter().all(|s| s.is_finite()));
        let finished = finish(vec![0.5; 48_000], 0.05);
        assert_eq!(finished[0], 0.);
        assert_eq!(*finished.last().unwrap(), 0.);
    }
    #[test]
    fn processing_preserves_finite_samples_and_reduces_extreme_peaks() {
        let mut samples = vec![0.1; 48000];
        samples[24000] = 1.;
        low_shelf(&mut samples, 180., 0.7);
        limit(&mut samples, 9.);
        assert!(samples.iter().all(|x| x.is_finite() && x.abs() < 0.6));
        let mut flange = Flanger::new();
        for n in 0..48000 {
            let x = flange.run(
                if n == 0 { 1. } else { 0. },
                0.001 + n as f32 / 48000. * 0.003,
                0.38,
            );
            assert!(x.is_finite() && x.abs() <= 1.);
        }
    }
}
