//! The line a round has really flown, smoothed into one curve, and the ribbons
//! of light laid along it.
//!
//! A ribbon is measured by distance from the round rather than by how many
//! points its trail holds, so laying a new point leaves its shape alone. It
//! swells out of nothing at the round and thins away to nothing at its tail,
//! so neither end shows an edge. Road older than [`LINGER`] seconds is let go,
//! so a round that slows or stops draws its trail back in after it.
use super::{Fuel, TRAIL_POINTS, Trail};
use crate::emission::{Emission, Layer, Stop};
use crate::render::View;
use macroquad::prelude::*;

/// Seconds a stretch of road stays lit after the round has left it.
pub(super) const LINGER: f32 = 0.42;
/// Sections along a ribbon, and how many lower shape quality spends.
pub(super) const FINE: usize = 15;
pub(super) const PLAIN: usize = 9;
/// The cosine of the sharpest turn the curve rounds off. Past it the round
/// bounced, and rounding the corner would swing the ribbon through the wall.
const CORNER: f32 = 0.3;
/// Room for the round itself, every point of its trail and where it gives out.
const MOST: usize = TRAIL_POINTS + 2;

/// Where a round has been, from the round backward, on screen.
#[derive(Clone, Copy, Debug)]
pub(super) struct Path {
    points: [Vec2; MOST],
    /// The way the path runs at each point, toward its tail. Zero at a
    /// corner, so the curve stops there and leaves the other way.
    ways: [Vec2; MOST],
    /// How far each point is from the round.
    along: [f32; MOST],
    count: usize,
}

impl Path {
    fn new(head: Vec2) -> Self {
        Self {
            points: [head; MOST],
            ways: [Vec2::ZERO; MOST],
            along: [0.; MOST],
            count: 1,
        }
    }

    /// A straight stretch behind `head`, for a round with no trail of its own.
    pub(super) fn straight(head: Vec2, way: Vec2, length: f32) -> Self {
        let mut path = Self::new(head);
        path.extend(head + way * length);
        path.finished(-way)
    }

    /// The road `trail` has laid, ending at `head`.
    pub(super) fn of(trail: &Trail, view: &View, head: Vec2, heading: Vec2) -> Self {
        Self::through(
            head,
            heading,
            (0..usize::from(trail.count))
                .map(|back| (view.screen(trail.point(back)), trail.age(back))),
        )
    }

    /// The path from `head` back through `points`, newest first, each with
    /// its age, up to where [`LINGER`] runs out.
    fn through(head: Vec2, heading: Vec2, points: impl Iterator<Item = (Vec2, f32)>) -> Self {
        let mut path = Self::new(head);
        let (mut newer, mut newer_age) = (head, 0.);
        for (at, age) in points {
            if age >= LINGER {
                let t = ((LINGER - newer_age) / (age - newer_age).max(1e-4)).clamp(0., 1.);
                path.extend(newer.lerp(at, t));
                break;
            }
            path.extend(at);
            (newer, newer_age) = (at, age);
        }
        path.finished(heading)
    }

    /// Add the next point back, unless it lies on the last one.
    fn extend(&mut self, at: Vec2) {
        if self.count >= MOST {
            return;
        }
        let gap = self.points[self.count - 1].distance(at);
        // Written so a NaN is refused too.
        if !(gap > 0.25 && gap.is_finite()) {
            return;
        }
        self.points[self.count] = at;
        self.along[self.count] = self.along[self.count - 1] + gap;
        self.count += 1;
    }

    /// Work out which way the curve runs through each point.
    fn finished(mut self, heading: Vec2) -> Self {
        if self.count < 2 {
            return self;
        }
        let last = self.count - 1;
        let chord = |path: &Self, i: usize| {
            (path.points[i + 1] - path.points[i]) / (path.along[i + 1] - path.along[i])
        };
        let first = chord(&self, 0);
        let back = -heading.try_normalize().unwrap_or(-first);
        // A round that bounced this frame is no longer flying along its road.
        self.ways[0] = if back.dot(first) > CORNER {
            back
        } else {
            first
        };
        for i in 1..last {
            let (into, out) = (chord(&self, i - 1), chord(&self, i));
            self.ways[i] = if into.dot(out) < CORNER {
                Vec2::ZERO
            } else {
                (into + out).normalize_or_zero()
            };
        }
        self.ways[last] = chord(&self, last - 1);
        self
    }

