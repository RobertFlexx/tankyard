//! The pictures on the Gates and Circuits palettes: real logic symbols for
//! the gates, and a small mark of its own for every other piece, so each one
//! can be told apart at a glance.
use super::ink::{Ink, on_curve};
use macroquad::prelude::*;
use std::f32::consts::{PI, TAU};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Symbol {
    Switch,
    And,
    Or,
    Xor,
    Not,
    Nand,
    Nor,
    Xnor,
    Majority,
    Parity,
    Branch,
    Lever,
    Coin,
    Dice,
    Steps,
    Readout,
    Bell,
    Star,
    Zero,
    Rearm,
    Crowd,
    Clock,
    Sum,
    Carry,
    Latch,
    Compare,
    Register,
    Flip,
    Number,
    Wave,
}

/// Each Gates and Circuits piece by name, with its symbol and whether it's
/// live, keeping its output up to date with no signal.
pub(super) const PIECES: [(&str, Symbol, bool); 34] = [
    ("Gate", Symbol::Switch, false),
    ("AND", Symbol::And, false),
    ("OR", Symbol::Or, false),
    ("XOR", Symbol::Xor, false),
    ("NOT", Symbol::Not, false),
    ("NAND", Symbol::Nand, false),
    ("NOR", Symbol::Nor, false),
    ("XNOR", Symbol::Xnor, false),
    ("Majority", Symbol::Majority, false),
    ("Parity", Symbol::Parity, false),
    ("If/else", Symbol::Branch, false),
    ("Lever", Symbol::Lever, false),
    ("Coin flip", Symbol::Coin, false),
    ("Dice", Symbol::Dice, false),
    ("Sequence", Symbol::Steps, false),
    ("Display", Symbol::Readout, false),
    ("Alarm", Symbol::Bell, false),
    ("Reward", Symbol::Star, false),
    ("Reset", Symbol::Zero, false),
    ("Rearm", Symbol::Rearm, false),
    ("Crowd", Symbol::Crowd, false),
    ("Clock", Symbol::Clock, false),
    ("Live AND", Symbol::And, true),
    ("Live OR", Symbol::Or, true),
    ("Live NOT", Symbol::Not, true),
    ("Live XOR", Symbol::Xor, true),
    ("Sum bit", Symbol::Sum, true),
    ("Carry bit", Symbol::Carry, true),
    ("Latch", Symbol::Latch, true),
    ("Compare", Symbol::Compare, true),
    ("Register", Symbol::Register, false),
    ("Flip bit", Symbol::Flip, false),
    ("Number", Symbol::Number, false),
    ("Pulse", Symbol::Wave, false),
];

/// The symbol of the Gates or Circuits piece called `name`.
pub fn of_piece(name: &str) -> Option<(Symbol, bool)> {
    PIECES
        .iter()
        .find(|(piece, ..)| *piece == name)
        .map(|&(_, symbol, live)| (symbol, live))
}

/// Where a gate's two and three inputs meet it.
const TWO: [f32; 2] = [-0.34, 0.34];
const THREE: [f32; 3] = [-0.42, 0., 0.42];
/// The radius of the small circle that marks an inverted output.
const BUBBLE: f32 = 0.13;

/// Draws in a square two units across, centered on `at`.
struct Pen<'a> {
    ink: &'a mut Ink,
    at: Vec2,
    unit: f32,
    width: f32,
    color: Color,
    points: Vec<Vec2>,
}

