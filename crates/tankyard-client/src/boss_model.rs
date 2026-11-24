//! Original vector models, shared by the game and the boss model sheet exporter.
use crate::tank_model::{OUTLINE, Primitive, cannon_profile};
use macroquad::prelude::{Color, Vec2, vec2};
use std::f32::consts::{FRAC_PI_2, FRAC_PI_4, PI, TAU};
use tankyard_core::BossKind;
use tankyard_core::creator_boss::{CANNONS, MUZZLE};

pub fn boss_color(kind: BossKind) -> Color {
    match kind {
        BossKind::Guardian => Color::new(1., 0.47, 0.86, 1.),
        BossKind::Summoner => Color::new(1., 0.91, 0.41, 1.),
        BossKind::Defender => Color::new(0.95, 0.30, 0.34, 1.),
        BossKind::FallenOverlord | BossKind::FallenBooster => Color::new(0.75, 0.75, 0.75, 1.),
        BossKind::Sentinel => Color::new(0.96, 0.69, 0.25, 1.),
        BossKind::Prism => Color::new(0.72, 0.50, 0.95, 1.),
        BossKind::Foundry => Color::new(0.89, 0.38, 0.28, 1.),
        BossKind::Tempest => Color::new(0.18, 0.80, 0.90, 1.),
        BossKind::Bastion => Color::new(0.53, 0.66, 0.77, 1.),
        BossKind::Eclipse => Color::new(0.57, 0.38, 0.86, 1.),
        BossKind::SiegeEngine => Color::new(0.69, 0.62, 0.41, 1.),
        BossKind::Skirmisher => Color::new(0.25, 0.67, 0.77, 1.),
        BossKind::Trapper => Color::new(0.75, 0.47, 0.31, 1.),
        BossKind::Railgunner => RAIL_ION,
    }
}

fn poly(
    emit: &mut impl FnMut(Primitive),
    center: Vec2,
    sides: u8,
    radius: f32,
    angle: f32,
    color: Color,
) {
    emit(Primitive::Polygon {
        center,
        sides,
        radius,
        angle,
        color,
    });
}
/// An outlined plate with a lighter rim, so armor reads as raised rather
/// than painted on. The rim drops out where it would be under a pixel.
fn hull(
    emit: &mut impl FnMut(Primitive),
    center: Vec2,
    sides: u8,
    radius: f32,
    angle: f32,
    color: Color,
    edge: f32,
) {
    poly(emit, center, sides, radius + edge, angle, OUTLINE);
    let rim = (radius * 0.07).min(edge * 2.5);
    if rim >= 0.8 {
        poly(emit, center, sides, radius, angle, lighten(color, 0.2));
        poly(emit, center, sides, radius - rim, angle, color);
    } else {
        poly(emit, center, sides, radius, angle, color);
    }
}
/// `color` moved `amount` of the way to white.
fn lighten(color: Color, amount: f32) -> Color {
    Color::new(
        color.r + (1. - color.r) * amount,
        color.g + (1. - color.g) * amount,
        color.b + (1. - color.b) * amount,
        color.a,
    )
}
fn tint(color: Color, factor: f32) -> Color {
    Color::new(
        color.r * factor,
        color.g * factor,
        color.b * factor,
        color.a,
    )
}

fn plate(emit: &mut impl FnMut(Primitive), center: Vec2, size: Vec2, angle: f32, color: Color) {
    let d = vec2(angle.cos(), angle.sin());
    let side = vec2(-d.y, d.x);
    let x = d * size.x * 0.5;
    let y = side * size.y * 0.5;
    emit(Primitive::Quad {
        vertices: [
            center - x - y,
            center + x - y,
            center + x + y,
            center - x + y,
        ],
        color,
    });
}

/// An outlined triangle, its outline grown out from its middle.
fn shard(emit: &mut impl FnMut(Primitive), vertices: [Vec2; 3], color: Color, edge: f32) {
    let middle = (vertices[0] + vertices[1] + vertices[2]) / 3.;
    let grown = vertices.map(|v| {
        let out = v - middle;
        v + out.normalize_or_zero() * edge * 1.8
    });
    emit(Primitive::Triangle {
        vertices: grown,
        color: Color {
            a: color.a,
            ..OUTLINE
        },
    });
    emit(Primitive::Triangle { vertices, color });
}

/// How a boss is posed when drawn.
#[derive(Clone, Copy, Debug, Default)]
pub struct Pose {
    /// Where its guns point.
    pub aim: f32,
    /// The angle of its turning launchers, from `BossKind::turn`.
    pub turn: f32,
    /// Seconds, for slow decorative motion; held still when motion is
    /// reduced.
    pub time: f32,
    /// 1 at full health, 2 when hurt, 3 when nearly beaten.
    pub phase: u8,
    /// How far its special attack has charged, 0 to 1.
    pub charge: f32,
    /// The game draws a wounded hull's heat as real light, so the flat heat
    /// plates that stand in for it on model sheets are left out.
    pub lit: bool,
    /// How far a shot has thrown the Railgunner's rails back, 0 to 1.
    pub recoil: f32,
    /// How hard its plasma beam is running, 0 to 1.
    pub beam: f32,
    /// How far its heat fins have opened, 0 to 1.
    pub vent: f32,
    /// How far its barrel has split open for the beam: 0 shut, 1 fully open,
    /// and a little past either while it settles.
    pub open: f32,
}

/// Each boss's main armor: its sides, its size against the boss radius and
/// its angle, matching the hulls drawn below.
pub fn armor(kind: BossKind, aim: f32, turn: f32) -> (u8, f32, f32) {
    match kind {
        BossKind::Guardian | BossKind::Tempest | BossKind::Skirmisher => (3, 1., aim),
        BossKind::Defender | BossKind::Prism => (3, 1., turn),
        BossKind::Summoner => (4, 1., aim + FRAC_PI_4),
        BossKind::Foundry => (4, 0.9, aim + FRAC_PI_4),
        BossKind::SiegeEngine => (4, 0.8, aim + FRAC_PI_4),
        BossKind::Bastion => (4, 1., FRAC_PI_4),
        BossKind::FallenOverlord | BossKind::FallenBooster | BossKind::Eclipse => (0, 1., 0.),
        BossKind::Sentinel | BossKind::Trapper => (6, 1., aim + PI / 6.),
        BossKind::Railgunner => (8, 1., aim + PI / 8.),
    }
}

