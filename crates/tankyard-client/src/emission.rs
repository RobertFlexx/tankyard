//! Light laid over the finished arena.
//!
//! Everything that burns, flashes or glows queues here while the world is
//! drawn, and the whole queue goes down in one batch after the darkness. So a
//! round keeps its brightness on a night map, a hundred glows cost one draw
//! call, and the order things happened to be drawn in can't cut a halo off.
//!
//! Pieces are built from the falloff in [`crate::glow`]: a spot is a round
//! falloff, and a band is that same falloff taken across a ribbon, so trails,
//! beams, rings and arcs all share one soft edge and one shader. The light
//! response follows the arena's brightness, keeping color on pale floors.
use crate::glow::{self, Blend};
use macroquad::models::Vertex;
use macroquad::prelude::*;
use std::cell::RefCell;
use std::f32::consts::TAU;

/// Most pieces one frame keeps; past this the rest are dropped.
const MOST_PIECES: usize = 6144;
/// Most band points one frame keeps.
const MOST_STOPS: usize = 40_960;
/// Most steps around a ring.
const MOST_STEPS: usize = 96;
/// Vertices gathered before they go to the renderer, well under its limit.
const BATCH: usize = 20_000;
/// Most pieces looked at to see whether a frame needs a cover map.
const MOST_CHECKS: usize = 1024;
/// Fainter than this can't show in an 8-bit frame.
const FAINTEST: f32 = 0.004;
/// Limit on how far a band corner can extend relative to its half width.
const MIN_MITER: f32 = 0.45;

/// Which part of the pass a piece belongs to, drawn in this order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Layer {
    /// Smoke and soot, which darken what is under them.
    Smoke,
    /// Broad soft light around a source.
    Wash,
    /// The bright source itself.
    Core,
}

/// One point along a band: where it runs, half its width, and its light.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Stop {
    pub at: Vec2,
    pub half: f32,
    pub color: Color,
}

#[derive(Clone, Copy, Debug)]
enum Piece {
    Spot {
        at: Vec2,
        radius: f32,
        color: Color,
    },
    Oval {
        at: Vec2,
        along: Vec2,
        across: f32,
        color: Color,
    },
    Band {
        from: u32,
        to: u32,
    },
    Ring {
        at: Vec2,
        radius: f32,
        half: f32,
        start: f32,
        sweep: f32,
        color: Color,
    },
    Outline {
        at: Vec2,
        sides: u8,
        radius: f32,
        angle: f32,
        half: f32,
        color: Color,
    },
}

#[derive(Default)]
struct Mesh {
    vertices: Vec<Vertex>,
    indices: Vec<u16>,
    /// The cover map this batch is held back by, and the window it spans.
    cover: Option<(Texture2D, f32, f32)>,
}