    /// How long the road is, in pixels.
    pub(super) fn length(&self) -> f32 {
        self.along[self.count - 1]
    }

    /// Where the curve is `distance` back from the round and the way it runs
    /// there. Before the round it carries straight on. `cursor` remembers the
    /// stretch reached, so walking down the path visits each only once.
    pub(super) fn at(&self, distance: f32, cursor: &mut usize) -> (Vec2, Vec2) {
        if self.count < 2 {
            return (self.points[0], vec2(-1., 0.));
        }
        if distance <= 0. {
            let way = self.ways[0];
            return (self.points[0] + way * distance, way);
        }
        while *cursor + 2 < self.count && self.along[*cursor + 1] < distance {
            *cursor += 1;
        }
        let i = (*cursor).min(self.count - 2);
        let span = (self.along[i + 1] - self.along[i]).max(1e-3);
        let t = ((distance - self.along[i]) / span).clamp(0., 1.);
        let (a, b) = (self.points[i], self.points[i + 1]);
        let (leave, arrive) = (self.ways[i] * span, self.ways[i + 1] * span);
        let (t2, t3) = (t * t, t * t * t);
        let at = a * (2. * t3 - 3. * t2 + 1.)
            + leave * (t3 - 2. * t2 + t)
            + b * (3. * t2 - 2. * t3)
            + arrive * (t3 - t2);
        let slope = a * (6. * t2 - 6. * t)
            + leave * (3. * t2 - 4. * t + 1.)
            + b * (6. * t - 6. * t2)
            + arrive * (3. * t2 - 2. * t);
        let way = slope
            .try_normalize()
            .or_else(|| (b - a).try_normalize())
            .unwrap_or(vec2(-1., 0.));
        (at, way)
    }

    /// The ribbon's middle: from `lead` ahead of the round to `reach` behind
    /// it, in `sections` even steps, swaying as `sway` says.
    pub(super) fn line(&self, lead: f32, reach: f32, sections: usize, sway: Sway) -> Line {
        let count = sections.clamp(2, FINE);
        let span = (lead + reach).max(1e-3);
        let mut spine = [(self.points[0], vec2(0., 1.)); FINE];
        let mut cursor = 0;
        for (index, slot) in spine[..count].iter_mut().enumerate() {
            let at = index as f32 / (count - 1) as f32 * span - lead;
            let (point, way) = self.at(at, &mut cursor);
            let across = vec2(-way.y, way.x);
            *slot = (
                point + across * sway.offset(at, (at / reach.max(1e-3)).clamp(0., 1.)),
                across,
            );
        }
        Line {
            spine,
            count,
            lead,
            span,
        }
    }
}

/// How a ribbon writhes: a wave running down it from the round, nothing at
/// the round and growing toward the tail.
#[derive(Clone, Copy, Debug)]
pub(super) struct Sway {
    size: f32,
    wavelength: f32,
    rate: f32,
    clock: f32,
}

impl Sway {
    /// No movement, for reduced motion.
    pub(super) const STILL: Self = Self {
        size: 0.,
        wavelength: 1.,
        rate: 0.,
        clock: 0.,
    };

    /// The way a round's fuel moves, for a round of radius `r`.
    pub(super) fn of(fuel: Fuel, r: f32, clock: f32) -> Self {
        let (size, wavelength, rate) = match fuel {
            Fuel::Fire => (0.34, 3.2, 11.),
            Fuel::Arc => (0.3, 1.8, 24.),
            Fuel::Void => (0.26, 3.7, 5.5),
            Fuel::Spectrum => (0.17, 4.4, 7.),
            Fuel::Star => (0.16, 3.9, 8.),
            Fuel::Plasma => (0.11, 3.5, 6.5),
        };
        Self {
            size: size * r,
            wavelength: (wavelength * r).max(6.),
            rate,
            clock,
        }
    }

    fn offset(self, at: f32, out: f32) -> f32 {
        if self.size <= 0. {
            return 0.;
        }
        let phase = at / self.wavelength - self.clock * self.rate;
        let wave = (phase.sin() + 0.45 * (phase * 2.3 + 1.7).sin()) / 1.45;
        self.size * wave * out.powf(1.35)
    }
}