/// Where a boss's glowing core sits, which brightens as it charges.
pub fn core_position(kind: BossKind, p: Vec2, r: f32, aim: f32) -> Vec2 {
    let ahead = vec2(aim.cos(), aim.sin());
    match kind {
        BossKind::Sentinel => p + ahead * r * 0.5,
        BossKind::Guardian => p - ahead * r * 0.12,
        BossKind::Tempest | BossKind::Skirmisher => p + ahead * r * 0.12,
        BossKind::Eclipse => p - ahead * r * 0.5,
        _ => p,
    }
}

/// A boss's shape in its own color, or its team's for a mothership. Every
/// boss is built the same way: a dark outline, a raised rim, armor plates a
/// shade darker inside, grey guns where its rounds really leave, and one
/// bright core.
pub fn boss_model_geometry(
    kind: BossKind,
    p: Vec2,
    r: f32,
    pose: Pose,
    team_color: Option<Color>,
    emit: &mut impl FnMut(Primitive),
) {
    let Pose {
        aim,
        turn,
        time,
        phase,
        charge,
        lit,
        ..
    } = pose;
    if !r.is_finite() || r <= 0. || !aim.is_finite() || !turn.is_finite() || !time.is_finite() {
        return;
    }
    let color = team_color.unwrap_or_else(|| boss_color(kind));
    let edge = (r * 0.045).max(0.6);
    // A wounded boss burns: amber at its second phase, a pulsing red at its
    // last, glowing around its armor in the armor's own shape.
    if phase >= 2 && !lit {
        let (sides, size, angle) = armor(kind, aim, turn);
        let (heat, pulse) = if phase >= 3 {
            (
                Color::new(1., 0.28, 0.2, color.a),
                0.5 + 0.5 * (time * 5.).sin(),
            )
        } else {
            (Color::new(1., 0.7, 0.24, color.a), 0.)
        };
        for (scale, strength) in [(1.2, 0.1), (1.09, 0.16)] {
            poly(
                emit,
                p,
                sides,
                r * size * (scale + 0.04 * pulse) + edge,
                angle,
                Color {
                    a: heat.a * (strength + 0.06 * pulse),
                    ..heat
                },
            );
        }
    }
    let direction = |angle: f32| vec2(angle.cos(), angle.sin());
    let ahead = direction(aim);
    let across = vec2(-ahead.y, ahead.x);
    let dark = tint(color, 0.72);
    let plate_color = tint(color, 0.86);
    // The core brightens toward white as a special attack charges.
    let glow = lighten(color, 0.55 + 0.4 * charge.clamp(0., 1.));
    let mount = |emit: &mut _, angle: f32, width: f32, length: f32, flare: f32| {
        cannon_profile(
            emit,
            p + direction(angle) * r * 0.47,
            angle,
            length * r,
            width * r,
            color,
            0.,
            flare,
            Some(edge),
        );
    };
    match kind {
        BossKind::Sentinel => {
            // A hexagonal watchtower: two heavy guns side by side under a
            // visor with one burning slit.
            for side in [-1., 1.] {
                cannon_profile(
                    emit,
                    p + across * side * r * 0.3,
                    aim,
                    r * 1.4,
                    r * 0.32,
                    color,
                    0.,
                    1.,
                    Some(edge),
                );
            }
            hull(emit, p, 6, r, aim + PI / 6., color, edge);
            hull(
                emit,
                p - ahead * r * 0.2,
                6,
                r * 0.48,
                aim + PI / 6.,
                plate_color,
                edge * 0.45,
            );
            plate(
                emit,
                p + ahead * r * 0.5,
                vec2(r * 0.3, r * 1.04),
                aim,
                OUTLINE,
            );
            plate(
                emit,
                p + ahead * r * 0.5,
                vec2(r * 0.3 - edge * 2., r * 1.04 - edge * 2.),
                aim,
                dark,
            );
            plate(
                emit,
                p + ahead * r * 0.5,
                vec2(r * 0.09, r * 0.68),
                aim,
                glow,
            );
        }
        BossKind::Prism => {
            // A turning crystal: three facets in three shades, an emitter
            // at each point.
            for n in 0..3 {
                mount(emit, turn + n as f32 * TAU / 3., 0.2, 0.78, 1.25);
            }
            hull(emit, p, 3, r, turn, color, edge);
            let inner = r - (r * 0.07).min(edge * 2.5);
            for n in 0..3 {
                let a = turn + n as f32 * TAU / 3.;
                emit(Primitive::Triangle {
                    vertices: [
                        p,
                        p + direction(a) * inner,
                        p + direction(a + TAU / 3.) * inner,
                    ],
                    color: [lighten(color, 0.22), color, tint(color, 0.74)][n],
                });
            }
            poly(emit, p, 3, r * 0.24, turn + PI, OUTLINE);
            poly(emit, p, 3, r * 0.24 - edge, turn + PI, glow);
        }
        BossKind::Foundry => {
            // A works: two launch bays at its front corners, a furnace
            // behind a grille and two stacks at the back.
            for side in [-1., 1.] {
                mount(emit, aim + side * 0.7, 0.5, 0.82, 1.4);
            }
            hull(emit, p, 4, r * 0.9, aim + FRAC_PI_4, color, edge);
            for side in [-1., 1.] {
                let stack = p - ahead * r * 0.42 + across * side * r * 0.4;
                hull(emit, stack, 0, r * 0.17, 0., dark, edge * 0.7);
                poly(emit, stack, 0, r * 0.09, 0., OUTLINE);
            }
            plate(
                emit,
                p + ahead * r * 0.1,
                vec2(r * 0.66, r * 0.66),
                aim,
                OUTLINE,
            );
            plate(
                emit,
                p + ahead * r * 0.1,
                vec2(r * 0.66 - edge * 2., r * 0.66 - edge * 2.),
                aim,
                tint(color, 0.5),
            );
            for n in -1..=1 {
                plate(
                    emit,
                    p + ahead * r * 0.1 + across * n as f32 * r * 0.19,
                    vec2(r * 0.5, r * 0.07),
                    aim,
                    glow,
                );
            }
        }
        BossKind::Tempest => {
            // A storm dart: a long point, swept wings and missile racks.
            for side in [-1., 1.] {
                shard(
                    emit,
                    [
                        p - ahead * r * 0.15 + across * side * r * 0.4,
                        p - ahead * r * 1.1 + across * side * r * 1.22,
                        p - ahead * r * 0.62 + across * side * r * 0.3,
                    ],
                    dark,
                    edge,
                );
                cannon_profile(
                    emit,
                    p - ahead * r * 0.42 + across * side * r * 0.74,
                    aim,
                    r * 0.92,
                    r * 0.22,
                    color,
                    0.,
                    1.,
                    Some(edge),
                );
            }
            hull(emit, p, 3, r, aim, color, edge);
            poly(emit, p + ahead * r * 0.12, 4, r * 0.26, aim, OUTLINE);
            poly(
                emit,
                p + ahead * r * 0.12,
                4,
                r * 0.26 - edge * 1.4,
                aim,
                glow,
            );
        }
        BossKind::Bastion => {
            // A square keep with round towers at its corners; the gun turns
            // on a ring in the middle.
            hull(emit, p, 4, r, FRAC_PI_4, color, edge);
            hull(emit, p, 4, r * 0.7, FRAC_PI_4, plate_color, edge * 0.45);
            for n in 0..4 {
                let corner = p + direction(FRAC_PI_4 + n as f32 * FRAC_PI_2) * r * 0.76;
                hull(emit, corner, 0, r * 0.25, 0., dark, edge * 0.7);
                // Mine hatches, turning with its launchers.
                poly(emit, corner, 4, r * 0.12, turn, OUTLINE);
                poly(emit, corner, 4, r * 0.12 - edge, turn, tint(color, 0.55));
            }
            cannon_profile(emit, p, aim, r * 1.52, r * 0.46, color, 0., 1., Some(edge));
            hull(emit, p, 0, r * 0.34, 0., plate_color, edge * 0.8);
            poly(emit, p, 0, r * 0.15, 0., OUTLINE);
            poly(emit, p, 0, r * 0.15 - edge, 0., glow);
        }
        BossKind::Eclipse => {
            // A dark moon across a burning disc, a corona of rays around
            // it; its fragments burst out of the corona.
            for n in 0..12 {
                let a = time * 0.12 + n as f32 * TAU / 12.;
                shard(
                    emit,
                    [
                        p + direction(a - 0.13) * r * 0.9,
                        p + direction(a) * r * 1.22,
                        p + direction(a + 0.13) * r * 0.9,
                    ],
                    lighten(color, 0.3),
                    edge * 0.6,
                );
            }
            hull(emit, p, 0, r * 0.96, 0., lighten(color, 0.45), edge);
            poly(emit, p + ahead * r * 0.17, 0, r * 0.8, 0., glow);
            poly(
                emit,
                p + ahead * r * 0.2,
                0,
                r * 0.76,
                0.,
                Color::new(0.1, 0.07, 0.16, color.a),
            );
        }
        BossKind::Guardian => {
            // A broad wedge that launches its swarm from a bay at the back,
            // with a pentagon set at each point.
            mount(emit, aim + PI, 0.64, 0.9, 1.6);
            hull(emit, p, 3, r, aim, color, edge);
            hull(
                emit,
                p - ahead * r * 0.12,
                3,
                r * 0.48,
                aim,
                plate_color,
                edge * 0.45,
            );
            for n in 0..3 {
                let a = aim + n as f32 * TAU / 3.;
                hull(
                    emit,
                    p + direction(a) * r * 0.66,
                    5,
                    r * 0.12,
                    a,
                    dark,
                    edge * 0.5,
                );
            }
            poly(emit, p - ahead * r * 0.12, 5, r * 0.16, aim, OUTLINE);
            poly(emit, p - ahead * r * 0.12, 5, r * 0.16 - edge, aim, glow);
        }
        BossKind::Summoner => {
            // A square seal with a bay on every side.
            for n in 0..4 {
                mount(emit, aim + n as f32 * FRAC_PI_2, 0.55, 0.9, 1.6);
            }
            hull(emit, p, 4, r, aim + FRAC_PI_4, color, edge);
            hull(emit, p, 4, r * 0.6, aim, plate_color, edge * 0.45);
            hull(emit, p, 4, r * 0.4, aim + FRAC_PI_4, dark, edge * 0.45);
            poly(emit, p, 4, r * 0.18, aim, OUTLINE);
            poly(emit, p, 4, r * 0.18 - edge, aim, glow);
        }
        BossKind::Defender => {
            // A turning triangle: trap launchers on its sides, a turret at
            // each point watching its target.
            for n in 0..3 {
                mount(emit, turn + PI + n as f32 * TAU / 3., 0.42, 0.94, 1.45);
            }
            hull(emit, p, 3, r, turn, color, edge);
            hull(emit, p, 3, r * 0.42, turn, dark, edge * 0.5);
            // Every turret barrel goes down before any turret ring, so no
            // barrel crosses another turret.
            let turrets = (0..3).map(|n| p + direction(turn + n as f32 * TAU / 3.) * r * 0.51);
            for position in turrets.clone() {
                cannon_profile(
                    emit,
                    position,
                    aim,
                    r * 0.68,
                    r * 0.22,
                    color,
                    0.,
                    1.,
                    Some(edge * 0.7),
                );
            }
            for position in turrets {
                hull(
                    emit,
                    position,
                    0,
                    r * 0.2,
                    0.,
                    Color::new(0.66, 0.66, 0.66, color.a),
                    edge * 0.7,
                );
            }
            poly(emit, p, 3, r * 0.17, turn + PI, glow);
        }
        BossKind::FallenOverlord => {
            for n in 0..4 {
                mount(emit, aim + n as f32 * FRAC_PI_2, 0.47, 0.9, 1.65);
            }
            hull(emit, p, 0, r, 0., color, edge);
            hull(emit, p, 0, r * 0.56, 0., plate_color, edge * 0.4);
            poly(emit, p, 4, r * 0.24, aim + FRAC_PI_4, OUTLINE);
            poly(emit, p, 4, r * 0.24 - edge * 1.4, aim + FRAC_PI_4, glow);
        }
        BossKind::FallenBooster => {
            // A heavy front gun and four thrusters behind.
            for offset in [-0.45, -0.2, 0.2, 0.45] {
                mount(emit, aim + PI + offset, 0.24, 0.95, 1.2);
            }
            mount(emit, aim, 0.46, 1.1, 1.);
            hull(emit, p, 0, r, 0., color, edge);
            hull(emit, p, 0, r * 0.53, 0., plate_color, edge * 0.55);
            poly(emit, p, 0, r * 0.2, 0., OUTLINE);
            poly(emit, p, 0, r * 0.2 - edge, 0., glow);
        }
        BossKind::SiegeEngine => {
            // Two long treads, a square hull, and on its turret ring one
            // huge gun with a muzzle brake. Everything it fires leaves it.
            for side in [-1., 1.] {
                let track = p + across * side * r * 0.64;
                plate(emit, track, vec2(r * 1.46, r * 0.4), aim, OUTLINE);
                plate(
                    emit,
                    track,
                    vec2(r * 1.46 - edge * 2., r * 0.4 - edge * 2.),
                    aim,
                    tint(color, 0.5),
                );
                for n in -3..=3 {
                    plate(
                        emit,
                        track + ahead * n as f32 * r * 0.19,
                        vec2(edge * 0.9, r * 0.3),
                        aim,
                        OUTLINE,
                    );
                }
            }
            hull(emit, p, 4, r * 0.8, aim + FRAC_PI_4, color, edge);
            plate(
                emit,
                p - ahead * r * 0.42,
                vec2(r * 0.16, r * 0.7),
                aim,
                dark,
            );
            cannon_profile(emit, p, aim, r * 1.62, r * 0.34, color, 0., 1., Some(edge));
            let brake = p + ahead * r * 1.48;
            plate(emit, brake, vec2(r * 0.28, r * 0.5), aim, OUTLINE);
            plate(
                emit,
                brake,
                vec2(r * 0.28 - edge * 2., r * 0.5 - edge * 2.),
                aim,
                Color::new(0.6, 0.6, 0.6, color.a),
            );
            hull(emit, p, 0, r * 0.4, 0., plate_color, edge);
            poly(emit, p, 0, r * 0.16, 0., OUTLINE);
            poly(emit, p, 0, r * 0.16 - edge, 0., glow);
        }
        BossKind::Skirmisher => {
            // A small, quick wedge: a gun on each wing, fins swept back.
            for side in [-1., 1.] {
                shard(
                    emit,
                    [
                        p - ahead * r * 0.2 + across * side * r * 0.45,
                        p - ahead * r * 1.12 + across * side * r * 1.05,
                        p - ahead * r * 0.6 + across * side * r * 0.25,
                    ],
                    dark,
                    edge,
                );
                cannon_profile(
                    emit,
                    p + direction(aim + side * 1.1) * r * 0.45,
                    aim,
                    r * 0.91,
                    r * 0.23,
                    color,
                    0.,
                    1.,
                    Some(edge),
                );
            }
            hull(emit, p, 3, r, aim, color, edge);
            hull(
                emit,
                p - ahead * r * 0.06,
                3,
                r * 0.5,
                aim,
                plate_color,
                edge * 0.45,
            );
            poly(emit, p + ahead * r * 0.12, 4, r * 0.18, aim, OUTLINE);
            poly(
                emit,
                p + ahead * r * 0.12,
                4,
                r * 0.18 - edge * 1.2,
                aim,
                glow,
            );
        }
        BossKind::Trapper => {
            // A hexagon with two trap launchers ahead and a mine rack on
            // each side, where its pockets of mines come from.
            for side in [-1., 1.] {
                mount(emit, aim + side * 0.55, 0.4, 0.98, 1.4);
                mount(emit, aim + side * FRAC_PI_2, 0.5, 0.7, 1.55);
            }
            hull(emit, p, 6, r, aim + PI / 6., color, edge);
            hull(
                emit,
                p,
                6,
                r * 0.56,
                aim + PI / 6.,
                plate_color,
                edge * 0.45,
            );
            poly(emit, p, 3, r * 0.24, aim, OUTLINE);
            poly(emit, p, 3, r * 0.24 - edge * 1.6, aim, glow);
        }
        BossKind::Railgunner => railgunner_model(p, r, pose, team_color, emit),
    }
}

