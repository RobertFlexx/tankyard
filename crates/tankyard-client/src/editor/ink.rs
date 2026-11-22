//! Smooth strokes for the workshop: lines, curves and small shapes with a
//! soft one-pixel edge, gathered and sent as one batch. They stay clean at any
//! angle, where a plain line steps along its edge.
use macroquad::{models::Vertex, prelude::*};

/// Vertices gathered before they go to the renderer, well under its limit.
const BATCH: usize = 16_000;
/// How wide a stroke's soft edge is, in pixels.
const FEATHER: f32 = 1.;
/// Most sides on a disc or ring.
const MOST_SIDES: usize = 48;

fn vertex(p: Vec2, color: Color) -> Vertex {
    crate::lighting::vertex(p, color)
}
fn clear(color: Color) -> Color {
    Color { a: 0., ..color }
}
fn between(a: Color, b: Color, t: f32) -> Color {
    Color::new(
        a.r + (b.r - a.r) * t,
        a.g + (b.g - a.g) * t,
        a.b + (b.b - a.b) * t,
        a.a + (b.a - a.a) * t,
    )
}
fn sides(radius: f32) -> usize {
    ((radius * 2.2).ceil() as usize).clamp(10, MOST_SIDES)
}

/// The point `t` of the way along a curve from `a` to `b` bending toward `via`.
pub fn on_curve(a: Vec2, via: Vec2, b: Vec2, t: f32) -> Vec2 {
    let u = 1. - t;
    a * (u * u) + via * (2. * u * t) + b * (t * t)
}

/// The middle point that bows a curve from `a` to `b` to one side, by `bow`
/// of its length.
pub fn bowed(a: Vec2, b: Vec2, bow: f32) -> Vec2 {
    a.lerp(b, 0.5) + (b - a).perp() * bow
}

/// How far from the middle of a stroke `width` wide its solid part and its
/// soft edge reach, and how strongly a stroke under a pixel wide shows.
fn stroke_edges(width: f32) -> (f32, f32, f32) {
    let width = width.max(0.);
    let half = width * 0.5;
    let solid = (half - FEATHER * 0.5).max(0.);
    let edge = half + FEATHER * 0.5;
    let strength = if width < FEATHER {
        2. * width / (width + FEATHER)
    } else {
        1.
    };
    (solid, edge, strength)
}

#[derive(Default)]
pub struct Ink {
    vertices: Vec<Vertex>,
    indices: Vec<u16>,
}

impl Ink {
    fn room(&mut self, count: usize) {
        if self.vertices.len() + count > BATCH {
            self.flush();
        }
    }

    /// Sends what's gathered to the renderer.
    pub fn flush(&mut self) {
        if !self.vertices.is_empty() {
            crate::lighting::geometry(&self.vertices, &self.indices);
            self.vertices.clear();
            self.indices.clear();
        }
    }

    /// A band through `count` points, each with its own width and color.
    fn ribbon(&mut self, count: usize, closed: bool, point: impl Fn(usize) -> (Vec2, f32, Color)) {
        if count < 2 {
            return;
        }
        self.room(count * 4);
        let first = self.vertices.len() as u16;
        for i in 0..count {
            let (at, width, color) = point(i);
            let before = if closed {
                (i + count - 1) % count
            } else {
                i.saturating_sub(1)
            };
            let after = if closed {
                (i + 1) % count
            } else {
                (i + 1).min(count - 1)
            };
            let into = (at - point(before).0).normalize_or_zero();
            let out = (point(after).0 - at).normalize_or_zero();
            let edge = if out == Vec2::ZERO { into } else { out };
            let mean = (into + out).normalize_or_zero();
            let along = if mean == Vec2::ZERO { edge } else { mean };
            let side = along.perp();
            // At a corner the band widens just enough to keep its thickness.
            let miter = 1. / side.dot(edge.perp()).max(0.45);
            let (solid, soft, strength) = stroke_edges(width);
            let full = Color {
                a: color.a * strength,
                ..color
            };
            self.vertices.extend([
                vertex(at + side * soft * miter, clear(color)),
                vertex(at + side * solid * miter, full),
                vertex(at - side * solid * miter, full),
                vertex(at - side * soft * miter, clear(color)),
            ]);
        }
        let segments = if closed { count } else { count - 1 };
        for segment in 0..segments {
            let near = first + segment as u16 * 4;
            let far = first + ((segment + 1) % count) as u16 * 4;
            for band in 0..3 {
                self.indices.extend([
                    near + band,
                    near + band + 1,
                    far + band,
                    near + band + 1,
                    far + band + 1,
                    far + band,
                ]);
            }
        }
    }

    pub fn line(&mut self, a: Vec2, b: Vec2, width: f32, color: Color) {
        self.ribbon(2, false, |i| (if i == 0 { a } else { b }, width, color));
    }

    /// One stroke through `points`.
    pub fn path(&mut self, points: &[Vec2], width: f32, color: Color) {
        self.ribbon(points.len(), false, |i| (points[i], width, color));
    }

    /// One stroke through `points` and back to the first.
    pub fn closed(&mut self, points: &[Vec2], width: f32, color: Color) {
        self.ribbon(points.len(), true, |i| (points[i], width, color));
    }

