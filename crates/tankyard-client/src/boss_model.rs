//! Original vector models, shared by the game and the boss model sheet exporter.
use crate::tank_model::{OUTLINE, Primitive, cannon_profile};
use macroquad::prelude::{Color, Vec2, vec2};
use std::f32::consts::{FRAC_PI_2, FRAC_PI_4, PI, TAU};
use tankyard_core::BossKind;

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
    poly(emit, center, sides, radius, angle, color);
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

#[allow(clippy::too_many_arguments)]
pub fn boss_model_geometry(
    kind: BossKind,
    p: Vec2,
    r: f32,
    aim: f32,
    time: f32,
    _phase: u8,
    team_color: Option<Color>,
    emit: &mut impl FnMut(Primitive),
) {
    if !r.is_finite() || r <= 0. || !aim.is_finite() || !time.is_finite() {
        return;
    }
    let color = team_color.unwrap_or_else(|| boss_color(kind));
    let edge = (r * 0.045).max(0.6);
    let rotate = time * 0.16;
    let direction = |angle: f32| vec2(angle.cos(), angle.sin());
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
        BossKind::Guardian => {
            mount(emit, aim + PI, 0.64, 0.90, 1.6);
            hull(emit, p, 3, r, aim, color, edge);
            poly(
                emit,
                p - direction(aim) * r * 0.12,
                3,
                r * 0.44,
                aim,
                tint(color, 0.93),
            );
        }
        BossKind::Summoner => {
            for n in 0..4 {
                mount(emit, aim + n as f32 * FRAC_PI_2, 0.55, 0.90, 1.6);
            }
            hull(emit, p, 4, r, aim + FRAC_PI_4, color, edge);
            hull(
                emit,
                p,
                4,
                r * 0.44,
                aim + FRAC_PI_4,
                tint(color, 0.96),
                edge * 0.4,
            );
        }
        BossKind::FallenOverlord => {
            for n in 0..4 {
                mount(emit, aim + n as f32 * FRAC_PI_2, 0.47, 0.90, 1.65);
            }
            hull(emit, p, 0, r, 0., color, edge);
            hull(emit, p, 0, r * 0.56, 0., tint(color, 0.94), edge * 0.4);
            poly(emit, p, 4, r * 0.2, aim + FRAC_PI_4, OUTLINE);
        }
        BossKind::FallenBooster => {
            for offset in [-0.45, -0.2, 0.2, 0.45] {
                mount(emit, aim + PI + offset, 0.24, 0.95, 1.);
            }
            mount(emit, aim, 0.46, 1.10, 1.);
            hull(emit, p, 0, r, 0., color, edge);
            hull(emit, p, 0, r * 0.53, 0., tint(color, 0.9), edge * 0.55);
        }
        BossKind::Defender => {
            for n in 0..3 {
                mount(emit, rotate + n as f32 * TAU / 3. + PI, 0.42, 0.94, 1.45);
            }
            hull(emit, p, 3, r, rotate, color, edge);
            for n in 0..3 {
                let position = p + direction(rotate + n as f32 * TAU / 3.) * r * 0.51;
                cannon_profile(
                    emit,
                    position,
                    aim,
                    r * 0.75,
                    r * 0.25,
                    color,
                    0.,
                    1.,
                    Some(edge * 0.7),
                );
                hull(
                    emit,
                    position,
                    0,
                    r * 0.23,
                    0.,
                    Color::new(0.66, 0.66, 0.66, 1.),
                    edge * 0.7,
                );
            }
        }
        BossKind::Sentinel => {
            for side in [-1., 1.] {
                mount(emit, aim + side * 0.36, 0.34, 1.0, 1.);
            }
            hull(emit, p, 6, r, aim, color, edge);
            hull(emit, p, 6, r * 0.6, aim, tint(color, 0.9), edge * 0.45);
            // A visor across the front, turned where it watches.
            plate(
                emit,
                p + direction(aim) * r * 0.22,
                vec2(r * 0.15, r * 0.5),
                aim,
                tint(color, 0.55),
            );
        }
        BossKind::Prism => {
            for n in 0..3 {
                mount(emit, rotate + n as f32 * TAU / 3., 0.25, 0.85, 1.);
            }
            hull(emit, p, 3, r, rotate, color, edge);
            let a = p + direction(rotate) * r * 0.77;
            let b = p + direction(rotate + TAU / 3.) * r * 0.77;
            emit(Primitive::Triangle {
                vertices: [p, a, b],
                color: tint(color, 1.13),
            });
            poly(emit, p, 3, r * 0.25, rotate, tint(color, 0.62));
        }
        BossKind::Foundry => {
            for side in [-1., 1.] {
                mount(emit, aim + side * 1.0, 0.48, 0.82, 1.5);
                let pod = p + direction(aim + side * FRAC_PI_2) * r * 0.64;
                hull(
                    emit,
                    pod,
                    4,
                    r * 0.38,
                    aim + FRAC_PI_4,
                    tint(color, 0.72),
                    edge * 0.7,
                );
            }
            hull(emit, p, 4, r * 0.89, aim + FRAC_PI_4, color, edge);
            plate(emit, p, vec2(r * 0.64, r * 0.46), aim, tint(color, 0.5));
            for n in -1..=1 {
                plate(
                    emit,
                    p + direction(aim) * n as f32 * r * 0.19,
                    vec2(r * 0.07, r * 0.36),
                    aim,
                    tint(color, 1.1),
                );
            }
        }
        BossKind::Tempest => {
            for side in [-1., 1.] {
                mount(emit, aim + PI + side * 0.5, 0.3, 0.9, 1.);
            }
            mount(emit, aim, 0.3, 1.05, 1.);
            hull(emit, p, 3, r, aim, color, edge);
            emit(Primitive::Triangle {
                vertices: [
                    p + direction(aim) * r * 0.7,
                    p + direction(aim + 2.5) * r * 0.43,
                    p + direction(aim - 2.5) * r * 0.43,
                ],
                color: tint(color, 0.68),
            });
        }
        BossKind::Bastion => {
            // A square keep with round towers at its corners; the gun turns
            // on a ring in the middle.
            hull(emit, p, 4, r, FRAC_PI_4, color, edge);
            hull(
                emit,
                p,
                4,
                r * 0.7,
                FRAC_PI_4,
                tint(color, 0.86),
                edge * 0.45,
            );
            for n in 0..4 {
                let corner = p + direction(FRAC_PI_4 + n as f32 * FRAC_PI_2) * r * 0.76;
                hull(emit, corner, 0, r * 0.25, 0., tint(color, 0.78), edge * 0.7);
                poly(emit, corner, 0, r * 0.1, 0., tint(color, 0.62));
            }
            cannon_profile(emit, p, aim, r * 1.52, r * 0.46, color, 0., 1., Some(edge));
            hull(emit, p, 0, r * 0.34, 0., tint(color, 0.95), edge * 0.8);
            poly(emit, p, 4, r * 0.13, aim + FRAC_PI_4, tint(color, 0.6));
        }
        BossKind::Eclipse => {
            mount(emit, aim + 0.8, 0.24, 0.82, 1.);
            mount(emit, aim - 0.8, 0.24, 0.82, 1.);
            hull(emit, p, 6, r, aim, tint(color, 0.76), edge);
            hull(
                emit,
                p - direction(aim) * r * 0.16,
                6,
                r * 0.63,
                aim,
                color,
                edge * 0.6,
            );
            plate(
                emit,
                p + direction(aim) * r * 0.25,
                vec2(r * 0.16, r * 0.72),
                aim,
                tint(color, 0.45),
            );
        }
        BossKind::SiegeEngine => {
            for side in [-1., 1.] {
                let track = p + direction(aim + FRAC_PI_2) * r * 0.62 * side;
                plate(emit, track, vec2(r * 1.40, r * 0.36), aim, OUTLINE);
                plate(
                    emit,
                    track,
                    vec2(r * 1.28, r * 0.24),
                    aim,
                    tint(color, 0.65),
                );
                for n in -2..=2 {
                    plate(
                        emit,
                        track + direction(aim) * n as f32 * r * 0.24,
                        vec2(edge * 0.8, r * 0.22),
                        aim,
                        OUTLINE,
                    );
                }
            }
            mount(emit, aim, 0.55, 1.25, 1.);
            hull(emit, p, 4, r * 0.81, aim + FRAC_PI_4, color, edge);
            plate(
                emit,
                p - direction(aim) * r * 0.20,
                vec2(r * 0.4, r * 0.55),
                aim,
                tint(color, 0.72),
            );
        }
        BossKind::Skirmisher => {
            for side in [-1., 1.] {
                let mount_at = p + direction(aim + side * 1.1) * r * 0.45;
                cannon_profile(
                    emit,
                    mount_at,
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
            poly(
                emit,
                p - direction(aim) * r * 0.08,
                3,
                r * 0.52,
                aim,
                tint(color, 0.88),
            );
            plate(
                emit,
                p - direction(aim) * r * 0.12,
                vec2(r * 0.3, r * 0.17),
                aim,
                tint(color, 0.58),
            );
        }
        BossKind::Trapper => {
            for side in [-1., 1.] {
                mount(emit, aim + side * 0.55, 0.40, 0.83, 1.4);
            }
            hull(emit, p, 6, r, aim, color, edge);
            hull(emit, p, 6, r * 0.3, aim, tint(color, 0.82), edge * 0.5);
            for side in [-1., 1.] {
                let cassette = p + direction(aim + side * FRAC_PI_2) * r * 0.55;
                hull(
                    emit,
                    cassette,
                    3,
                    r * 0.23,
                    aim + side * FRAC_PI_2,
                    tint(color, 0.6),
                    edge * 0.6,
                );
            }
        }
    }
}

/// Axiom: six articulated cannon petals around an armored geometric astrolabe.
pub fn axiom_geometry(p: Vec2, r: f32, aim: f32, time: f32, emit: &mut impl FnMut(Primitive)) {
    let gold = Color::new(1., 0.79, 0.34, 1.);
    let teal = Color::new(0.20, 0.80, 0.87, 1.);
    let armor = Color::new(0.23, 0.31, 0.39, 1.);
    let edge = (r * 0.035).max(0.7);
    let phase = time * 0.17;
    for i in 0..6 {
        let a = aim + i as f32 * TAU / 6.;
        let d = vec2(a.cos(), a.sin());
        cannon_profile(
            emit,
            p + d * r * 0.47,
            a,
            r * 0.82,
            r * 0.20,
            armor,
            0.,
            1.25,
            Some(edge),
        );
        hull(
            emit,
            p + d * r * 0.91,
            4,
            r * 0.16,
            a + FRAC_PI_4,
            gold,
            edge * 0.6,
        );
    }
    hull(emit, p, 12, r * 0.94, phase, armor, edge);
    hull(emit, p, 6, r * 0.78, -phase, gold, edge);
    hull(emit, p, 6, r * 0.66, -phase, armor, edge * 0.6);
    for i in 0..6 {
        let a = phase + i as f32 * TAU / 6.;
        hull(
            emit,
            p + vec2(a.cos(), a.sin()) * r * 0.84,
            4,
            r * 0.08,
            a + FRAC_PI_4,
            teal,
            edge * 0.5,
        );
    }
    hull(emit, p, 6, r * 0.47, phase, teal, edge);
    hull(emit, p, 6, r * 0.34, -phase, armor, edge * 0.7);
    hull(emit, p, 4, r * 0.23, phase + FRAC_PI_4, gold, edge * 0.6);
    poly(
        emit,
        p,
        4,
        r * 0.12,
        phase + FRAC_PI_4,
        Color::new(1., 0.96, 0.78, 1.),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn all_boss_silhouettes_are_finite_bounded_and_distinct() {
        let mut signatures = std::collections::HashSet::new();
        for kind in BossKind::ALL {
            let mut signature = String::new();
            boss_model_geometry(
                kind,
                Vec2::ZERO,
                kind.radius(),
                0.4,
                2.,
                2,
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