/// The Railgunner's ion light, and the violet at the edge of it.
pub const RAIL_ION: Color = Color::new(0.35, 0.92, 1., 1.);
pub const RAIL_FRINGE: Color = Color::new(0.76, 0.35, 1., 1.);
const RAIL_STEEL: Color = Color::new(0.30, 0.38, 0.52, 1.);
/// Where the coil clamps sit along its barrel, in hull radii.
pub const RAIL_COILS: [f32; 7] = [0.68, 0.92, 1.16, 1.4, 1.64, 1.88, 2.12];
/// How far a shot throws its rails back, in hull radii.
pub const RAIL_THROW: f32 = 0.2;
/// Below this radius it's an icon, and fine detail would only be noise.
const RAIL_FINE: f32 = 26.;
/// The hinge each armored half of the barrel swings on, as (along, out).
const RAIL_HINGE: (f32, f32) = (0.22, 0.34);
/// The muzzle's claws on each half, as (how far out the hinge is, length, how
/// far behind the other it opens).
const RAIL_CLAWS: [(f32, f32, f32); 2] = [(0.075, 0.36, 0.), (0.17, 0.29, 0.24)];

/// One part's share of the barrel's move: nothing before `from`, all of it by
/// `to`, starting and stopping gently.
fn rail_stage(open: f32, from: f32, to: f32) -> f32 {
    let x = (open - from) / (to - from);
    // Written so a NaN counts as not started.
    let x = if x > 0. { x.min(1.) } else { 0. };
    x * x * x * (x * (x * 6. - 15.) + 10.)
}

