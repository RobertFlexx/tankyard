//! The Jester's folded cowl and the Carillon's bell foundry. All dimensions
//! are in hull radii, so the cards and the arena use the same silhouettes.
use super::*;

const INK: Color = Color::new(0.14, 0.10, 0.19, 1.);
const IVORY: Color = Color::new(0.95, 0.88, 0.72, 1.);
const BRASS: Color = Color::new(0.92, 0.66, 0.24, 1.);
const IRON: Color = Color::new(0.23, 0.27, 0.27, 1.);
const PATINA: Color = Color::new(0.24, 0.46, 0.42, 1.);
pub const FURNACE: Color = Color::new(1., 0.43, 0.12, 1.);

/// Separate rounds carry adjacent colors even with animation disabled.
pub fn shot_silk(id: u64, time: f32) -> Color {
    silk((id % 89) as f32 * 0.237 + time * 0.13)
}

/// Dyed silk. Each fold uses adjacent stops, including across the seam.
pub fn silk(phase: f32) -> Color {
    const DYES: [Color; 6] = [
        Color::new(0.88, 0.18, 0.34, 1.),
        Color::new(1., 0.55, 0.19, 1.),
        Color::new(0.94, 0.82, 0.22, 1.),
        Color::new(0.15, 0.72, 0.50, 1.),
        Color::new(0.15, 0.56, 0.91, 1.),
        Color::new(0.61, 0.29, 0.84, 1.),
    ];
    let t = phase.rem_euclid(1.) * DYES.len() as f32;
    let i = t as usize % DYES.len();
    let (a, b) = (DYES[i], DYES[(i + 1) % DYES.len()]);
    let f = t.fract();
    Color::new(
        a.r + (b.r - a.r) * f,
        a.g + (b.g - a.g) * f,
        a.b + (b.b - a.b) * f,
        1.,
    )
}

#[derive(Clone, Copy)]
struct Cowl {
    root: Vec2,
    bend: Vec2,
    tip: Vec2,
}

impl Cowl {
    fn new(index: usize, swing: f32) -> Self {
        let (root, bend, tip) = match index {
            0 => (vec2(-0.45, -0.27), vec2(-1.68, -1.18), vec2(-0.35, -1.30)),
            1 => (vec2(-0.55, 0.02), vec2(-1.84, 0.44), vec2(-1.38, -0.12)),
            _ => (vec2(-0.45, 0.27), vec2(-1.51, 1.25), vec2(-0.31, 1.30)),
        };
        let hinge = root.lerp(bend, 0.42);
        let twist = Vec2::from_angle(swing);
        Self {
            root,
            bend: hinge + twist.rotate(bend - hinge),
            tip: hinge + twist.rotate(tip - hinge),
        }
    }
    fn section(self, t: f32) -> (Vec2, Vec2) {
        let lo = self.root.lerp(self.bend, t);
        let hi = self.bend.lerp(self.tip, t);
        let ridge = lo.lerp(hi, t);
        let tangent = (hi - lo).normalize_or_zero();
        let normal = vec2(-tangent.y, tangent.x);
        (ridge, normal * (0.028 + 0.33 * (1. - t) * (1. - 0.28 * t)))
    }
}

// The face hangs upright on its swivel. The slight lean follows the gun's
// bearing continuously across the wrap from pi to minus pi.
fn hat_axes(aim: f32) -> Vec2 {
    Vec2::from_angle(FRAC_PI_2 + mask_tilt(aim))
}
fn mask_tilt(aim: f32) -> f32 {
    0.09 * aim.sin()
}
pub(super) fn jewel(p: Vec2, r: f32, aim: f32) -> Vec2 {
    p - hat_axes(aim) * r * 0.83
}

pub fn bell_tips(p: Vec2, r: f32, aim: f32, swings: [f32; 3]) -> [Vec2; 3] {
    let axes = hat_axes(aim);
    std::array::from_fn(|i| p + axes.rotate(Cowl::new(i, swings[i]).tip) * r)
}