impl Mesh {
    /// Make room for `count` more vertices, sending what's gathered so far
    /// when it wouldn't fit.
    fn room(&mut self, count: usize) {
        if self.vertices.len() + count > BATCH {
            self.submit();
        }
    }
    fn submit(&mut self) {
        if !self.vertices.is_empty() {
            if let Some((_, w, h)) = self.cover {
                for vertex in &mut self.vertices {
                    let (x, y) = (vertex.position.x, vertex.position.y);
                    vertex.normal = vec4(x / w, 1. - y / h, 0., 0.);
                }
            }
            crate::lighting::textured_geometry(
                &self.vertices,
                &self.indices,
                self.cover.as_ref().map(|(texture, ..)| texture),
            );
            self.vertices.clear();
            self.indices.clear();
        }
    }
    fn quad(&mut self, corners: [(Vec2, f32, f32); 4], color: Color) {
        self.room(4);
        let first = self.vertices.len() as u16;
        for (p, u, v) in corners {
            self.vertices.push(Vertex::new(p.x, p.y, 0., u, v, color));
        }
        self.indices
            .extend([0, 1, 2, 0, 2, 3].map(|index| first + index));
    }
    fn spot(&mut self, at: Vec2, radius: f32, color: Color) {
        let (x, y) = (vec2(radius, 0.), vec2(0., radius));
        self.quad(
            [
                (at - x - y, 0., 0.),
                (at + x - y, 1., 0.),
                (at + x + y, 1., 1.),
                (at - x + y, 0., 1.),
            ],
            color,
        );
    }
    fn oval(&mut self, at: Vec2, along: Vec2, across: f32, color: Color) {
        let side = vec2(-along.y, along.x).normalize_or_zero() * across;
        self.quad(
            [
                (at - along - side, 0., 0.),
                (at + along - side, 1., 0.),
                (at + along + side, 1., 1.),
                (at - along + side, 0., 1.),
            ],
            color,
        );
    }
    /// Two rows of vertices: one edge at `v` 0, the other at 1. The falloff
    /// is then taken straight across the ribbon.
    fn pair(&mut self, a: Vec2, b: Vec2, color: Color) {
        self.vertices
            .push(Vertex::new(a.x, a.y, 0., 0.5, 0., color));
        self.vertices
            .push(Vertex::new(b.x, b.y, 0., 0.5, 1., color));
    }
    fn strip(&mut self, first: u16, sections: usize) {
        for section in 0..sections as u16 {
            let at = first + section * 2;
            self.indices
                .extend([at, at + 1, at + 3, at, at + 3, at + 2]);
        }
    }
    fn band(&mut self, stops: &[Stop]) {
        if stops.len() < 2 {
            return;
        }
        self.room(stops.len() * 2);
        let first = self.vertices.len() as u16;
        for (index, stop) in stops.iter().enumerate() {
            let before = stops[index.saturating_sub(1)].at;
            let after = stops[(index + 1).min(stops.len() - 1)].at;
            let into = (stop.at - before).normalize_or_zero();
            let out = (after - stop.at).normalize_or_zero();
            let mut tangent = (into + out).normalize_or_zero();
            if tangent == Vec2::ZERO {
                tangent = if out == Vec2::ZERO { into } else { out };
            }
            let normal = vec2(-tangent.y, tangent.x);
            // Keep the band its full width around a bend, within reason.
            let reference = if out == Vec2::ZERO { into } else { out };
            let miter = normal
                .dot(vec2(-reference.y, reference.x))
                .abs()
                .max(MIN_MITER);
            let offset = normal * (stop.half / miter);
            self.pair(stop.at - offset, stop.at + offset, stop.color);
        }
        self.strip(first, stops.len() - 1);
    }
    /// A ring or part of one. `fades` softens the two ends of an arc; the
    /// visible slice of a whole ring runs off the window and needs none.
    #[allow(clippy::too_many_arguments)]
    fn ring(
        &mut self,
        at: Vec2,
        radius: f32,
        half: f32,
        start: f32,
        sweep: f32,
        color: Color,
        fades: bool,
    ) {
        let steps = ring_steps(radius, sweep, fades);
        self.room((steps + 1) * 2);
        let first = self.vertices.len() as u16;
        let (inner, outer) = ((radius - half).max(0.), radius + half);
        for step in 0..=steps {
            let t = step as f32 / steps as f32;
            let angle = start + sweep * t;
            let direction = vec2(angle.cos(), angle.sin());
            let shade = if fades {
                (t.min(1. - t) / 0.2).clamp(0., 1.)
            } else {
                1.
            };
            self.pair(
                at + direction * inner,
                at + direction * outer,
                Color {
                    a: color.a * shade,
                    ..color
                },
            );
        }
        self.strip(first, steps);
    }
}

fn ring_steps(radius: f32, sweep: f32, fades: bool) -> usize {
    let span = sweep.abs().min(TAU) / TAU;
    (((radius.max(1.).sqrt() * 5.5 * span) as usize).max(if fades { 6 } else { 16 }))
        .min(MOST_STEPS)
}

/// A conservative radial bound on a polygonal ring's hole. The straight
/// segments lie inside the true circle, so radius - half alone is too large.
fn ring_hole(radius: f32, half: f32, sweep: f32) -> f32 {
    let full = sweep.abs() >= TAU - 1e-3;
    let steps = ring_steps(radius, sweep, !full);
    // A full ring can be clipped to a facing arc. Allow for rounding its
    // section count down, which can lengthen a section slightly.
    let steps = if full { steps - 1 } else { steps };
    let angle = sweep.abs().min(TAU) / steps as f32;
    (radius - half).max(0.) * (angle * 0.5).cos().max(0.)
}

fn outline_radii(sides: u8, radius: f32, half: f32) -> (f32, f32) {
    let cosine = (std::f32::consts::PI / f32::from(sides.max(3))).cos();
    ((radius * cosine - half).max(0.), radius + half / cosine)
}

impl Mesh {
    /// A closed band along a regular polygon, its corners mitered so it
    /// keeps its width all the way around.
    fn outline(&mut self, at: Vec2, sides: u8, radius: f32, angle: f32, half: f32, color: Color) {
        let count = usize::from(sides.max(3));
        let apothem = radius * (std::f32::consts::PI / count as f32).cos();
        if apothem <= 0. {
            return;
        }
        let inner = ((apothem - half) / apothem).max(0.);
        let outer = (apothem + half) / apothem;
        self.room((count + 1) * 2);
        let first = self.vertices.len() as u16;
        for corner in 0..=count {
            let turn = angle + corner as f32 * TAU / count as f32;
            let point = vec2(turn.cos(), turn.sin()) * radius;
            self.pair(at + point * inner, at + point * outer, color);
        }
        self.strip(first, count);
    }
}