/// Where every moving part of the Railgunner's barrel is at one point of its
/// opening. The parts go in order: the armor unlatches and swings wide, the
/// rails stand off the bore, the coils part from breech to muzzle, and the
/// claws unfold last. Shutting runs the same order backward. The model and its
/// lights both read this, so a light is always on the part it belongs to.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RailBarrel {
    /// How far each armored half has swung off the bore, in radians.
    pub swing: f32,
    /// How far each rail stands off the middle at the breech and the muzzle.
    pub rail_root: f32,
    pub rail_tip: f32,
    /// Where each coil's half starts, out from the middle, breech first.
    pub gaps: [f32; RAIL_COILS.len()],
    /// How far the claws have unfolded, in radians.
    pub claw: f32,
    /// How far the barrel's front section has run out on its slides.
    pub reach: f32,
}

impl RailBarrel {
    pub fn at(open: f32) -> Self {
        let open = if open.is_finite() {
            open.clamp(-0.1, 1.3)
        } else {
            0.
        };
        // A spring carries it a little past each end before it settles.
        let (over, shy) = ((open - 1.).max(0.), open.min(0.));
        let armor = rail_stage(open, 0., 0.55);
        let rails = rail_stage(open, 0.16, 0.7);
        let claws = rail_stage(open, 0.46, 1.);
        let mut gaps = [0.; RAIL_COILS.len()];
        for (index, gap) in gaps.iter_mut().enumerate() {
            let u = index as f32 / (RAIL_COILS.len() - 1) as f32;
            *gap = 0.012 + 0.1 * rail_stage(open, 0.14 + 0.42 * u, 0.4 + 0.52 * u) + 0.06 * over;
        }
        Self {
            swing: 0.19 * armor + 0.16 * over + 0.03 * shy,
            rail_root: 0.15 + 0.06 * rails + 0.04 * over,
            rail_tip: 0.125 + 0.125 * rails + 0.08 * over,
            gaps,
            claw: 0.98 * claws + 0.5 * over,
            reach: 0.1 * rail_stage(open, 0.3, 0.86) + 0.05 * over,
        }
    }