impl Pen<'_> {
    fn p(&self, (x, y): (f32, f32)) -> Vec2 {
        self.at + vec2(x, y) * self.unit
    }
    fn line(&mut self, a: (f32, f32), b: (f32, f32)) {
        let (a, b) = (self.p(a), self.p(b));
        self.ink.line(a, b, self.width, self.color);
    }
    fn start(&mut self, points: &[(f32, f32)]) {
        self.points.clear();
        for &point in points {
            let point = self.p(point);
            self.points.push(point);
        }
    }
    fn path(&mut self, points: &[(f32, f32)]) {
        self.start(points);
        self.ink.path(&self.points, self.width, self.color);
    }
    fn closed(&mut self, points: &[(f32, f32)]) {
        self.start(points);
        self.ink.closed(&self.points, self.width, self.color);
    }
    fn solid(&mut self, points: &[(f32, f32)]) {
        self.start(points);
        self.ink.fill(&self.points, self.color);
    }
    fn dot(&mut self, at: (f32, f32), radius: f32) {
        let at = self.p(at);
        self.ink.disc(at, radius * self.unit, self.color);
    }
    fn circle(&mut self, at: (f32, f32), radius: f32) {
        let at = self.p(at);
        self.ink
            .ring(at, radius * self.unit, self.width, self.color);
    }
    /// Adds the curve from `a` to `b` by way of `via` to the points in hand,
    /// leaving off its last point for whatever follows.
    fn add_curve(&mut self, a: (f32, f32), via: (f32, f32), b: (f32, f32)) {
        let (a, via, b) = (self.p(a), self.p(via), self.p(b));
        for step in 0..12 {
            self.points.push(on_curve(a, via, b, step as f32 / 12.));
        }
    }
    fn bend(&mut self, a: (f32, f32), via: (f32, f32), b: (f32, f32)) {
        self.points.clear();
        self.add_curve(a, via, b);
        let end = self.p(b);
        self.points.push(end);
        self.ink.path(&self.points, self.width, self.color);
    }
    /// The arc around `at` between two angles, in degrees, ending in an
    /// arrowhead when `head` is set.
    fn arc(&mut self, at: (f32, f32), radius: f32, from: f32, to: f32, head: bool) {
        const STEPS: usize = 24;
        self.points.clear();
        let center = self.p(at);
        let reach = radius * self.unit;
        let angle = |step: usize| (from + (to - from) * step as f32 / STEPS as f32).to_radians();
        for step in 0..=STEPS {
            let turn = angle(step);
            self.points
                .push(center + vec2(turn.cos(), turn.sin()) * reach);
        }
        self.ink.path(&self.points, self.width, self.color);
        if head {
            let turn = angle(STEPS);
            let tip = center + vec2(turn.cos(), turn.sin()) * reach;
            let way = (to - from).signum();
            let along = vec2(-turn.sin(), turn.cos()) * way;
            let size = self.unit * 0.42;
            self.ink
                .head(tip + along * size * 0.62, along, size, self.color);
        }
    }
    /// An arrowhead ending at `tip`, coming from `from`.
    fn tip(&mut self, from: (f32, f32), tip: (f32, f32)) {
        let (from, tip) = (self.p(from), self.p(tip));
        let along = (tip - from).normalize_or_zero();
        self.ink.head(tip, along, self.unit * 0.36, self.color);
    }
    /// Lines from the piece's left edge in to `x`, one at each height.
    fn inputs(&mut self, heights: &[f32], x: f32) {
        for &y in heights {
            self.line((-0.96, y), (x, y));
        }
    }
    /// The output from the body's tip, through a small circle if inverted.
    fn output(&mut self, tip: f32, inverted: bool) {
        if inverted {
            self.circle((tip + BUBBLE, 0.), BUBBLE);
            self.line((tip + BUBBLE * 2., 0.), (0.98, 0.));
        } else {
            self.line((tip, 0.), (0.96, 0.));
        }
    }
    /// The body of an AND gate: a flat back and a round front.
    fn and_body(&mut self) {
        self.points.clear();
        for point in [(-0.52, 0.6), (-0.52, -0.6)] {
            let point = self.p(point);
            self.points.push(point);
        }
        const STEPS: usize = 18;
        for step in 0..=STEPS {
            let turn = -PI / 2. + PI * step as f32 / STEPS as f32;
            let point = self.p((-0.02 + 0.6 * turn.cos(), 0.6 * turn.sin()));
            self.points.push(point);
        }
        self.ink.closed(&self.points, self.width, self.color);
    }
    /// The body of an OR gate, moved `x` along: a curved back and two curved
    /// sides meeting at a point.
    fn or_body(&mut self, x: f32) {
        self.points.clear();
        self.add_curve((x - 0.56, -0.6), (x + 0.16, -0.62), (x + 0.6, 0.));
        self.add_curve((x + 0.6, 0.), (x + 0.16, 0.62), (x - 0.56, 0.6));
        self.add_curve((x - 0.56, 0.6), (x - 0.24, 0.), (x - 0.56, -0.6));
        self.ink.closed(&self.points, self.width, self.color);
    }
    /// The extra curved line behind an exclusive gate's back.
    fn or_back(&mut self, x: f32) {
        self.bend((x - 0.74, -0.6), (x - 0.42, 0.), (x - 0.74, 0.6));
    }
}