    /// A curve from `a` to `b` bending toward `via`, its width and color
    /// running from the first of each pair to the second.
    pub fn curve(
        &mut self,
        (a, via, b): (Vec2, Vec2, Vec2),
        width: (f32, f32),
        color: (Color, Color),
    ) {
        let length = a.distance(via) + via.distance(b);
        let steps = ((length / 22.).ceil() as usize).clamp(2, 18);
        self.ribbon(steps + 1, false, |i| {
            let t = i as f32 / steps as f32;
            (
                on_curve(a, via, b, t),
                width.0 + (width.1 - width.0) * t,
                between(color.0, color.1, t),
            )
        });
    }

    /// A shape every point of which can be seen from its middle, as convex
    /// shapes and stars can, with a soft edge.
    pub fn fill(&mut self, points: &[Vec2], color: Color) {
        let count = points.len();
        if count < 3 {
            return;
        }
        self.room(1 + count * 2);
        let middle = points.iter().copied().sum::<Vec2>() / count as f32;
        let turn: f32 = (0..count)
            .map(|i| points[i].perp_dot(points[(i + 1) % count]))
            .sum();
        // Which side of each edge is outward, by the way the points wind.
        let outward = if turn >= 0. { -1. } else { 1. };
        let first = self.vertices.len() as u16;
        self.vertices.push(vertex(middle, color));
        for i in 0..count {
            let at = points[i];
            let into = (at - points[(i + count - 1) % count]).normalize_or_zero();
            let out = (points[(i + 1) % count] - at).normalize_or_zero();
            let (a, b) = (into.perp() * outward, out.perp() * outward);
            let normal = (a + b).normalize_or_zero();
            let reach = FEATHER * 0.5 / normal.dot(b).max(0.5);
            self.vertices.push(vertex(at - normal * reach, color));
            self.vertices
                .push(vertex(at + normal * reach, clear(color)));
        }
        for i in 0..count as u16 {
            let next = (i + 1) % count as u16;
            let (solid, soft) = (first + 1 + i * 2, first + 2 + i * 2);
            let (next_solid, next_soft) = (first + 1 + next * 2, first + 2 + next * 2);
            self.indices.extend([
                first, solid, next_solid, solid, soft, next_solid, soft, next_soft, next_solid,
            ]);
        }
    }

    pub fn disc(&mut self, at: Vec2, radius: f32, color: Color) {
        let count = sides(radius);
        let mut points = [Vec2::ZERO; MOST_SIDES];
        for (i, point) in points.iter_mut().take(count).enumerate() {
            let turn = i as f32 / count as f32 * std::f32::consts::TAU;
            *point = at + vec2(turn.cos(), turn.sin()) * radius;
        }
        self.fill(&points[..count], color);
    }

    pub fn ring(&mut self, at: Vec2, radius: f32, width: f32, color: Color) {
        let count = sides(radius);
        self.ribbon(count, true, |i| {
            let turn = i as f32 / count as f32 * std::f32::consts::TAU;
            (at + vec2(turn.cos(), turn.sin()) * radius, width, color)
        });
    }

    /// An arrowhead with its tip at `tip`, pointing along `along`.
    pub fn head(&mut self, tip: Vec2, along: Vec2, size: f32, color: Color) {
        let back = tip - along * size;
        let side = along.perp() * size * 0.52;
        self.fill(&[tip, back + side, back - side], color);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strokes_keep_their_width_at_any_size() {
        // The solid part and half of the soft edge add up to the width, and
        // a hairline shows only as strongly as it is wide.
        for width in [1., 1.5, 2., 4., 9.] {
            let (solid, edge, strength) = stroke_edges(width);
            assert_eq!(strength, 1.);
            assert!((solid + edge - width).abs() < 1e-4, "{width}");
        }
        let (solid, edge, strength) = stroke_edges(0.5);
        assert_eq!(solid, 0.);
        assert!((edge * strength - 0.5).abs() < 1e-4);
        assert_eq!(stroke_edges(-3.).2, 0.);
    }

    #[test]
    fn a_curve_runs_end_to_end_through_its_bend() {
        let (a, b) = (vec2(0., 0.), vec2(100., 0.));
        let via = bowed(a, b, 0.2);
        assert_eq!(via, vec2(50., 20.));
        assert_eq!(on_curve(a, via, b, 0.), a);
        assert_eq!(on_curve(a, via, b, 1.), b);
        // Halfway along it has come half the way to its middle point.
        assert!(on_curve(a, via, b, 0.5).distance(vec2(50., 10.)) < 1e-4);
    }

    #[test]
    fn shapes_gather_whole_triangles_within_the_batch() {
        let mut ink = Ink::default();
        ink.line(vec2(0., 0.), vec2(10., 0.), 2., WHITE);
        ink.fill(&[vec2(0., 0.), vec2(9., 0.), vec2(0., 9.)], WHITE);
        ink.ring(vec2(5., 5.), 40., 2., WHITE);
        ink.curve(
            (vec2(0., 0.), vec2(40., 30.), vec2(90., 0.)),
            (1., 3.),
            (WHITE, BLACK),
        );
        assert_eq!(ink.indices.len() % 3, 0);
        assert!(
            ink.indices
                .iter()
                .all(|&index| usize::from(index) < ink.vertices.len())
        );
        // Too few points to draw leave nothing behind.
        let before = ink.vertices.len();
        ink.path(&[vec2(1., 1.)], 2., WHITE);
        ink.fill(&[vec2(1., 1.), vec2(2., 2.)], WHITE);
        assert_eq!(ink.vertices.len(), before);
    }
}