/// The part of a whole ring worth building: on a ring far bigger than the
/// window, just the arc facing it, so its steps stay too short to see.
fn facing_arc(at: Vec2, radius: f32, w: f32, h: f32) -> (f32, f32) {
    let middle = vec2(w, h) * 0.5;
    let reach = middle.length() + 8.;
    let to_window = middle - at;
    let distance = to_window.length();
    if radius < reach || distance <= reach {
        return (0., TAU);
    }
    let half = (reach / distance).clamp(0., 1.).asin() * 1.08;
    (to_window.y.atan2(to_window.x) - half, half * 2.)
}

fn finite(p: Vec2) -> bool {
    p.x.is_finite() && p.y.is_finite()
}
fn shows(color: Color) -> bool {
    color.a.is_finite() && color.a > FAINTEST
}
/// Whether a shape reaching `reach` from `at` touches a `w` by `h` window.
fn on_screen(at: Vec2, reach: f32, w: f32, h: f32) -> bool {
    at.x + reach > 0. && at.y + reach > 0. && at.x - reach < w && at.y - reach < h
}

/// One frame's light. Queue it with the methods below, then [`Emission::flush`]
/// it after the darkness, with no clip set beyond the arena's own.
#[derive(Default)]
pub struct Emission {
    smoke: Vec<Piece>,
    wash: Vec<Piece>,
    core: Vec<Piece>,
    stops: Vec<Stop>,
    mesh: Mesh,
    daylight: f32,
    detail: u8,
    /// How many pieces of each layer went in before the tanks: smoke, wash
    /// and core. Those lie under the tanks, and a tank on top hides them.
    under: [usize; 3],
}

thread_local! {
    /// The queue's storage, kept between frames so drawing allocates nothing.
    static SPARE: RefCell<Emission> = RefCell::new(Emission::default());
}