    /// Where a coil sits along the barrel. The front ones ride the section
    /// that runs out.
    pub fn coil_along(&self, index: usize, along: f32) -> f32 {
        along + self.reach * ((index as f32 - 3.) / 3.).clamp(0., 1.)
    }

    /// A point on one armored half, given as it lies shut, where it is now.
    /// `side` is -1 or 1, `out` is measured away from the middle, and the
    /// result is (along, out) with the side's sign.
    pub fn on_armor(&self, side: f32, along: f32, out: f32) -> (f32, f32) {
        let (x, y) = (along - RAIL_HINGE.0, out - RAIL_HINGE.1);
        let (sin, cos) = self.swing.sin_cos();
        (
            RAIL_HINGE.0 + x * cos - y * sin,
            side * (RAIL_HINGE.1 + x * sin + y * cos),
        )
    }

    /// The hinge and tip of one of the muzzle's claws, as (along, out).
    pub fn claw(&self, side: f32, which: usize) -> ((f32, f32), (f32, f32)) {
        let (hinge_out, length, late) = RAIL_CLAWS[which.min(RAIL_CLAWS.len() - 1)];
        let hinge = self.on_armor(side, 2.04 + self.reach, hinge_out);
        let angle = self.swing - 0.13 + self.claw * (1. - late);
        (
            hinge,
            (
                hinge.0 + angle.cos() * length,
                hinge.1 + side * angle.sin() * length,
            ),
        )
    }
}

/// A tapered bar from `a` to `b`, outlined when `edge` is above zero.
fn bar(
    emit: &mut impl FnMut(Primitive),
    (a, b): (Vec2, Vec2),
    (half_a, half_b): (f32, f32),
    color: Color,
    edge: f32,
) {
    let way = (b - a).normalize_or_zero();
    let out = vec2(-way.y, way.x);
    let corners = |grow: f32| {
        [
            a - way * grow - out * (half_a + grow),
            b + way * grow - out * (half_b + grow),
            b + way * grow + out * (half_b + grow),
            a - way * grow + out * (half_a + grow),
        ]
    };
    if edge > 0. {
        emit(Primitive::Quad {
            vertices: corners(edge),
            color: Color {
                a: color.a,
                ..OUTLINE
            },
        });
    }
    emit(Primitive::Quad {
        vertices: corners(0.),
        color,
    });
}

/// An outlined four-sided armor plate.
fn facet(emit: &mut impl FnMut(Primitive), corners: [Vec2; 4], color: Color, edge: f32) {
    let middle = (corners[0] + corners[1] + corners[2] + corners[3]) / 4.;
    emit(Primitive::Quad {
        vertices: corners.map(|at| at + (at - middle).normalize_or_zero() * edge * 1.5),
        color: Color {
            a: color.a,
            ..OUTLINE
        },
    });
    emit(Primitive::Quad {
        vertices: corners,
        color,
    });
}

/// A share from 0 to 1, whatever it was handed.
fn share(value: f32) -> f32 {
    if value.is_finite() {
        value.clamp(0., 1.)
    } else {
        0.
    }
}