pub(super) fn jester(p: Vec2, r: f32, pose: Pose, emit: &mut impl FnMut(Primitive)) {
    let axes = hat_axes(pose.aim);
    let world = |v: Vec2| p + axes.rotate(v) * r;
    let at = |x, y| world(vec2(x, y));
    let e = (r * 0.034).max(0.48);
    let dye = pose.time * 0.055;
    // A foundry port under the collar and the three steel card guides.
    let back = -Vec2::from_angle(pose.aim);
    let feed = p + back * r * 1.13;
    bar(
        emit,
        (p + back * r * 0.42, feed),
        (r * 0.17, r * 0.27),
        INK,
        e,
    );
    for (depth, width) in [(0.03, 0.42), (0.11, 0.37), (0.19, 0.31)] {
        let card = feed - back * r * depth;
        plate(emit, card, vec2(0.045, width) * r, pose.aim, IVORY);
    }
    for (i, angle) in [-0.38_f32, 0., 0.38].into_iter().enumerate() {
        let d = Vec2::from_angle(pose.aim + angle);
        let length = if i == 1 {
            tankyard_core::encounters::CARD_MUZZLE
        } else {
            1.31
        };
        let foot = p + d * r * (0.48 - pose.recoil * 0.12);
        bar(
            emit,
            (foot, foot + d * r * (length - 0.48)),
            (r * 0.103, r * 0.071),
            INK,
            e,
        );
        bar(
            emit,
            (foot + d * r * 0.17, foot + d * r * (length - 0.5)),
            (r * 0.032, r * 0.024),
            silk(dye + i as f32 / 3.),
            0.,
        );
        let shoe = foot + d * r * (length - 0.54);
        plate(emit, shoe, vec2(0.12, 0.18) * r, pose.aim + angle, BRASS);
        plate(
            emit,
            shoe + d * 0.018 * r,
            vec2(0.11, 0.098) * r,
            pose.aim + angle,
            INK,
        );
        if r > 26. {
            plate(
                emit,
                foot + d * r * 0.48,
                vec2(0.11, 0.085) * r,
                pose.aim + angle + FRAC_PI_4,
                IVORY,
            );
        }
    }
    // Joined pairs of strips form a raised seam. Shared vertices keep the
    // taper continuous even at the hooked ends of the cap.
    for i in [1, 0, 2] {
        let cowl = Cowl::new(i, pose.bells[i]);
        let mut last = cowl.section(0.);
        let steps = if r > 30. { 12 } else { 7 };
        for j in 1..=steps {
            let t = j as f32 / steps as f32;
            let next = cowl.section(t);
            let fold = |u: f32| silk(dye + i as f32 / 3. + 0.17 * u);
            let (a, b) = (fold(t - 1. / steps as f32), fold(t));
            let strip = |sign: f32, ca, cb| Primitive::ShadedQuad {
                vertices: [
                    world(last.0),
                    world(next.0),
                    world(next.0 + next.1 * sign),
                    world(last.0 + last.1 * sign),
                ],
                color: ca,
                end: cb,
            };
            emit(strip(1., lighten(a, 0.13), lighten(b, 0.13)));
            emit(strip(-1., tint(a, 0.56), tint(b, 0.56)));
            last = next;
        }
        ModelArt::JingleBell.place(
            world(cowl.tip),
            r * 0.275,
            mask_tilt(pose.aim) + pose.bells[i],
            emit,
        );
    }
    // A ruff of alternating lamé and dyed silk frames the porcelain. Its
    // folds fan from the neck, leaving both the cap and gun muzzles exposed.
    hull(emit, p, 10, r * 0.78, mask_tilt(pose.aim), INK, e);
    for fold in 0..9 {
        let angle = 0.30 + fold as f32 * (PI - 0.60) / 8.;
        let radial = Vec2::from_angle(angle + mask_tilt(pose.aim));
        let normal = radial.perp();
        let root = p + radial * r * 0.44;
        let ridge = p + radial * r * (if fold % 2 == 0 { 1.07 } else { 1.00 });
        let hem = p + radial * r * 0.81;
        let color = silk(dye + fold as f32 * 0.094);
        let l = hem + normal * r * 0.165;
        let h = hem - normal * r * 0.165;
        shard(emit, [root, l, ridge], lighten(color, 0.08), e * 0.38);
        shard(emit, [root, ridge, h], tint(color, 0.58), e * 0.38);
        if r > 26. {
            bar(emit, (l, ridge), (r * 0.011, r * 0.009), BRASS, 0.);
        }
    }
    for side in [-1., 1.] {
        let hem = [
            at(-0.61, side * 0.61),
            at(-0.35, side * 0.73),
            at(0.33, side * 0.65),
            at(0.47, side * 0.43),
        ];
        facet(
            emit,
            hem,
            tint(silk(dye + if side > 0. { 0.72 } else { 0.25 }), 0.76),
            e * 0.6,
        );
    }
    let tilt = mask_tilt(pose.aim);
    ModelArt::ComedyMask.place(p + axes * r * 0.11, 1.78 * r, tilt, emit);
    // Set the jewel above the forehead so the cast light never fills an eye.
    for side in [-1., 1.] {
        bar(
            emit,
            (at(-0.70, side * 0.43), at(-0.83, 0.)),
            (r * 0.023, r * 0.034),
            BRASS,
            e * 0.29,
        );
    }
    let core = jewel(p, r, pose.aim);
    let glass = pose.mask.map_or_else(|| silk(dye + 0.85), boss_color);
    hull(emit, core, 4, r * 0.111, tilt, BRASS, e * 0.5);
    poly(
        emit,
        core,
        4,
        r * 0.075,
        tilt,
        lighten(glass, 0.18 + 0.50 * pose.charge),
    );
}