/// One step of a ribbon: its half width, its light, and how far to one side
/// of the middle it runs.
#[derive(Clone, Copy, Debug)]
pub(super) struct Section {
    pub half: f32,
    pub color: Color,
    pub side: f32,
}

/// The middle of a ribbon, ready for bands of light along it.
#[derive(Clone, Copy, Debug)]
pub(super) struct Line {
    spine: [(Vec2, Vec2); FINE],
    count: usize,
    lead: f32,
    span: f32,
}

impl Line {
    /// Lay a band along the line. `shape` gets how far down the ribbon each
    /// step is, from 0 to 1, and its distance back from the round.
    pub(super) fn band(
        &self,
        light: &mut Emission,
        layer: Layer,
        shape: impl Fn(f32, f32) -> Section,
    ) {
        let clear = Stop {
            at: Vec2::ZERO,
            half: 0.,
            color: Color::new(0., 0., 0., 0.),
        };
        let mut stops = [clear; FINE];
        for (index, (stop, &(at, across))) in
            stops.iter_mut().zip(&self.spine[..self.count]).enumerate()
        {
            let t = index as f32 / (self.count - 1) as f32;
            let section = shape(t, t * self.span - self.lead);
            *stop = Stop {
                at: at + across * section.side,
                half: section.half.max(0.),
                color: section.color,
            };
        }
        light.band(layer, &stops[..self.count]);
    }
}

/// A shape's width, color and brightness through a ribbon, as (distance back,
/// half width, heat, strength) keys in order, eased between them.
pub(super) fn keyed(keys: &[(f32, f32, f32, f32)], at: f32) -> (f32, f32, f32) {
    let after = keys
        .iter()
        .position(|key| key.0 >= at)
        .unwrap_or(keys.len() - 1)
        .max(1);
    let (a, b) = (keys[after - 1], keys[after]);
    let t = ((at - a.0) / (b.0 - a.0).max(1e-4)).clamp(0., 1.);
    let t = t * t * (3. - 2. * t);
    (
        a.1 + (b.1 - a.1) * t,
        a.2 + (b.2 - a.2) * t,
        a.3 + (b.3 - a.3) * t,
    )
}