impl Emission {
    /// A clear queue with the scene's daylight response and shape quality.
    pub fn begin(daylight: f32, detail: u8) -> Self {
        let mut emission = SPARE.with(|spare| std::mem::take(&mut *spare.borrow_mut()));
        emission.clear();
        emission.daylight = daylight;
        emission.detail = detail;
        emission
    }
    fn clear(&mut self) {
        self.smoke.clear();
        self.wash.clear();
        self.core.clear();
        self.stops.clear();
        self.under = [0; 3];
    }
    /// The tanks are next: what's queued so far lies under them.
    pub fn tanks_next(&mut self) {
        self.under = [self.smoke.len(), self.wash.len(), self.core.len()];
    }
    /// Whether this frame has any light that tank silhouettes could hide.
    pub fn has_under(&self) -> bool {
        self.under.iter().any(|&count| count > 0)
    }
    /// Whether any light under the tanks reaches one of `tanks`, given as
    /// where each stands and how far it reaches. When none does, this frame
    /// needs no cover map at all.
    pub fn under_tanks(&self, tanks: impl Iterator<Item = (Vec2, f32)> + Clone) -> bool {
        if tanks.clone().next().is_none() {
            return false;
        }
        let pieces = self.smoke[..self.under[0]]
            .iter()
            .chain(&self.wash[..self.under[1]])
            .chain(&self.core[..self.under[2]]);
        for (index, piece) in pieces.enumerate() {
            // Looking any longer would cost more than the map it could save.
            if index >= MOST_CHECKS {
                return true;
            }
            if tanks
                .clone()
                .any(|(at, reach)| touches(piece, &self.stops, at, reach))
            {
                return true;
            }
        }
        false
    }
    /// Whether to spend pieces on the extra layers of an effect.
    pub fn rich(&self) -> bool {
        self.detail >= 2
    }
    /// How much the floor needs a tinted glow to show its edges.
    pub fn daylight(&self) -> f32 {
        self.daylight
    }
    /// `color` heated by `amount`, 0 to 1. White-hot on a dark floor; on a
    /// pale one, where white wouldn't show, it stays well inside the color.
    pub fn hot(&self, color: Color, amount: f32) -> Color {
        heated(color, amount, self.daylight)
    }
    fn push(&mut self, layer: Layer, piece: Piece) {
        let list = match layer {
            Layer::Smoke => &mut self.smoke,
            Layer::Wash => &mut self.wash,
            Layer::Core => &mut self.core,
        };
        if list.len() < MOST_PIECES {
            list.push(piece);
        }
    }
    /// A round soft light.
    pub fn spot(&mut self, layer: Layer, at: Vec2, radius: f32, color: Color) {
        if finite(at) && radius.is_finite() && radius >= 0.25 && shows(color) {
            self.push(layer, Piece::Spot { at, radius, color });
        }
    }
    /// A soft light stretched along `along`, which is half its length, and
    /// `across` half its width.
    pub fn oval(&mut self, layer: Layer, at: Vec2, along: Vec2, across: f32, color: Color) {
        if finite(at)
            && finite(along)
            && along.length_squared() >= 0.01
            && across.is_finite()
            && across >= 0.1
            && shows(color)
        {
            self.push(
                layer,
                Piece::Oval {
                    at,
                    along,
                    across,
                    color,
                },
            );
        }
    }
    /// A soft band of light through `stops`, each with its own width and color.
    pub fn band(&mut self, layer: Layer, stops: &[Stop]) {
        if stops.len() < 2
            || self.stops.len() + stops.len() > MOST_STOPS
            || stops
                .iter()
                .any(|stop| !finite(stop.at) || !stop.half.is_finite() || stop.half < 0.)
            || !stops.iter().any(|stop| shows(stop.color))
        {
            return;
        }
        let from = self.stops.len() as u32;
        self.stops.extend_from_slice(stops);
        let to = self.stops.len() as u32;
        self.push(layer, Piece::Band { from, to });
    }
    /// A tapering streak from `tail`, which fades to nothing, up to `head`.
    pub fn streak(
        &mut self,
        layer: Layer,
        tail: Vec2,
        head: Vec2,
        half: f32,
        tail_color: Color,
        head_color: Color,
    ) {
        let along = head - tail;
        let cap = along.normalize_or_zero() * half * 0.8;
        let clear = |color: Color| Color { a: 0., ..color };
        self.band(
            layer,
            &[
                Stop {
                    at: tail,
                    half: half * 0.3,
                    color: clear(tail_color),
                },
                Stop {
                    at: tail + along * 0.45,
                    half: half * 0.68,
                    color: mix_color(tail_color, head_color, 0.5),
                },
                Stop {
                    at: head,
                    half,
                    color: head_color,
                },
                Stop {
                    at: head + cap,
                    half: half * 0.45,
                    color: clear(head_color),
                },
            ],
        );
    }
    /// A soft band of even width from `from` to `to`, fading out at both ends.
    pub fn beam(&mut self, layer: Layer, from: Vec2, to: Vec2, half: f32, color: Color) {
        let along = to - from;
        let length = along.length();
        if length < 0.5 {
            return;
        }
        let d = along / length;
        let fade = (half * 1.2).min(length * 0.3);
        let clear = Color { a: 0., ..color };
        self.band(
            layer,
            &[
                Stop {
                    at: from,
                    half: half * 0.55,
                    color: clear,
                },
                Stop {
                    at: from + d * fade,
                    half,
                    color,
                },
                Stop {
                    at: to - d * fade,
                    half,
                    color,
                },
                Stop {
                    at: to,
                    half: half * 0.55,
                    color: clear,
                },
            ],
        );
    }
    /// A whole soft ring.
    pub fn ring(&mut self, layer: Layer, at: Vec2, radius: f32, half: f32, color: Color) {
        self.arc(layer, at, radius, half, 0., TAU, color);
    }
    /// Part of a ring from `start`, `sweep` radians around, fading at its ends.
    #[allow(clippy::too_many_arguments)]
    pub fn arc(
        &mut self,
        layer: Layer,
        at: Vec2,
        radius: f32,
        half: f32,
        start: f32,
        sweep: f32,
        color: Color,
    ) {
        if finite(at)
            && radius.is_finite()
            && half.is_finite()
            && start.is_finite()
            && sweep.is_finite()
            && radius >= 0.5
            && half >= 0.1
            && sweep != 0.
            && shows(color)
        {
            self.push(
                layer,
                Piece::Ring {
                    at,
                    radius,
                    half,
                    start,
                    sweep,
                    color,
                },
            );
        }
    }
    /// A soft line along a regular polygon's outline, matching one drawn
    /// with these `sides`, `radius` and `angle`.
    #[allow(clippy::too_many_arguments)]
    pub fn outline(
        &mut self,
        layer: Layer,
        at: Vec2,
        sides: u8,
        radius: f32,
        angle: f32,
        half: f32,
        color: Color,
    ) {
        if finite(at)
            && radius.is_finite()
            && angle.is_finite()
            && half.is_finite()
            && radius >= 0.5
            && half >= 0.1
            && shows(color)
        {
            self.push(
                layer,
                Piece::Outline {
                    at,
                    sides,
                    radius,
                    angle,
                    half,
                    color,
                },
            );
        }
    }
    /// The thin crossed streaks over something very bright.
    pub fn glint(&mut self, at: Vec2, length: f32, turn: f32, color: Color) {
        let along = vec2(turn.cos(), turn.sin()) * length;
        let thin = (length * 0.05).max(0.6);
        self.oval(Layer::Core, at, along, thin, color);
        self.oval(
            Layer::Core,
            at,
            vec2(-along.y, along.x) * 0.62,
            thin,
            Color {
                a: color.a * 0.8,
                ..color
            },
        );
    }
    /// Lay the queue down and keep its storage. With a `cover` map, what
    /// was queued before the tanks is held back where a tank stands on it.
    pub fn flush(mut self, cover: Option<Texture2D>) {
        let (w, h) = (screen_width(), screen_height());
        let _daylight = glow::daylight(self.daylight);
        if glow::available() {
            let mut from = [0; 3];
            if let Some(cover) = cover {
                self.mesh.cover = Some((cover, w, h));
                let [smoke, wash, core] = self.under;
                if smoke > 0 && glow::begin(Blend::CoveredSoft) {
                    self.draw(Layer::Smoke, 0..smoke, w, h);
                    glow::end();
                    from[0] = smoke;
                }
                if (wash > 0 || core > 0) && glow::begin(Blend::CoveredGlow) {
                    self.draw(Layer::Wash, 0..wash, w, h);
                    self.draw(Layer::Core, 0..core, w, h);
                    glow::end();
                    from[1] = wash;
                    from[2] = core;
                }
                self.mesh.cover = None;
            }
            let [smoke, wash, core] = from;
            if self.smoke.len() > smoke && glow::begin(Blend::Soft) {
                self.draw(Layer::Smoke, smoke..self.smoke.len(), w, h);
                glow::end();
            }
            if (self.wash.len() > wash || self.core.len() > core) && glow::begin(Blend::Glow) {
                self.draw(Layer::Wash, wash..self.wash.len(), w, h);
                self.draw(Layer::Core, core..self.core.len(), w, h);
                glow::end();
            }
        } else {
            self.draw_flat();
        }
        self.clear();
        SPARE.with(|spare| *spare.borrow_mut() = self);
    }
    fn draw(&mut self, layer: Layer, range: std::ops::Range<usize>, w: f32, h: f32) {
        let Self {
            smoke,
            wash,
            core,
            stops,
            mesh,
            ..
        } = self;
        let pieces = match layer {
            Layer::Smoke => smoke,
            Layer::Wash => wash,
            Layer::Core => core,
        };
        for piece in &pieces[range] {
            if visible(piece, stops, w, h) {
                add_piece(mesh, stops, piece, w, h);
            }
        }
        mesh.submit();
    }
    /// Without shaders: thin flat strokes and stacked discs, so that what
    /// players must see still shows.
    fn draw_flat(&self) {
        for piece in self.wash.iter().chain(&self.core) {
            match *piece {
                Piece::Spot { at, radius, color } => glow::fallback_spot(at, radius, color),
                Piece::Oval {
                    at, along, color, ..
                } => draw_line(
                    at.x - along.x,
                    at.y - along.y,
                    at.x + along.x,
                    at.y + along.y,
                    1.5,
                    Color {
                        a: color.a * 0.6,
                        ..color
                    },
                ),
                Piece::Band { from, to } => {
                    for pair in self.stops[from as usize..to as usize].windows(2) {
                        draw_line(
                            pair[0].at.x,
                            pair[0].at.y,
                            pair[1].at.x,
                            pair[1].at.y,
                            (pair[1].half * 0.7).max(1.),
                            Color {
                                a: pair[1].color.a * 0.6,
                                ..pair[1].color
                            },
                        );
                    }
                }
                Piece::Ring {
                    at,
                    radius,
                    half,
                    start,
                    sweep,
                    color,
                } => draw_arc(
                    at.x,
                    at.y,
                    48,
                    radius,
                    start.to_degrees(),
                    (half * 0.6).max(1.),
                    sweep.to_degrees(),
                    Color {
                        a: color.a * 0.7,
                        ..color
                    },
                ),
                Piece::Outline {
                    at,
                    sides,
                    radius,
                    angle,
                    half,
                    color,
                } => draw_poly_lines(
                    at.x,
                    at.y,
                    sides.max(3),
                    radius,
                    angle.to_degrees(),
                    (half * 0.6).max(1.),
                    Color {
                        a: color.a * 0.7,
                        ..color
                    },
                ),
            }
        }
    }
}