pub(super) fn carillon(p: Vec2, r: f32, pose: Pose, emit: &mut impl FnMut(Primitive)) {
    let axes = Vec2::from_angle(pose.aim);
    let at = |x, y| p + axes.rotate(vec2(x, y)) * r;
    let e = (r * 0.042).max(0.55);
    let bronze = boss_color(BossKind::Carillon);
    let hot = lighten(FURNACE, 0.18 + 0.52 * pose.charge);
    // Three cast iron throats. Each has a bronze ferrule over the slide.
    for lane in [-0.25_f32, 0., 0.25] {
        let angle = pose.aim + lane;
        let d = Vec2::from_angle(angle);
        let foot = p + d * r * (0.48 - 0.16 * pose.recoil);
        let tube_end = foot + d * r * 1.02;
        bar(
            emit,
            (foot, tube_end),
            (r * 0.11, r * 0.142),
            tint(IRON, 0.72),
            e * 0.55,
        );
        let side = d.perp();
        let upper = side * r * 0.095;
        bar(
            emit,
            (foot - upper, tube_end - upper),
            (r * 0.021, r * 0.023),
            lighten(IRON, 0.32),
            0.,
        );
        for depth in [0.52, 0.84] {
            let collar = foot + d * r * depth;
            plate(
                emit,
                collar,
                vec2(0.105, 0.32) * r,
                angle,
                tint(bronze, 0.63),
            );
            plate(
                emit,
                collar - d * r * 0.017,
                vec2(0.065, 0.315) * r,
                angle,
                bronze,
            );
        }
        bar(
            emit,
            (tube_end - side * r * 0.105, tube_end + side * r * 0.105),
            (r * 0.026, r * 0.026),
            tint(IRON, 0.37),
            0.,
        );
        if pose.charge > 0.02 {
            let slit = tube_end - d * r * 0.015;
            bar(
                emit,
                (slit - side * r * 0.075, slit + side * r * 0.075),
                (r * 0.011, r * 0.011),
                tint(hot, 0.55 + pose.charge * 0.45),
                0.,
            );
        }
    }
    for side in [-1., 1.] {
        facet(
            emit,
            [
                at(-0.97, side * 0.51),
                at(-0.79, side * 0.97),
                at(0.71, side * 0.97),
                at(0.88, side * 0.55),
            ],
            IRON,
            e,
        );
        for tooth in 0..6 {
            plate(
                emit,
                at(-0.65 + tooth as f32 * 0.24, side * 0.85),
                vec2(0.10, 0.18) * r,
                pose.aim,
                tint(PATINA, 0.76),
            );
        }
    }
    hull(emit, p, 8, r * 0.94, pose.aim + PI / 8., bronze, e);
    hull(emit, p, 8, r * 0.78, pose.aim + PI / 8., IRON, e * 0.55);
    // The bell sits in a bolted, sprung cradle. A separate sight-glass
    // at the bow shows the heat building even while the hammers are lifted.
    let bell = at(-0.16, 0.);
    hull(
        emit,
        bell,
        16,
        r * 0.69,
        pose.aim,
        tint(IRON, 0.61),
        e * 0.4,
    );
    ModelArt::CastBell.place(bell, r * 1.35, pose.aim, emit);
    let firebox = at(0.59, 0.);
    facet(
        emit,
        [
            at(0.42, -0.24),
            at(0.71, -0.19),
            at(0.71, 0.19),
            at(0.42, 0.24),
        ],
        tint(bronze, 0.67),
        e * 0.55,
    );
    plate(emit, firebox, vec2(0.18, 0.32) * r, pose.aim, hot);
    if r > 24. {
        for slot in [-0.096, 0., 0.096] {
            bar(
                emit,
                (at(0.50, slot), at(0.68, slot)),
                (r * 0.014, r * 0.015),
                IRON,
                0.,
            );
        }
    }
    for side in [-1., 1.] {
        // A trunnion lifts its hammer clear of the bell as the furnace
        // charges. On release it falls inward and kicks back with the shot.
        let pivot = at(-0.56, side * 0.73);
        let swing = side * (-0.35 + pose.charge * 0.78 - pose.recoil * 0.24);
        let axis = Vec2::from_angle(pose.aim + swing);
        let head = pivot + axis * r * 0.94;
        bar(emit, (pivot, head), (r * 0.057, r * 0.074), PATINA, e * 0.6);
        if r > 24. {
            let steam = at(-0.76, side * 0.77);
            let crank = pivot + axis * r * 0.39;
            bar(
                emit,
                (steam, crank),
                (r * 0.045, r * 0.019),
                lighten(IRON, 0.52),
                e * 0.27,
            );
            hull(emit, crank, 0, r * 0.047, 0., BRASS, e * 0.24);
        }
        hull(emit, pivot, 0, r * 0.12, 0., bronze, e * 0.5);
        poly(emit, pivot, 6, r * 0.062, pose.aim, IRON);
        plate(emit, head, vec2(0.24, 0.35) * r, pose.aim + swing, IRON);
        plate(emit, head, vec2(0.19, 0.31) * r, pose.aim + swing, bronze);
        let lip = head - axis.perp() * side * r * 0.13;
        plate(
            emit,
            lip,
            vec2(0.20, 0.07) * r,
            pose.aim + swing,
            lighten(BRASS, 0.16),
        );
        if r > 24. {
            let throat = at(-0.62, side * 0.48);
            bar(
                emit,
                (at(-0.80, side * 0.37), throat),
                (r * 0.042, r * 0.036),
                PATINA,
                e * 0.32,
            );
        }
        let chimney = at(-0.75, side * 0.38);
        hull(emit, chimney, 8, r * 0.16, pose.aim, bronze, e * 0.6);
        poly(
            emit,
            chimney,
            8,
            r * 0.098,
            pose.aim,
            tint(FURNACE, 0.56 + pose.charge * 0.44),
        );
    }
}