/// The Railgunner: an armored octagon on two engines, carrying a barrel longer
/// than the rest of it. The barrel is two armored halves on hinges, closed
/// around a pair of rails. For the beam they swing apart on hydraulic rams, the
/// coils part, and claws unfold at the muzzle. The rails kick back when it
/// fires, the fins fold out to dump heat, and the dish never stops. The game
/// lays its light over this with `BossFx::rail_lights`, which reads the same
/// [`RailBarrel`].
fn railgunner_model(
    p: Vec2,
    r: f32,
    pose: Pose,
    team_color: Option<Color>,
    emit: &mut impl FnMut(Primitive),
) {
    let Pose {
        aim,
        time,
        charge,
        recoil,
        beam,
        vent,
        open: split,
        ..
    } = pose;
    let ahead = vec2(aim.cos(), aim.sin());
    let side = vec2(-ahead.y, ahead.x);
    let at = |along: f32, out: f32| p + ahead * r * along + side * r * out;
    let fine = r >= RAIL_FINE;
    let edge = (r * 0.034).max(0.6);
    let steel = team_color.map_or(RAIL_STEEL, |team| tint(team, 0.62));
    let accent = team_color.unwrap_or(RAIL_ION);
    let (mid, deep, pale) = (tint(steel, 0.8), tint(steel, 0.55), lighten(steel, 0.36));
    let thrown = share(recoil) * RAIL_THROW;
    let open = share(vent);
    let live = share(charge).max(share(beam));
    let lamp = tint(accent, 0.5 + 0.5 * live);
    // Two engines, set back on its flanks.
    for out in [-1., 1.] {
        bar(
            emit,
            (at(-0.28, out * 0.58), at(-1.18, out * 0.68)),
            (r * 0.28, r * 0.2),
            mid,
            edge,
        );
        bar(
            emit,
            (at(-1.1, out * 0.67), at(-1.32, out * 0.7)),
            (r * 0.14, r * 0.19),
            deep,
            edge,
        );
        if fine {
            bar(
                emit,
                (at(-0.42, out * 0.6), at(-1.02, out * 0.66)),
                (r * 0.05, r * 0.035),
                deep,
                0.,
            );
            // Cooling ribs across the engine, and the ring of its nozzle.
            for rib in 0..4 {
                let along = -0.5 - rib as f32 * 0.15;
                let width = 0.235 - rib as f32 * 0.016;
                let middle = 0.6 + (along + 0.42) * -0.1;
                bar(
                    emit,
                    (
                        at(along, out * (middle - width)),
                        at(along, out * (middle + width)),
                    ),
                    (r * 0.014, r * 0.014),
                    deep,
                    0.,
                );
            }
            bar(
                emit,
                (at(-1.27, out * 0.555), at(-1.27, out * 0.845)),
                (r * 0.02, r * 0.02),
                pale,
                0.,
            );
        }
    }
    // Heat fins, hinged at the hull. They fold out when it vents.
    for out in [-1., 1.] {
        for (index, root) in [1.94_f32, 2.36, 2.78].into_iter().enumerate() {
            let hinge = aim + out * root;
            let lean = aim + out * (root + open * (0.36 - index as f32 * 0.18));
            let base = p + vec2(hinge.cos(), hinge.sin()) * r * 0.9;
            let way = vec2(lean.cos(), lean.sin());
            let across = vec2(-way.y, way.x) * r * 0.1;
            shard(
                emit,
                [
                    base + across,
                    base + way * r * (0.5 + 0.17 * open),
                    base - across,
                ],
                if index == 1 { pale } else { mid },
                edge * 0.8,
            );
        }
    }
    // Flank guns: all it has for anyone who gets close.
    for out in [-1., 1.] {
        cannon_profile(
            emit,
            at(0.18, out * 0.77),
            aim,
            r * 0.58,
            r * 0.15,
            steel,
            0.,
            1.14,
            Some(edge),
        );
    }
    hull(emit, p, 8, r, aim + PI / 8., steel, edge);
    hull(emit, p, 8, r * 0.83, aim + PI / 8., mid, edge * 0.5);
    for out in [-1., 1.] {
        // Cheek armor either side of the barrel.
        facet(
            emit,
            [
                at(0.3, out * 0.33),
                at(0.74, out * 0.3),
                at(0.5, out * 0.66),
                at(0.04, out * 0.72),
            ],
            steel,
            edge * 0.7,
        );
        // Plates along the rear deck.
        facet(
            emit,
            [
                at(-0.12, out * 0.72),
                at(-0.3, out * 0.3),
                at(-0.68, out * 0.26),
                at(-0.56, out * 0.6),
            ],
            deep,
            edge * 0.55,
        );
    }
    if fine {
        // Chevrons running up the spine, pointing where it means to shoot.
        for step in 0..3 {
            let tip = -0.5 + step as f32 * 0.11;
            for out in [-1., 1.] {
                bar(
                    emit,
                    (at(tip, 0.), at(tip - 0.17, out * 0.2)),
                    (r * 0.026, r * 0.026),
                    lamp,
                    0.,
                );
            }
        }
    }
    let barrel = RailBarrel::at(split);
    let (back, front) = (0.3 - thrown, 2.31 - thrown);
    // The armor rides back a little with the rails, on its hinge.
    let armor_back = thrown * 0.3;
    let black = Color {
        a: steel.a,
        ..OUTLINE
    };
    // The bore, dark until there's something in it.
    bar(
        emit,
        (at(back, 0.), at(front - 0.02, 0.)),
        (r * (barrel.rail_root + 0.07), r * (barrel.rail_tip + 0.06)),
        black,
        0.,
    );
    if fine {
        // Rungs across the bore, seen once the armor has parted.
        for rung in 0..9 {
            let along = back + 0.16 + rung as f32 * 0.205;
            let u = (along - back) / (front - back);
            let half = barrel.rail_root + (barrel.rail_tip - barrel.rail_root) * u;
            bar(
                emit,
                (at(along, -half), at(along, half)),
                (r * 0.013, r * 0.013),
                deep,
                0.,
            );
        }
    }
    for out in [-1., 1.] {
        bar(
            emit,
            (
                at(back, out * barrel.rail_root),
                at(front, out * barrel.rail_tip),
            ),
            (r * 0.05, r * 0.04),
            pale,
            edge * 0.8,
        );
        if fine {
            bar(
                emit,
                (
                    at(back + 0.04, out * (barrel.rail_root - 0.014)),
                    at(front - 0.04, out * (barrel.rail_tip - 0.012)),
                ),
                (r * 0.014, r * 0.011),
                lighten(pale, 0.5),
                0.,
            );
        }
    }
    // The two armored halves, swinging on their hinges at the breech.
    for out in [-1., 1.] {
        let on = |along: f32, off: f32| {
            let (run, across) = barrel.on_armor(out, along - armor_back, off);
            at(run, across)
        };
        facet(
            emit,
            [
                on(0.24, 0.09),
                on(0.18, 0.38),
                on(1.52, 0.31),
                on(1.6, 0.082),
            ],
            steel,
            edge,
        );
        facet(
            emit,
            [
                on(1.57 + barrel.reach, 0.082),
                on(1.53 + barrel.reach, 0.296),
                on(1.98 + barrel.reach, 0.22),
                on(2.07 + barrel.reach, 0.07),
            ],
            mid,
            edge * 0.8,
        );
        if fine {
            // A slit down the plate that lights as it charges.
            bar(
                emit,
                (on(0.42, 0.255), on(1.46, 0.232)),
                (r * 0.015, r * 0.011),
                lamp,
                0.,
            );
            // A latch on the seam, the first thing to let go.
            let latch = rail_stage(split, 0., 0.14) * 0.05;
            plate(
                emit,
                {
                    let (run, across) = barrel.on_armor(out, 1.57 - armor_back, 0.1 + latch);
                    at(run, across)
                },
                vec2(r * 0.15, r * 0.075),
                aim,
                pale,
            );
        }
    }
    // Each coil is two halves that ride the armor and part down the barrel.
    for (index, along) in RAIL_COILS.into_iter().enumerate() {
        let reach = 0.335 - index as f32 * 0.014;
        let along = barrel.coil_along(index, along);
        for out in [-1., 1.] {
            let on = |off: f32| {
                let (run, across) = barrel.on_armor(out, along - thrown, off);
                at(run, across)
            };
            let gap = barrel.gaps[index];
            bar(
                emit,
                (on(gap), on(reach)),
                (r * 0.042, r * 0.042),
                steel,
                edge * 0.75,
            );
            if fine {
                bar(
                    emit,
                    (on(gap + 0.04), on(reach - 0.04)),
                    (r * 0.012, r * 0.012),
                    lamp,
                    0.,
                );
            }
        }
    }
    // The rams that do the work, from the hull to each half.
    for out in [-1., 1.] {
        let base = at(0.26, out * 0.63);
        let (run, across) = barrel.on_armor(out, 1.14 - armor_back, 0.335);
        let end = at(run, across);
        let sleeve = base.lerp(end, 0.52);
        bar(
            emit,
            (sleeve, end),
            (r * 0.024, r * 0.024),
            lighten(pale, 0.3),
            edge * 0.5,
        );
        bar(
            emit,
            (base, sleeve),
            (r * 0.052, r * 0.046),
            mid,
            edge * 0.7,
        );
        if fine {
            hull(emit, base, 0, r * 0.05, 0., steel, edge * 0.5);
            poly(emit, base, 0, r * 0.018, 0., pale);
            hull(emit, end, 0, r * 0.036, 0., pale, edge * 0.5);
        }
    }
    // The muzzle's claws. They close into a point and unfold for the beam.
    for out in [-1., 1.] {
        for which in 0..RAIL_CLAWS.len() {
            let (hinge, tip) = barrel.claw(out, which);
            let (hinge, tip) = (at(hinge.0 - thrown, hinge.1), at(tip.0 - thrown, tip.1));
            let way = (tip - hinge).normalize_or_zero();
            let across = vec2(-way.y, way.x) * r * (0.062 - which as f32 * 0.012);
            shard(
                emit,
                [hinge + across, tip, hinge - across],
                if which == 0 { pale } else { steel },
                edge * 0.7,
            );
            if fine {
                let middle = hinge.lerp(tip, 0.62);
                bar(emit, (hinge, middle), (r * 0.012, r * 0.004), lamp, 0.);
            }
        }
    }
    // The turret, ringed with capacitors, with the core in the middle.
    hull(emit, p, 12, r * 0.62, aim, steel, edge);
    hull(emit, p, 12, r * 0.47, aim + PI / 12., deep, edge * 0.6);
    const CELLS: usize = 10;
    for cell in 0..CELLS {
        let turn = aim + (cell as f32 + 0.5) / CELLS as f32 * TAU;
        hull(
            emit,
            p + vec2(turn.cos(), turn.sin()) * r * 0.545,
            4,
            r * 0.062,
            turn + FRAC_PI_4,
            lamp,
            edge * 0.45,
        );
    }
    hull(emit, p, 8, r * 0.3, aim + PI / 8., mid, edge * 0.7);
    poly(emit, p, 0, r * 0.17, 0., OUTLINE);
    poly(
        emit,
        p,
        0,
        r * 0.17 - edge,
        0.,
        lighten(accent, 0.45 + 0.5 * live),
    );
    if fine {
        // The dish that finds them, turning on the turret's back.
        let dish = at(-0.38, 0.);
        hull(emit, dish, 0, r * 0.13, 0., steel, edge * 0.5);
        plate(emit, dish, vec2(r * 0.24, r * 0.05), time * 1.7, pale);
        poly(emit, dish, 0, r * 0.04, 0., lamp);
        // The rangefinder on its shoulder, with a lens that watches you.
        let optic = at(0.12, 0.3);
        plate(
            emit,
            optic,
            vec2(r * 0.26 + edge * 2., r * 0.11 + edge * 2.),
            aim,
            OUTLINE,
        );
        plate(emit, optic, vec2(r * 0.26, r * 0.11), aim, mid);
        let lens = at(0.22, 0.3);
        poly(emit, lens, 0, r * 0.043, 0., OUTLINE);
        poly(
            emit,
            lens,
            0,
            r * 0.03,
            0.,
            lighten(accent, 0.2 + 0.7 * live),
        );
    }
}