/// `color` heated by `amount`, 0 to 1; see [`Emission::hot`]. For code that
/// holds the queue mutably while it works out its colors.
pub fn heated(color: Color, amount: f32, daylight: f32) -> Color {
    let amount = amount.clamp(0., 1.) * (1. - 0.66 * daylight);
    Color::new(
        color.r + (1. - color.r) * amount,
        color.g + (1. - color.g) * amount,
        color.b + (1. - color.b) * amount,
        color.a,
    )
}

fn mix_color(a: Color, b: Color, t: f32) -> Color {
    Color::new(
        a.r + (b.r - a.r) * t,
        a.g + (b.g - a.g) * t,
        a.b + (b.b - a.b) * t,
        a.a + (b.a - a.a) * t,
    )
}

/// Whether `piece` reaches a circle of `reach` around `at`.
fn touches(piece: &Piece, stops: &[Stop], at: Vec2, reach: f32) -> bool {
    let near = |center: Vec2, radius: f32| center.distance_squared(at) < (radius + reach).powi(2);
    match *piece {
        Piece::Spot {
            at: center, radius, ..
        } => near(center, radius),
        Piece::Oval {
            at: center,
            along,
            across,
            ..
        } => near(center, along.length() + across),
        Piece::Ring {
            at: center,
            radius,
            half,
            sweep,
            ..
        } => {
            // A ring lights its rim alone, so a tank inside a wide one is clear.
            let distance = center.distance(at);
            distance < radius + half + reach && distance + reach > ring_hole(radius, half, sweep)
        }
        Piece::Outline {
            at: center,
            sides,
            radius,
            half,
            ..
        } => {
            // The sides lie inside the corner radius, and their mitered
            // corners reach farther out than a circular ring of this width.
            let (inner, outer) = outline_radii(sides, radius, half);
            let distance = center.distance(at);
            distance < outer + reach && distance + reach > inner
        }
        Piece::Band { from, to } => stops[from as usize..to as usize].windows(2).any(|pair| {
            let (a, b) = (pair[0].at, pair[1].at);
            let along = b - a;
            let t = ((at - a).dot(along) / along.length_squared().max(1e-3)).clamp(0., 1.);
            // Include the same corner extension as Mesh::band. Missing a
            // miter here would let it shine through the edge of a tank.
            let half = pair[0].half.max(pair[1].half) / MIN_MITER;
            (a + along * t).distance_squared(at) < (half + reach).powi(2)
        }),
    }
}