/// How far a ribbon has opened at `t`, where `start` is where it's fully
/// open. It starts from nothing, so the end at the round has no edge.
pub(super) fn swell(t: f32, start: f32) -> f32 {
    let u = (t / start.max(1e-3)).clamp(0., 1.);
    u * u * (3. - 2. * u)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Points every `gap` back along a circle of `radius` from the round.
    fn curve(head_angle: f32, radius: f32, gap: f32, count: usize) -> Vec<(Vec2, f32)> {
        (0..count)
            .map(|i| {
                let turn = head_angle - (i as f32 + 1.) * gap / radius;
                (vec2(turn.cos(), turn.sin()) * radius, i as f32 * 0.01)
            })
            .collect()
    }

    fn at(path: &Path, distance: f32) -> Vec2 {
        path.at(distance, &mut 0).0
    }

    #[test]
    fn a_new_point_leaves_the_ribbon_where_it_was() {
        let radius = 200.;
        let head = vec2(radius, 0.);
        let heading = vec2(0., 1.);
        let points = curve(0., radius, 12., TRAIL_POINTS);
        let with = Path::through(head, heading, points.iter().copied());
        let without = Path::through(head, heading, points[1..].iter().copied());
        for step in 1..=16 {
            let distance = step as f32 * 6.;
            let gap = at(&with, distance).distance(at(&without, distance));
            assert!(gap < 0.35, "{gap} at {distance}");
            // And it lies on the real circle, not on a straight chord.
            assert!((at(&with, distance).length() - radius).abs() < 0.12);
        }
    }

    #[test]
    fn the_curve_goes_through_every_point() {
        let head = vec2(200., 0.);
        let points = curve(0., 200., 12., 8);
        let path = Path::through(head, vec2(0., 1.), points.iter().copied());
        for index in 0..path.count {
            let p = at(&path, path.along[index]);
            assert!(p.distance(path.points[index]) < 0.01);
        }
    }

    #[test]
    fn a_bounce_turns_on_the_spot_instead_of_swinging_through_the_wall() {
        // A round that came down to the wall at y = 0 and bounced back up.
        let head = vec2(40., 30.);
        let points = [
            (vec2(30., 20.), 0.01),
            (vec2(20., 10.), 0.02),
            (vec2(10., 0.), 0.03),
            (vec2(0., 10.), 0.04),
            (vec2(-10., 20.), 0.05),
        ];
        let path = Path::through(head, vec2(1., 1.), points.into_iter());
        let mut cursor = 0;
        for step in 0..=100 {
            let distance = path.length() * step as f32 / 100.;
            let (p, way) = path.at(distance, &mut cursor);
            assert!(p.y >= -0.01, "crossed the wall: {p}");
            assert!(p.is_finite() && way.is_finite());
        }
        assert!(at(&path, path.along[3]).distance(vec2(10., 0.)) < 0.01);
    }

    #[test]
    fn a_stopped_round_draws_its_trail_in() {
        let head = vec2(0., 0.);
        let points = |aged: f32| {
            (1..=10)
                .map(move |i| (vec2(i as f32 * -10., 0.), aged + i as f32 * 0.01))
                .collect::<Vec<_>>()
        };
        let moving = Path::through(head, vec2(1., 0.), points(0.).into_iter());
        assert!((moving.length() - 100.).abs() < 0.01);
        let halfway = Path::through(head, vec2(1., 0.), points(LINGER - 0.05).into_iter());
        assert!(halfway.length() < 60., "{}", halfway.length());
        let gone = Path::through(head, vec2(1., 0.), points(LINGER + 1.).into_iter());
        assert!(gone.length() < 4., "{}", gone.length());
    }

    #[test]
    fn broken_input_stays_finite() {
        let bad = [
            (vec2(f32::NAN, 1.), 0.),
            (Vec2::ZERO, 0.01),
            (Vec2::ZERO, 0.02),
            (vec2(f32::INFINITY, 0.), 0.03),
            (vec2(3., 0.), 0.04),
        ];
        for heading in [Vec2::ZERO, vec2(f32::NAN, 0.), vec2(0., 5.)] {
            let path = Path::through(Vec2::ZERO, heading, bad.into_iter());
            let line = path.line(
                4.,
                path.length().max(1.),
                FINE,
                Sway::of(Fuel::Fire, 8., 1.),
            );
            for &(p, across) in &line.spine[..line.count] {
                assert!(p.is_finite() && across.is_finite(), "{p} {across}");
            }
        }
        // One point alone is no road at all, and asking along it is harmless.
        let lone = Path::new(vec2(5., 5.));
        assert_eq!(lone.length(), 0.);
        assert_eq!(at(&lone, 10.), vec2(5., 5.));
    }

    #[test]
    fn a_ribbon_is_anchored_at_the_round_and_writhes_toward_its_tail() {
        let path = Path::straight(Vec2::ZERO, vec2(-1., 0.), 100.);
        let sway = Sway::of(Fuel::Fire, 10., 0.7);
        let line = path.line(0., 100., FINE, sway);
        assert!(line.spine[0].0.distance(Vec2::ZERO) < 1e-3);
        let furthest = line.spine[..line.count]
            .iter()
            .map(|&(p, _)| p.y.abs())
            .fold(0., f32::max);
        assert!(furthest > 0.5 && furthest <= 3.5, "{furthest}");
        // It moves with time.
        let later = path.line(0., 100., FINE, Sway::of(Fuel::Fire, 10., 0.9));
        assert!(line.spine[10].0.distance(later.spine[10].0) > 0.05);
        // Held still, it's a straight line.
        let still = path.line(0., 100., FINE, Sway::STILL);
        assert!(still.spine[..still.count].iter().all(|&(p, _)| p.y == 0.));
    }

    #[test]
    fn both_ends_of_a_ribbon_fade_from_nothing() {
        assert_eq!(swell(0., 0.2), 0.);
        assert_eq!(swell(0.2, 0.2), 1.);
        assert_eq!(swell(0.9, 0.2), 1.);
        let keys = [
            (-5., 1., 0.2, 0.),
            (0., 4., 0.3, 1.),
            (10., 2., 0.5, 0.5),
            (20., 0., 0.9, 0.),
        ];
        assert_eq!(keyed(&keys, -9.).2, 0.);
        assert_eq!(keyed(&keys, 30.).2, 0.);
        assert!((keyed(&keys, 0.).2 - 1.).abs() < 1e-5);
        let mid = keyed(&keys, 5.);
        assert!(mid.0 > 2. && mid.0 < 4. && mid.2 > 0.5 && mid.2 < 1.);
    }
}