/// Axiom's gold.
pub const AXIOM_GOLD: Color = Color::new(1., 0.79, 0.34, 1.);
/// The teal of Axiom's rings.
pub const AXIOM_TEAL: Color = Color::new(0.20, 0.80, 0.87, 1.);
const AXIOM_ARMOR: Color = Color::new(0.23, 0.31, 0.39, 1.);
const AXIOM_PALE: Color = Color::new(1., 0.96, 0.78, 1.);
/// How fast Axiom's rings turn, in radians a second.
pub const AXIOM_TURN: f32 = 0.17;
/// How far a firing cannon is thrown back, in hull radii.
pub const AXIOM_RECOIL: f32 = 0.1;
/// Below this radius Axiom is an icon, and fine detail would only be noise.
const AXIOM_FINE: f32 = 24.;

/// How Axiom is posed when drawn.
#[derive(Clone, Copy, Debug, Default)]
pub struct AxiomPose {
    /// Where its forward cannon points.
    pub aim: f32,
    /// Seconds, for its turning rings; held still when motion is reduced.
    pub time: f32,
    /// How far back each cannon is, 0 to 1, from the one on its aim.
    pub recoil: [f32; CANNONS],
}

/// Axiom at rest: six articulated cannon petals around an armored astrolabe.
pub fn axiom_geometry(p: Vec2, r: f32, aim: f32, time: f32, emit: &mut impl FnMut(Primitive)) {
    axiom_model(
        p,
        r,
        AxiomPose {
            aim,
            time,
            ..AxiomPose::default()
        },
        emit,
    );
}