fn visible(piece: &Piece, stops: &[Stop], w: f32, h: f32) -> bool {
    match *piece {
        Piece::Spot { at, radius, .. } => on_screen(at, radius, w, h),
        Piece::Oval {
            at, along, across, ..
        } => on_screen(at, along.length() + across, w, h),
        Piece::Ring {
            at, radius, half, ..
        } => {
            // A ring around the whole window has nothing to show either.
            let reach = radius + half;
            on_screen(at, reach, w, h)
                && (radius - half <= 0.
                    || [vec2(0., 0.), vec2(w, 0.), vec2(0., h), vec2(w, h)]
                        .iter()
                        .any(|&corner| corner.distance_squared(at) >= (radius - half).powi(2)))
        }
        Piece::Outline {
            at,
            sides,
            radius,
            half,
            ..
        } => on_screen(at, outline_radii(sides, radius, half).1, w, h),
        Piece::Band { from, to } => {
            stops[from as usize..to as usize].windows(2).any(|pair| {
                // A segment can cross the window with neither endpoint
                // inside it, including ribbons with many sections.
                let half = pair[0].half.max(pair[1].half) / MIN_MITER;
                let lo = pair[0].at.min(pair[1].at) - half;
                let hi = pair[0].at.max(pair[1].at) + half;
                hi.x > 0. && hi.y > 0. && lo.x < w && lo.y < h
            })
        }
    }
}

