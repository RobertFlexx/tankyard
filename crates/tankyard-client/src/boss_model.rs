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

#[allow(clippy::too_many_arguments)]
pub fn boss_model_geometry(
    kind: BossKind,
    p: Vec2,
    r: f32,
    aim: f32,
    time: f32,
    phase: u8,
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
            for n in 0..3 {
                poly(
                    emit,
                    p + direction(aim + PI + (n as f32 - 1.) * 0.45) * r * 0.38,
                    0,
                    r * 0.10,
                    0.,
                    OUTLINE,
                );
            }
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
        _ => {
            let (sides, mounts, angle) = match kind {
                BossKind::Sentinel => (6, 3, aim),
                BossKind::Prism => (3, 3, rotate),
                BossKind::Foundry => (5, 5, -rotate),
                BossKind::Tempest => (8, 4, aim),
                BossKind::Bastion => (4, 4, FRAC_PI_4),
                _ => (7, 7, rotate),
            };
            for n in 0..mounts {
                let a = aim + n as f32 * TAU / mounts as f32;
                mount(
                    emit,
                    a,
                    if kind == BossKind::Foundry {
                        0.43
                    } else {
                        0.28
                    },
                    if kind == BossKind::Sentinel {
                        1.05
                    } else {
                        0.90
                    },
                    if kind == BossKind::Foundry { 1.65 } else { 1. },
                );
            }
            hull(emit, p, sides, r, angle, tint(color, 0.65), edge);
            // Precisely nested armor, separated by a constant dark seam.
            hull(emit, p, sides, r * 0.81, angle, color, edge * 0.65);
            for n in 0..sides {
                let a = angle + (n as f32 + 0.5) * TAU / sides as f32;
                hull(
                    emit,
                    p + direction(a) * r * 0.59,
                    4,
                    r * 0.105,
                    a + FRAC_PI_4,
                    tint(color, 1.12),
                    edge * 0.3,
                );
            }
            let reactor = Color::new(0.90, 0.97, 1., 1.);
            hull(
                emit,
                p,
                if kind == BossKind::Bastion { 4 } else { 0 },
                r * 0.34,
                angle,
                tint(color, 0.46),
                edge * 0.8,
            );
            poly(emit, p, 3 + phase.min(3), r * 0.22, -rotate * 2., reactor);
            poly(emit, p, 0, r * 0.09, 0., color);
        }
    }
    if matches!(
        kind,
        BossKind::Guardian
            | BossKind::Summoner
            | BossKind::Defender
            | BossKind::FallenOverlord
            | BossKind::FallenBooster
    ) {
        // Familiar broad silhouettes, with original reactor assemblies and
        // precisely spaced armor inlays specific to Tank Yard's machines.
        let (segments, angle, reach) = match kind {
            BossKind::Guardian => (3, aim, 0.54),
            BossKind::Summoner => (4, aim + FRAC_PI_4, 0.61),
            BossKind::Defender => (3, rotate, 0.29),
            BossKind::FallenOverlord => (4, aim + FRAC_PI_4, 0.72),
            _ => (5, aim + PI, 0.70),
        };
        for i in 0..segments {
            let a = angle + i as f32 * TAU / segments as f32;
            let pos = p + direction(a) * r * reach;
            hull(
                emit,
                pos,
                4,
                r * 0.065,
                a + FRAC_PI_4,
                tint(color, 0.74),
                edge * 0.25,
            );
        }
        hull(
            emit,
            p,
            if kind == BossKind::Summoner { 4 } else { 3 },
            r * 0.18,
            angle,
            tint(color, 0.58),
            edge * 0.42,
        );
        poly(emit, p, 0, r * 0.075, 0., Color::new(0.91, 0.97, 1., 1.));
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