/// Axiom in a pose. The game lays its light over this with
/// `BossFx::axiom_glow`, which follows the same rings at the same angles.
pub fn axiom_model(p: Vec2, r: f32, pose: AxiomPose, emit: &mut impl FnMut(Primitive)) {
    let AxiomPose { aim, time, recoil } = pose;
    let edge = (r * 0.035).max(0.7);
    let turn = time * AXIOM_TURN;
    let fine = r >= AXIOM_FINE;
    let step = TAU / CANNONS as f32;
    if fine {
        // Slim fins between the cannons, so the outline reads as a star.
        for i in 0..CANNONS {
            let a = aim + (i as f32 + 0.5) * step;
            let d = vec2(a.cos(), a.sin());
            let n = vec2(-d.y, d.x);
            shard(
                emit,
                [
                    p + d * r * 0.86 + n * r * 0.1,
                    p + d * r * 1.14,
                    p + d * r * 0.86 - n * r * 0.1,
                ],
                tint(AXIOM_GOLD, 0.86),
                edge * 0.8,
            );
        }
    }
    for (i, &kick) in recoil.iter().enumerate() {
        let a = aim + i as f32 * step;
        let d = vec2(a.cos(), a.sin());
        let thrown = if kick.is_finite() {
            kick.clamp(0., 1.) * r * AXIOM_RECOIL
        } else {
            0.
        };
        cannon_profile(
            emit,
            p + d * r * 0.6,
            a,
            r * (MUZZLE - 0.6),
            r * 0.25,
            AXIOM_ARMOR,
            thrown,
            1.16,
            Some(edge),
        );
        if fine {
            // A teal seam down the barrel and a gold band at its mouth.
            plate(
                emit,
                p + d * (r * (MUZZLE + 0.92) * 0.5 - thrown),
                vec2(r * (MUZZLE - 1.12), r * 0.034),
                a,
                AXIOM_TEAL,
            );
            let band = p + d * (r * (MUZZLE - 0.045) - thrown);
            plate(
                emit,
                band,
                vec2(r * 0.09 + edge * 2., r * 0.33 + edge * 2.),
                a,
                OUTLINE,
            );
            plate(emit, band, vec2(r * 0.09, r * 0.33), a, AXIOM_GOLD);
        }
        // The shoulder it recoils into.
        let shoulder = p + d * (r * 0.96 - thrown * 0.3);
        plate(
            emit,
            shoulder,
            vec2(r * 0.13 + edge * 2., r * 0.37 + edge * 2.),
            a,
            OUTLINE,
        );
        plate(
            emit,
            shoulder,
            vec2(r * 0.13, r * 0.37),
            a,
            tint(AXIOM_GOLD, 0.78),
        );
    }
    hull(emit, p, 12, r * 0.94, turn, AXIOM_ARMOR, edge);
    hull(emit, p, 6, r * 0.78, -turn, tint(AXIOM_GOLD, 0.88), edge);
    hull(emit, p, 6, r * 0.66, -turn, AXIOM_ARMOR, edge * 0.6);
    for i in 0..CANNONS {
        let a = turn + i as f32 * step;
        hull(
            emit,
            p + vec2(a.cos(), a.sin()) * r * 0.84,
            4,
            r * 0.08,
            a + FRAC_PI_4,
            AXIOM_TEAL,
            edge * 0.5,
        );
    }
    if fine {
        // Two small worlds riding the astrolabe's inner track.
        for (lead, color) in [(0., AXIOM_GOLD), (PI, AXIOM_TEAL)] {
            let a = -turn * 2.4 + lead;
            hull(
                emit,
                p + vec2(a.cos(), a.sin()) * r * 0.565,
                0,
                r * 0.06,
                0.,
                color,
                edge * 0.5,
            );
        }
    }
    hull(emit, p, 6, r * 0.47, turn, tint(AXIOM_TEAL, 0.56), edge);
    hull(emit, p, 6, r * 0.34, -turn, AXIOM_ARMOR, edge * 0.7);
    if fine {
        // The astrolabe's needle, which always shows where it's aiming.
        let d = vec2(aim.cos(), aim.sin());
        let n = vec2(-d.y, d.x);
        shard(
            emit,
            [p + d * r * 0.58, p + n * r * 0.075, p - n * r * 0.075],
            lighten(AXIOM_GOLD, 0.2),
            edge * 0.7,
        );
        shard(
            emit,
            [p - d * r * 0.34, p - n * r * 0.075, p + n * r * 0.075],
            AXIOM_ARMOR,
            edge * 0.7,
        );
    }
    hull(
        emit,
        p,
        4,
        r * 0.23,
        turn + FRAC_PI_4,
        AXIOM_GOLD,
        edge * 0.6,
    );
    poly(emit, p, 4, r * 0.12, turn + FRAC_PI_4, AXIOM_PALE);
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn axiom_stays_in_its_bounds_in_any_pose_and_simplifies_at_icon_size() {
        let count = |r: f32, pose: AxiomPose| {
            let mut pieces = 0;
            axiom_model(Vec2::ZERO, r, pose, &mut |primitive| {
                pieces += 1;
                let points = match primitive {
                    Primitive::Polygon { center, radius, .. } => {
                        vec![center - Vec2::splat(radius), center + Vec2::splat(radius)]
                    }
                    Primitive::Triangle { vertices, .. } => vertices.to_vec(),
                    Primitive::Quad { vertices, .. } => vertices.to_vec(),
                };
                for point in points {
                    assert!(point.is_finite() && point.length() < r * 2.);
                }
            });
            pieces
        };
        let thrown = AxiomPose {
            aim: 1.3,
            time: 900.,
            recoil: [1., 0., 0.5, 7., -3., f32::NAN],
        };
        let full = count(180., thrown);
        assert!(count(180., AxiomPose::default()) == full);
        assert!(count(7., thrown) < full, "an icon draws less");
    }

    #[test]
    fn the_railgunner_barrel_opens_in_order_and_stays_in_bounds() {
        let shut = RailBarrel::at(0.);
        let full = RailBarrel::at(1.);
        assert_eq!(shut.swing, 0.);
        assert_eq!(shut.claw, 0.);
        assert!(full.swing > 0.15 && full.claw > 0.9);
        assert!(full.rail_tip > shut.rail_tip + 0.1, "the rails stand off");
        // The armor leads and the claws come last.
        let part = RailBarrel::at(0.42);
        assert!(part.swing > 0.6 * full.swing);
        assert_eq!(part.claw, 0.);
        // The coils part from the breech toward the muzzle.
        let halfway = RailBarrel::at(0.45);
        for pair in halfway.gaps.windows(2) {
            assert!(pair[0] >= pair[1], "{:?}", halfway.gaps);
        }
        for open in [-3., -0.08, 0., 0.3, 0.7, 1., 1.2, 9., f32::NAN] {
            let barrel = RailBarrel::at(open);
            for side in [-1., 1.] {
                for which in 0..2 {
                    let (hinge, tip) = barrel.claw(side, which);
                    assert!(hinge.0.is_finite() && tip.0.is_finite());
                    assert!((tip.0 * tip.0 + tip.1 * tip.1).sqrt() < 2.6, "{open}");
                    assert!(tip.1 * side > -0.02, "a claw crossed the bore at {open}");
                }
                let (_, out) = barrel.on_armor(side, 1., 0.2);
                assert!(out * side > 0., "the armor stays on its own side");
            }
            let pose = Pose {
                aim: 0.7,
                open,
                vent: 1.,
                beam: 1.,
                recoil: 1.,
                ..Pose::default()
            };
            boss_model_geometry(
                BossKind::Railgunner,
                Vec2::ZERO,
                100.,
                pose,
                None,
                &mut |primitive| {
                    let points = match primitive {
                        Primitive::Polygon { center, radius, .. } => {
                            vec![center - Vec2::splat(radius), center + Vec2::splat(radius)]
                        }
                        Primitive::Triangle { vertices, .. } => vertices.to_vec(),
                        Primitive::Quad { vertices, .. } => vertices.to_vec(),
                    };
                    for point in points {
                        assert!(point.is_finite() && point.length() < 275., "{open}");
                    }
                },
            );
        }
    }

    #[test]
    fn all_boss_silhouettes_are_finite_bounded_and_distinct() {
        let mut signatures = std::collections::HashSet::new();
        for kind in BossKind::ALL {
            let mut signature = String::new();
            boss_model_geometry(
                kind,
                Vec2::ZERO,
                kind.radius(),
                Pose {
                    aim: 0.4,
                    turn: 1.,
                    time: 2.,
                    phase: 2,
                    charge: 0.5,
                    lit: false,
                    ..Pose::default()
                },
                None,
                &mut |primitive| {
                    let points = match primitive {
                        Primitive::Polygon { center, radius, .. } => {
                            vec![center - Vec2::splat(radius), center + Vec2::splat(radius)]
                        }
                        Primitive::Triangle { vertices, .. } => vertices.to_vec(),
                        Primitive::Quad { vertices, .. } => vertices.to_vec(),
                    };
                    for point in points {
                        assert!(point.is_finite() && point.length() < kind.radius() * 2.5);
                    }
                    signature.push_str(&format!("{primitive:?}"));
                },
            );
            assert!(signatures.insert(signature), "duplicate model {kind:?}");
        }
    }
}