/// Draws `symbol` in `r`, in `color`.
pub fn draw(ink: &mut Ink, symbol: Symbol, r: Rect, color: Color) {
    let unit = r.w.min(r.h) * 0.5;
    let mut pen = Pen {
        ink,
        at: r.center(),
        unit,
        width: (unit * 0.11).clamp(1.25, 3.4),
        color,
        points: Vec::with_capacity(48),
    };
    match symbol {
        Symbol::And | Symbol::Nand => {
            pen.inputs(&TWO, -0.52);
            pen.and_body();
            pen.output(0.58, symbol == Symbol::Nand);
        }
        Symbol::Or | Symbol::Nor => {
            pen.inputs(&TWO, -0.45);
            pen.or_body(0.);
            pen.output(0.6, symbol == Symbol::Nor);
        }
        Symbol::Xor | Symbol::Xnor => {
            pen.inputs(&TWO, -0.55);
            pen.or_back(0.08);
            pen.or_body(0.08);
            pen.output(0.68, symbol == Symbol::Xnor);
        }
        Symbol::Not => {
            pen.inputs(&[0.], -0.5);
            pen.closed(&[(-0.5, -0.56), (0.46, 0.), (-0.5, 0.56)]);
            pen.output(0.46, true);
        }
        Symbol::Majority => {
            pen.inputs(&THREE, -0.52);
            pen.and_body();
            pen.output(0.58, false);
            // Two of three lit.
            pen.dot((-0.18, -0.3), 0.1);
            pen.dot((-0.18, 0.), 0.1);
            pen.circle((-0.18, 0.3), 0.09);
        }
        Symbol::Parity => {
            pen.line((-0.96, -0.42), (-0.58, -0.42));
            pen.line((-0.96, 0.), (-0.5, 0.));
            pen.line((-0.96, 0.42), (-0.58, 0.42));
            pen.or_back(0.08);
            pen.or_body(0.08);
            pen.output(0.68, false);
        }
        Symbol::Switch => {
            pen.line((-0.96, 0.24), (-0.4, 0.24));
            pen.line((0.42, 0.24), (0.96, 0.24));
            pen.line((-0.4, 0.24), (0.34, -0.4));
            pen.dot((-0.4, 0.24), 0.11);
            pen.dot((0.42, 0.24), 0.11);
        }
        Symbol::Branch => {
            pen.line((-0.94, 0.), (-0.3, 0.));
            pen.bend((-0.3, 0.), (0.16, 0.), (0.5, -0.48));
            pen.bend((-0.3, 0.), (0.16, 0.), (0.5, 0.48));
            pen.tip((0.26, -0.28), (0.8, -0.72));
            pen.tip((0.26, 0.28), (0.8, 0.72));
            pen.dot((-0.3, 0.), 0.12);
        }
        Symbol::Lever => {
            pen.line((-0.72, 0.62), (0.72, 0.62));
            pen.line((0., 0.62), (0.44, -0.46));
            pen.dot((0.5, -0.58), 0.2);
            pen.dot((0., 0.62), 0.14);
        }
        Symbol::Coin => {
            pen.circle((0., 0.), 0.76);
            let mut half = [(0., 0.); 15];
            for (i, point) in half.iter_mut().enumerate() {
                let turn = -PI / 2. + PI * i as f32 / 14.;
                *point = (0.44 * turn.cos(), 0.44 * turn.sin());
            }
            pen.solid(&half);
            pen.circle((0., 0.), 0.44);
        }
        Symbol::Dice => {
            pen.closed(&[
                (-0.56, -0.72),
                (0.56, -0.72),
                (0.72, -0.56),
                (0.72, 0.56),
                (0.56, 0.72),
                (-0.56, 0.72),
                (-0.72, 0.56),
                (-0.72, -0.56),
            ]);
            for at in [(-0.34, -0.34), (0., 0.), (0.34, 0.34)] {
                pen.dot(at, 0.14);
            }
        }
        Symbol::Steps => {
            for (i, (x, top)) in [(-0.62, 0.26), (0., -0.14), (0.62, -0.62)]
                .into_iter()
                .enumerate()
            {
                let bar = [
                    (x - 0.2, 0.68),
                    (x - 0.2, top),
                    (x + 0.2, top),
                    (x + 0.2, 0.68),
                ];
                if i == 2 {
                    pen.solid(&bar);
                } else {
                    pen.closed(&bar);
                }
            }
        }
        Symbol::Readout => {
            pen.closed(&[(-0.62, -0.82), (0.62, -0.82), (0.62, 0.82), (-0.62, 0.82)]);
            let width = pen.width;
            pen.width = width * 1.2;
            for (a, b) in [
                ((-0.2, -0.48), (0.2, -0.48)),
                ((-0.2, 0.), (0.2, 0.)),
                ((-0.2, 0.48), (0.2, 0.48)),
                ((-0.3, -0.38), (-0.3, -0.1)),
                ((0.3, -0.38), (0.3, -0.1)),
                ((-0.3, 0.1), (-0.3, 0.38)),
                ((0.3, 0.1), (0.3, 0.38)),
            ] {
                pen.line(a, b);
            }
            pen.width = width;
        }
        Symbol::Bell => {
            pen.points.clear();
            pen.add_curve((-0.48, 0.2), (-0.46, -0.66), (0., -0.66));
            pen.add_curve((0., -0.66), (0.46, -0.66), (0.48, 0.2));
            pen.add_curve((0.48, 0.2), (0.52, 0.36), (0.7, 0.46));
            let (right, left) = (pen.p((0.7, 0.46)), pen.p((-0.7, 0.46)));
            pen.points.push(right);
            pen.points.push(left);
            pen.add_curve((-0.7, 0.46), (-0.52, 0.36), (-0.48, 0.2));
            let (points, width, color) = (std::mem::take(&mut pen.points), pen.width, pen.color);
            pen.ink.closed(&points, width, color);
            pen.dot((0., 0.68), 0.14);
            pen.dot((0., -0.76), 0.08);
        }
        Symbol::Star => {
            let mut star = [(0., 0.); 10];
            for (i, point) in star.iter_mut().enumerate() {
                let turn = -PI / 2. + TAU * i as f32 / 10.;
                let reach = if i % 2 == 0 { 0.86 } else { 0.36 };
                *point = (reach * turn.cos(), 0.05 + reach * turn.sin());
            }
            pen.solid(&star);
        }
        Symbol::Zero => {
            pen.arc((0., 0.), 0.72, -40., -320., true);
            let mut zero = [(0., 0.); 20];
            for (i, point) in zero.iter_mut().enumerate() {
                let turn = TAU * i as f32 / 20.;
                *point = (0.2 * turn.cos(), 0.32 * turn.sin());
            }
            pen.closed(&zero);
        }
        Symbol::Rearm => {
            pen.arc((0., 0.), 0.68, -220., 70., true);
            pen.dot((0., 0.), 0.18);
        }
        Symbol::Crowd => {
            pen.line((-0.34, 0.2), (-0.62, -0.62));
            pen.line((0.34, 0.2), (0.62, -0.62));
            pen.circle((-0.36, 0.26), 0.36);
            pen.circle((0.36, 0.26), 0.36);
            pen.dot((-0.36, 0.26), 0.12);
            pen.dot((0.36, 0.26), 0.12);
        }
        Symbol::Clock => {
            pen.circle((0., 0.), 0.78);
            pen.path(&[(0., -0.52), (0., 0.), (0.38, 0.18)]);
            pen.dot((0., 0.), 0.08);
        }
        Symbol::Sum => {
            pen.width *= 1.25;
            pen.path(&[
                (0.52, -0.5),
                (0.52, -0.68),
                (-0.52, -0.68),
                (0.06, 0.),
                (-0.52, 0.68),
                (0.52, 0.68),
                (0.52, 0.5),
            ]);
        }
        Symbol::Carry => {
            pen.width *= 1.2;
            pen.line((-0.68, 0.34), (-0.12, 0.34));
            pen.line((-0.4, 0.06), (-0.4, 0.62));
            pen.bend((0.64, 0.42), (0.62, -0.58), (-0.08, -0.54));
            pen.tip((0.3, -0.56), (-0.26, -0.52));
        }
        Symbol::Latch => {
            pen.inputs(&TWO, -0.56);
            pen.closed(&[(-0.56, -0.68), (0.56, -0.68), (0.56, 0.68), (-0.56, 0.68)]);
            pen.line((0.56, 0.), (0.96, 0.));
            pen.path(&[(-0.34, 0.34), (-0.06, 0.34), (-0.06, -0.34), (0.34, -0.34)]);
        }
        Symbol::Compare => {
            pen.width *= 1.35;
            pen.path(&[(-0.5, -0.62), (0.5, -0.18), (-0.5, 0.26)]);
            pen.line((-0.5, 0.62), (0.5, 0.62));
        }
        Symbol::Register => {
            pen.line((-0.96, -0.34), (-0.56, -0.34));
            pen.line((0.56, -0.34), (0.96, -0.34));
            pen.closed(&[(-0.56, -0.7), (0.56, -0.7), (0.56, 0.7), (-0.56, 0.7)]);
            pen.path(&[(-0.56, 0.2), (-0.26, 0.42), (-0.56, 0.64)]);
            pen.solid(&[(-0.06, -0.48), (0.3, -0.48), (0.3, -0.12), (-0.06, -0.12)]);
        }
        Symbol::Flip => {
            pen.line((-0.72, -0.32), (0.44, -0.32));
            pen.tip((0.3, -0.32), (0.8, -0.32));
            pen.line((0.72, 0.32), (-0.44, 0.32));
            pen.tip((-0.3, 0.32), (-0.8, 0.32));
        }
        Symbol::Number => {
            pen.width *= 1.2;
            pen.line((-0.18, -0.72), (-0.34, 0.72));
            pen.line((0.34, -0.72), (0.18, 0.72));
            pen.line((-0.66, -0.26), (0.68, -0.26));
            pen.line((-0.68, 0.26), (0.66, 0.26));
        }
        Symbol::Wave => {
            pen.path(&[
                (-0.96, 0.42),
                (-0.62, 0.42),
                (-0.62, -0.42),
                (-0.18, -0.42),
                (-0.18, 0.42),
                (0.26, 0.42),
                (0.26, -0.42),
                (0.7, -0.42),
                (0.7, 0.42),
                (0.96, 0.42),
            ]);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_piece_has_one_symbol_and_gates_stay_apart() {
        for (i, (name, ..)) in PIECES.iter().enumerate() {
            assert!(
                PIECES[i + 1..].iter().all(|(other, ..)| other != name),
                "{name} is listed twice"
            );
            assert!(of_piece(name).is_some());
        }
        assert_eq!(of_piece("Live AND"), Some((Symbol::And, true)));
        assert_eq!(of_piece("AND"), Some((Symbol::And, false)));
        assert_eq!(of_piece("Timer"), None);
        let mut gates = [
            of_piece("AND"),
            of_piece("OR"),
            of_piece("XOR"),
            of_piece("NOT"),
            of_piece("NAND"),
            of_piece("NOR"),
            of_piece("XNOR"),
        ]
        .map(|piece| piece.unwrap().0 as u8);
        gates.sort_unstable();
        assert!(gates.windows(2).all(|pair| pair[0] != pair[1]));
    }
}