fn add_piece(mesh: &mut Mesh, stops: &[Stop], piece: &Piece, w: f32, h: f32) {
    match *piece {
        Piece::Spot { at, radius, color } => mesh.spot(at, radius, color),
        Piece::Oval {
            at,
            along,
            across,
            color,
        } => mesh.oval(at, along, across, color),
        Piece::Band { from, to } => mesh.band(&stops[from as usize..to as usize]),
        Piece::Ring {
            at,
            radius,
            half,
            start,
            sweep,
            color,
        } => {
            if sweep.abs() >= TAU - 1e-3 {
                let (start, sweep) = facing_arc(at, radius, w, h);
                mesh.ring(at, radius, half, start, sweep, color, false);
            } else {
                mesh.ring(at, radius, half, start, sweep, color, true);
            }
        }
        Piece::Outline {
            at,
            sides,
            radius,
            angle,
            half,
            color,
        } => mesh.outline(at, sides, radius, angle, half, color),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn queue() -> Emission {
        Emission {
            detail: 2,
            ..Default::default()
        }
    }
    fn build(emission: &Emission) -> Mesh {
        let mut mesh = Mesh::default();
        for piece in emission.wash.iter().chain(&emission.core) {
            add_piece(&mut mesh, &emission.stops, piece, 1920., 1080.);
        }
        mesh
    }

    #[test]
    fn a_ring_much_larger_than_the_window_builds_only_the_arc_facing_it() {
        let (w, h) = (1920., 1080.);
        // Small, or around the window: the whole ring.
        assert_eq!(facing_arc(vec2(300., 300.), 50., w, h), (0., TAU));
        assert_eq!(facing_arc(vec2(960., 540.), 3000., w, h), (0., TAU));
        // A huge safe zone centered far off to the left of the window.
        let at = vec2(-5000., 540.);
        let (start, sweep) = facing_arc(at, 5600., w, h);
        assert!(sweep < 0.6, "{sweep}");
        // The arc covers where the ring crosses the window.
        let crossing = (vec2(600., 540.) - at).to_angle();
        assert!(crossing > start && crossing < start + sweep);
        // Its steps are short enough to read as a curve.
        let step = 5600. * sweep / MOST_STEPS as f32;
        assert!(5600. * (1. - (step / 5600. / 2.).cos()) < 0.2);
    }

    #[test]
    fn every_piece_builds_finite_geometry_with_valid_indices() {
        let mut light = queue();
        let at = vec2(100., 100.);
        light.spot(Layer::Wash, at, 20., WHITE);
        light.oval(Layer::Core, at, vec2(30., 5.), 6., RED);
        light.beam(Layer::Core, at, vec2(400., 220.), 6., WHITE);
        light.streak(Layer::Core, at, vec2(40., 12.), 3., RED, WHITE);
        light.ring(Layer::Wash, at, 60., 4., WHITE);
        light.arc(Layer::Core, at, 60., 4., 1., -2.5, WHITE);
        light.glint(at, 30., 0.4, WHITE);
        light.outline(Layer::Wash, at, 6, 60., 0.3, 4., WHITE);
        light.outline(Layer::Core, at, 0, 40., 1.1, 2., RED);
        // A band that doubles straight back on itself must stay finite too.
        let stop = |x: f32| Stop {
            at: vec2(x, 0.),
            half: 2.,
            color: WHITE,
        };
        light.band(Layer::Wash, &[stop(0.), stop(10.), stop(0.), stop(0.)]);
        let mesh = build(&light);
        assert!(!mesh.vertices.is_empty());
        assert!(mesh.indices.len().is_multiple_of(3));
        assert!(
            mesh.indices
                .iter()
                .all(|&index| (index as usize) < mesh.vertices.len())
        );
        assert!(
            mesh.vertices
                .iter()
                .all(|v| v.position.x.is_finite() && v.position.y.is_finite())
        );
    }

    #[test]
    fn bad_or_invisible_input_queues_nothing() {
        let mut light = queue();
        let nan = f32::NAN;
        let at = vec2(1., 1.);
        light.spot(Layer::Wash, vec2(nan, 0.), 10., WHITE);
        light.spot(Layer::Wash, at, nan, WHITE);
        light.spot(Layer::Wash, at, 10., Color::new(1., 1., 1., 0.));
        light.oval(Layer::Core, at, vec2(0., 0.), 3., WHITE);
        light.ring(Layer::Core, at, f32::INFINITY, 2., WHITE);
        light.arc(Layer::Core, at, 10., 2., 0., 0., WHITE);
        light.band(
            Layer::Core,
            &[Stop {
                at,
                half: 2.,
                color: WHITE,
            }],
        );
        assert!(light.wash.is_empty() && light.core.is_empty() && light.stops.is_empty());
    }

    #[test]
    fn a_frame_cannot_queue_without_limit() {
        let mut light = queue();
        for _ in 0..MOST_PIECES * 2 {
            light.spot(Layer::Core, vec2(1., 1.), 2., WHITE);
            light.beam(Layer::Wash, vec2(0., 0.), vec2(50., 0.), 2., WHITE);
        }
        assert!(light.core.len() <= MOST_PIECES);
        assert!(light.wash.len() <= MOST_PIECES);
        assert!(light.stops.len() <= MOST_STOPS);
    }

    #[test]
    fn arcs_fade_at_their_ends_and_rings_do_not() {
        let mut mesh = Mesh::default();
        mesh.ring(vec2(0., 0.), 50., 3., 0., 1.5, WHITE, true);
        assert_eq!(mesh.vertices.first().unwrap().color[3], 0);
        assert_eq!(mesh.vertices.last().unwrap().color[3], 0);
        let mut mesh = Mesh::default();
        mesh.ring(vec2(0., 0.), 50., 3., 0., TAU, WHITE, false);
        assert!(mesh.vertices.iter().all(|v| v.color[3] == 255));
    }

    #[test]
    fn heat_goes_to_white_on_a_dark_floor_but_keeps_its_color_on_a_pale_one() {
        let color = Color::new(1., 0.2, 0.1, 1.);
        let dark = queue();
        assert_eq!(dark.hot(color, 1.), Color::new(1., 1., 1., 1.));
        let pale = Emission {
            daylight: 1.,
            ..queue()
        };
        let tinted = pale.hot(color, 1.);
        assert!(tinted.g < 0.55 && tinted.b < 0.5, "{tinted:?}");
    }

    #[test]
    fn only_light_that_reaches_a_tank_asks_for_a_cover_map() {
        let tank = (vec2(500., 500.), 30.);
        let check = |light: &Emission| light.under_tanks([tank].into_iter());
        let mut light = queue();
        assert!(!check(&light));
        light.spot(Layer::Wash, vec2(100., 100.), 40., WHITE);
        light.tanks_next();
        assert!(!check(&light), "a glow across the arena");
        // Something on top of the tanks doesn't count.
        light.spot(Layer::Core, vec2(500., 500.), 40., WHITE);
        assert!(!check(&light), "queued after the tanks");
        let mut light = queue();
        light.spot(Layer::Wash, vec2(480., 480.), 20., WHITE);
        light.tanks_next();
        assert!(check(&light), "a glow under the tank");
        // A long beam with no point near the tank still crosses it.
        let mut light = queue();
        light.beam(Layer::Core, vec2(0., 500.), vec2(1200., 500.), 3., WHITE);
        light.tanks_next();
        assert!(check(&light));
        // A wide ring around the tank lights only its rim.
        let mut light = queue();
        light.ring(Layer::Wash, vec2(500., 500.), 300., 4., WHITE);
        light.tanks_next();
        assert!(!check(&light));
        let mut light = queue();
        light.ring(Layer::Wash, vec2(500., 200.), 300., 4., WHITE);
        light.tanks_next();
        assert!(check(&light), "its rim runs under the tank");
    }

    #[test]
    fn a_new_frame_forgets_what_lay_under_the_tanks() {
        let mut light = queue();
        light.spot(Layer::Wash, vec2(1., 1.), 4., WHITE);
        light.tanks_next();
        assert_eq!(light.under, [0, 1, 0]);
        light.clear();
        assert_eq!(light.under, [0; 3]);
        assert!(!light.under_tanks([(vec2(1., 1.), 9.)].into_iter()));
    }

    #[test]
    fn overlap_checks_include_miters_and_the_inside_of_polygon_rims() {
        let mut light = queue();
        let stop = |x, y| Stop {
            at: vec2(x, y),
            half: 12.,
            color: WHITE,
        };
        light.band(
            Layer::Wash,
            &[stop(-100., 0.), stop(0., 0.), stop(-100., 40.)],
        );
        light.outline(Layer::Wash, vec2(300., 300.), 3, 100., 0., 3., WHITE);
        light.ring(Layer::Wash, vec2(600., 600.), 100., 1., WHITE);
        for piece in &light.wash {
            let mut mesh = Mesh::default();
            add_piece(&mut mesh, &light.stops, piece, 1920., 1080.);
            for indices in mesh.indices.as_chunks::<3>().0 {
                let center = indices
                    .iter()
                    .map(|&i| mesh.vertices[usize::from(i)].position.truncate())
                    .sum::<Vec2>()
                    / 3.;
                assert!(
                    touches(piece, &light.stops, center, 0.01),
                    "missed emitted geometry at {center}: {piece:?}"
                );
            }
        }
    }

    #[test]
    fn a_long_ribbon_crossing_the_window_needs_no_stop_inside_it() {
        let mut light = queue();
        let stops: Vec<_> = (0..15)
            .map(|i| Stop {
                at: vec2(if i % 2 == 0 { -100. } else { 300. }, 50.),
                half: 2.,
                color: WHITE,
            })
            .collect();
        light.band(Layer::Wash, &stops);
        assert!(visible(&light.wash[0], &light.stops, 200., 100.));
        light.tanks_next();
        assert!(light.under_tanks([(vec2(100., 50.), 5.)].into_iter()));
    }

    #[test]
    fn an_empty_arena_never_builds_a_cover_map_even_when_effects_are_busy() {
        let mut light = queue();
        for _ in 0..MOST_CHECKS + 1 {
            light.spot(Layer::Wash, Vec2::ZERO, 10., WHITE);
        }
        light.tanks_next();
        assert!(!light.under_tanks(std::iter::empty()));
    }

    #[test]
    fn a_piece_off_the_window_is_skipped_but_a_beam_across_it_is_not() {
        let spot = Piece::Spot {
            at: vec2(-100., 50.),
            radius: 20.,
            color: WHITE,
        };
        assert!(!visible(&spot, &[], 800., 600.));
        let across = [
            Stop {
                at: vec2(-500., 300.),
                half: 3.,
                color: WHITE,
            },
            Stop {
                at: vec2(1500., 300.),
                half: 3.,
                color: WHITE,
            },
        ];
        assert!(visible(
            &Piece::Band { from: 0, to: 2 },
            &across,
            800.,
            600.
        ));
        // A ring wider than the window with the window inside its hole.
        let hole = Piece::Ring {
            at: vec2(400., 300.),
            radius: 2000.,
            half: 5.,
            start: 0.,
            sweep: TAU,
            color: WHITE,
        };
        assert!(!visible(&hole, &[], 800., 600.));
    }
}
