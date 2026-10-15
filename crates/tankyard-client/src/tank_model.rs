//! Tank vector geometry shared by the native renderer and the model sheet tool.
//! Emitting primitives needs no window, GPU context or per-tank allocation.
use macroquad::prelude::{Color, Rect, Vec2, WHITE, vec2};
use tankyard_content::{Barrel, BarrelProfile, BodyStyle, ClassDef, FirePattern, ProjectileKind};
use tankyard_core::V2;
pub const OUTLINE: Color = Color::new(0.333, 0.333, 0.333, 1.0);
#[derive(Clone, Copy, Debug)]
pub enum Primitive {
    Polygon {
        center: Vec2,
        sides: u8,
        radius: f32,
        angle: f32,
        color: Color,
    },
    Triangle {
        vertices: [Vec2; 3],
        color: Color,
    },
    Quad {
        vertices: [Vec2; 4],
        color: Color,
    },
}
/// Fit the entire silhouette, including long rifles, into a UI preview.
pub fn preview_placement(class: &ClassDef, rect: Rect, max_radius: f32, aim: f32) -> (Vec2, f32) {
    let radius = 22.0 * class.multipliers[0].sqrt().clamp(0.85, 1.6) * class.body_scale;
    let mut lo = Vec2::splat(f32::INFINITY);
    let mut hi = Vec2::splat(f32::NEG_INFINITY);
    let mut include = |point: Vec2| {
        lo = lo.min(point);
        hi = hi.max(point);
    };
    class_model_geometry(
        class,
        Vec2::ZERO,
        radius,
        aim,
        0.0,
        0.0,
        WHITE,
        0.0,
        None,
        None,
        Some(1.0),
        &mut |primitive| match primitive {
            Primitive::Polygon { center, radius, .. } => {
                include(center - Vec2::splat(radius));
                include(center + Vec2::splat(radius));
            }
            Primitive::Triangle { vertices, .. } => vertices.into_iter().for_each(&mut include),
            Primitive::Quad { vertices, .. } => vertices.into_iter().for_each(&mut include),
        },
    );
    let size = hi - lo;
    let scale = (max_radius / radius)
        .min(rect.w / size.x)
        .min(rect.h / size.y)
        .max(0.0);
    (rect.center() - (lo + hi) * (0.5 * scale), radius * scale)
}
fn alpha(color: Color, opacity: f32) -> Color {
    Color {
        a: opacity,
        ..color
    }
}
fn polygon(
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
fn triangle(emit: &mut impl FnMut(Primitive), a: Vec2, b: Vec2, c: Vec2, color: Color) {
    emit(Primitive::Triangle {
        vertices: [a, b, c],
        color,
    });
}
fn quad(emit: &mut impl FnMut(Primitive), vertices: [Vec2; 4], color: Color) {
    emit(Primitive::Quad { vertices, color });
}
// Draw barrel roots behind the hull, with a flat fill and dark border.
#[allow(clippy::too_many_arguments)]
fn cannon(
    emit: &mut impl FnMut(Primitive),
    p: Vec2,
    angle: f32,
    length: f32,
    width: f32,
    color: Color,
    recoil: f32,
) {
    cannon_profile(emit, p, angle, length, width, color, recoil, 1.0, None);
}
#[allow(clippy::too_many_arguments)]
pub fn cannon_profile(
    emit: &mut impl FnMut(Primitive),
    p: Vec2,
    angle: f32,
    length: f32,
    width: f32,
    color: Color,
    recoil: f32,
    flare: f32,
    border: Option<f32>,
) {
    let d = vec2(angle.cos(), angle.sin());
    let n = vec2(-d.y, d.x);
    let base = p - d * recoil;
    let end = base + d * length;
    let mut quad = |half: f32, inset: f32, tint: Color| {
        let a = base + d * inset + n * half;
        let b = base + d * inset - n * half;
        let c = end - d * inset - n * (half * flare);
        let e = end - d * inset + n * (half * flare);
        quad(emit, [a, b, c, e], tint);
    };
    // Scale the border to the actual width; very close parallel mounts can
    // be less than 4 world units wide and must not invert their inner quad.
    let edge = border
        .unwrap_or_else(|| (width * 0.14).clamp(0.5, 4.25))
        .min(width * 0.28);
    quad(width * 0.5, 0.0, alpha(OUTLINE, color.a));
    quad(
        (width * 0.5 - edge).max(width * 0.10),
        edge,
        Color::new(0.60, 0.60, 0.60, color.a),
    );
}

pub fn minion_model_geometry(
    p: Vec2,
    radius: f32,
    aim: f32,
    recoil: f32,
    color: Color,
    border: f32,
    emit: &mut impl FnMut(Primitive),
) {
    cannon_profile(
        emit,
        p,
        aim,
        radius * tankyard_core::MinionGun::BARREL_LENGTH,
        radius * 0.65,
        color,
        recoil,
        1.0,
        Some(border.min(radius * 0.18)),
    );
    polygon(emit, p, 0, radius + border, 0.0, alpha(OUTLINE, color.a));
    polygon(emit, p, 0, radius, 0.0, color);
}
fn barrel_flare(barrel: &Barrel) -> f32 {
    match barrel.profile {
        BarrelProfile::Rectangle => return 1.0,
        BarrelProfile::Trapezoid => return 1.5,
        BarrelProfile::Trap => return 1.5,
        BarrelProfile::Rocket => return 1.6,
        BarrelProfile::Auto => {}
    }
    match barrel.projectile {
        ProjectileKind::Trap | ProjectileKind::Mine => 1.5,
        ProjectileKind::Drone | ProjectileKind::Minion => 1.6,
        ProjectileKind::Scatter => 1.18,
        ProjectileKind::Bullet if barrel.width >= 19.0 => 1.18,
        _ => 1.0,
    }
}

fn same_mount(a: &Barrel, b: &Barrel) -> bool {
    a.pattern != FirePattern::Auto
        && b.pattern != FirePattern::Auto
        && (a.pattern == FirePattern::Spin) == (b.pattern == FirePattern::Spin)
        && a.offset == b.offset
        && a.angle == b.angle
        && a.length == b.length
        && a.width == b.width
        && a.profile == b.profile
}
fn representative(class: &ClassDef, index: usize) -> usize {
    (0..index)
        .find(|&other| same_mount(&class.barrels[index], &class.barrels[other]))
        .unwrap_or(index)
}
fn visual_mount_count(class: &ClassDef) -> usize {
    (0..class.barrels.len())
        .filter(|&i| representative(class, i) == i)
        .count()
}

fn stacked_barrel_border(class: &ClassDef, index: usize, unit: f32) -> Option<f32> {
    let barrel = &class.barrels[index];
    let normal = V2::new(-barrel.angle.sin(), barrel.angle.cos());
    class
        .barrels
        .iter()
        .enumerate()
        .filter_map(|(i, other)| {
            if i == index
                || other.pattern == FirePattern::Auto
                || (barrel.pattern == FirePattern::Spin) != (other.pattern == FirePattern::Spin)
            {
                return None;
            }
            let angle = (other.angle - barrel.angle + std::f32::consts::PI)
                .rem_euclid(std::f32::consts::TAU)
                - std::f32::consts::PI;
            let offset = V2::new(
                other.offset[0] - barrel.offset[0],
                other.offset[1] - barrel.offset[1],
            );
            let step = (other.width - barrel.width).abs();
            (angle.abs() < 0.01 && offset.dot(normal).abs() < 2.0 && step > 0.1)
                .then_some(step * 1.16 * 0.3 * unit)
        })
        .min_by(f32::total_cmp)
}

fn visual_barrel_width(
    class: &ClassDef,
    index: usize,
    angles: &[f32],
    hull_aim: f32,
    count: usize,
) -> f32 {
    let barrel: &Barrel = &class.barrels[index];
    let base_radius = 22.0 * class.multipliers[0].sqrt().clamp(0.85, 1.6);
    // A small turret has its own proportions, independent of main cannon count.
    if barrel.pattern == FirePattern::Auto {
        return (barrel.width * 1.1).clamp(0.75, base_radius * 0.7);
    }
    // Keep heavy guns broad and rifles narrow. Parallel mounts are limited
    // by their spacing below so their outlines stay separate.
    let mut width = if barrel.projectile.is_drone() {
        barrel.width * 1.12
    } else if count == 1 {
        if barrel.width <= 11.0 {
            (barrel.width * 1.25).max(base_radius * 0.66)
        } else if barrel.width >= 18.0 {
            (barrel.width * 1.20).max(base_radius * 1.05)
        } else {
            (barrel.width * 1.35).max(base_radius * 0.83)
        }
    } else {
        barrel.width * 1.16
    };
    let axis = V2::from_angle(angles[index]);
    let perpendicular = V2::new(-axis.y, axis.x);
    let offset = V2::new(barrel.offset[0], barrel.offset[1]).rotate(hull_aim);
    for (other_index, other) in class.barrels.iter().enumerate() {
        if index == other_index || other.pattern == FirePattern::Auto {
            continue;
        }
        let angle_delta = (angles[other_index] - angles[index] + std::f32::consts::PI)
            .rem_euclid(std::f32::consts::TAU)
            - std::f32::consts::PI;
        if angle_delta.abs() > 0.09 {
            continue;
        }
        let other_offset = V2::new(other.offset[0], other.offset[1]).rotate(hull_aim);
        let across = (other_offset - offset).dot(perpendicular).abs();
        // Treat near-zero lateral offsets as coaxial stacked mounts. We draw
        // the larger behind the smaller, so this is an intentional overlap.
        if across >= 2.0 {
            // Two barrels with different original widths each get a fair share
            // of the actual free distance. Prevent all visual intersections.
            let envelope = barrel.width * barrel_flare(barrel).max(1.0);
            let other_envelope = other.width * barrel_flare(other).max(1.0);
            let share = envelope / (envelope + other_envelope).max(0.01);
            width = width.min(across * share * 1.90 / barrel_flare(barrel).max(1.0));
        }
    }
    width
        .max(0.75)
        .min(base_radius * if barrel.width >= 32.0 { 1.92 } else { 1.48 })
}

#[allow(clippy::too_many_arguments)]
fn trap_cannon(
    emit: &mut impl FnMut(Primitive),
    p: Vec2,
    angle: f32,
    length: f32,
    width: f32,
    color: Color,
    recoil: f32,
) {
    let d = vec2(angle.cos(), angle.sin());
    let n = vec2(-d.y, d.x);
    let root = p - d * recoil;
    let neck = length * 0.68;
    let edge = (width * 0.14).clamp(0.5, 4.25).min(width * 0.28);
    let mut section = |from: f32, to: f32, start_half: f32, end_half: f32, tint: Color| {
        let a = root + d * from + n * start_half;
        let b = root + d * from - n * start_half;
        let c = root + d * to - n * end_half;
        let e = root + d * to + n * end_half;
        quad(emit, [a, b, c, e], tint);
    };
    let half = width * 0.5;
    let border = alpha(OUTLINE, color.a);
    let fill = Color::new(0.60, 0.60, 0.60, color.a);
    section(0.0, neck, half, half, border);
    section(neck, length, half, half * 1.5, border);
    section(edge, neck + edge, half - edge, half - edge, fill);
    section(neck, length - edge, half - edge, half * 1.5 - edge, fill);
}

#[allow(clippy::too_many_arguments)]
pub fn class_model_geometry(
    class: &ClassDef,
    p: Vec2,
    r: f32,
    aim: f32,
    recoil: f32,
    flash: f32,
    color: Color,
    time: f32,
    barrel_recoils: Option<&[f32]>,
    barrel_aims: Option<&[f32]>,
    world_zoom: Option<f32>,
    emit: &mut impl FnMut(Primitive),
) {
    let base_radius = 22.0 * class.multipliers[0].sqrt().clamp(0.85, 1.6) * class.body_scale;
    // Use the same mount and muzzle positions as the simulation, scaled for
    // either the camera or a preview card.
    let unit = world_zoom.unwrap_or(r / base_radius);
    // Longer, narrow tubes go behind the wider sleeves. Only their exposed
    // ends remain visible, giving coaxial cannons a clean stepped silhouette.
    let mut order = [0_usize; 16];
    let count = class.barrels.len().min(16);
    let mount_count = visual_mount_count(class);
    for (slot, index) in order.iter_mut().take(count).enumerate() {
        *index = slot;
    }
    order[..count].sort_unstable_by(|&a, &b| {
        class.barrels[a]
            .width
            .total_cmp(&class.barrels[b].width)
            .then_with(|| class.barrels[b].length.total_cmp(&class.barrels[a].length))
    });
    let mut angles = [0.0_f32; 16];
    for (index, barrel) in class.barrels.iter().take(count).enumerate() {
        angles[index] = if barrel.pattern == FirePattern::Auto {
            barrel_aims
                .and_then(|a| a.get(index))
                .copied()
                .unwrap_or(aim + barrel.angle)
        } else {
            aim + barrel.angle
                + if barrel.pattern == FirePattern::Spin {
                    time * 1.7
                } else {
                    0.0
                }
        };
    }
    for &index in &order[..count] {
        let barrel = &class.barrels[index];
        if barrel.pattern == FirePattern::Auto || representative(class, index) != index {
            continue;
        }

        let mut recoil = barrel_recoils
            .and_then(|recoils| recoils.get(index))
            .copied()
            .unwrap_or(recoil);
        if let Some(recoils) = barrel_recoils {
            for (other, &value) in recoils.iter().enumerate().take(class.barrels.len()) {
                if same_mount(barrel, &class.barrels[other]) {
                    recoil = recoil.max(value);
                }
            }
        }
        let angle = angles[index];
        // Only the barrel's *direction* spins. The authoritative simulator
        // mounts all origins in hull-local coordinates rotated by tank aim.
        let offset = V2::new(barrel.offset[0], barrel.offset[1]).rotate(aim);
        let point = p + vec2(offset.x, offset.y) * unit;
        // Apply camera scale after constraining width to the mount spacing.
        let width = visual_barrel_width(class, index, &angles, aim, mount_count) * unit;
        let length = (barrel.length + r / unit * 0.45) * unit;
        if barrel.profile == BarrelProfile::Rocket {
            cannon_profile(
                emit,
                point,
                angle,
                length * 0.73,
                width * 1.10,
                color,
                recoil * 60.0 * unit,
                1.45,
                None,
            );
            cannon_profile(
                emit,
                point,
                angle,
                length,
                width * 0.86,
                color,
                recoil * 60.0 * unit,
                1.0,
                None,
            );
        } else if barrel.profile == BarrelProfile::Trap
            || (barrel.profile == BarrelProfile::Auto
                && matches!(
                    barrel.projectile,
                    ProjectileKind::Trap | ProjectileKind::Mine
                ))
        {
            trap_cannon(
                emit,
                point,
                angle,
                length,
                width,
                color,
                recoil * 60.0 * unit,
            );
        } else {
            let border = stacked_barrel_border(class, index, unit);
            cannon_profile(
                emit,
                point,
                angle,
                length,
                width,
                color,
                recoil * 60.0 * unit,
                if border.is_some()
                    && barrel.profile == BarrelProfile::Auto
                    && barrel.projectile != ProjectileKind::Scatter
                {
                    1.0
                } else {
                    barrel_flare(barrel)
                },
                border,
            );
        }
        // Recoil is expressed entirely by the barrel moving back.
    }
    // The hull covers the barrel roots.
    let smasher = matches!(class.body_style, BodyStyle::Smasher | BodyStyle::Landmine)
        || (class.body_style == BodyStyle::Auto
            && (class.barrels.is_empty() || class.id == "auto_smasher"));
    if smasher {
        polygon(
            emit,
            p,
            6,
            r * 1.16 + 3.0 * unit,
            time * 0.12,
            alpha(OUTLINE, color.a),
        );
        polygon(
            emit,
            p,
            6,
            r * 1.16,
            time * 0.12,
            Color::new(0.60, 0.60, 0.60, color.a),
        );
        if class.body_style == BodyStyle::Landmine
            || (class.body_style == BodyStyle::Auto && class.id == "silt")
        {
            polygon(
                emit,
                p,
                6,
                r * 1.16 + 3.0 * unit,
                -time * 0.06 + std::f32::consts::FRAC_PI_6,
                alpha(OUTLINE, color.a),
            );
            polygon(
                emit,
                p,
                6,
                r * 1.16,
                -time * 0.06 + std::f32::consts::FRAC_PI_6,
                Color::new(0.60, 0.60, 0.60, color.a),
            );
        }
    }
    let spike = class.body_style == BodyStyle::Spike
        || (class.body_style == BodyStyle::Auto && class.id == "pike");
    if spike {
        for tooth in 0..12 {
            let angle = aim + tooth as f32 * std::f32::consts::TAU / 12.0;
            let d = vec2(angle.cos(), angle.sin());
            let n = vec2(-d.y, d.x);
            triangle(
                emit,
                p + d * (r * 1.32 + 3.0 * unit),
                p + d * (r * 0.84) + n * r * 0.26,
                p + d * (r * 0.84) - n * r * 0.26,
                alpha(OUTLINE, color.a),
            );
            triangle(
                emit,
                p + d * (r * 1.26),
                p + d * (r * 0.86) + n * r * 0.19,
                p + d * (r * 0.86) - n * r * 0.19,
                Color::new(0.60, 0.60, 0.60, color.a),
            );
        }
    }
    let sides = if spike || smasher || class.body_style == BodyStyle::Circle {
        0
    } else {
        class.hull_sides
    };
    let body_angle = if sides == 3 {
        aim
    } else if sides == 4 {
        aim + std::f32::consts::FRAC_PI_4
    } else {
        time * 0.06
    };
    polygon(
        emit,
        p,
        sides,
        r + 3.2 * unit,
        body_angle,
        alpha(OUTLINE, color.a),
    );
    polygon(emit, p, sides, r, body_angle, color);
    // Brief, unobtrusive damage flash that never hides the tank silhouette.
    if flash > 0.0 {
        polygon(
            emit,
            p,
            sides,
            r,
            body_angle,
            alpha(WHITE, (flash / 0.12 * 0.42).clamp(0.0, 0.42)),
        );
    }
    // Automatic mounts sit above the hull and rotate independently.
    for &index in &order[..count] {
        let barrel = &class.barrels[index];
        if barrel.pattern != FirePattern::Auto {
            continue;
        }
        let offset = V2::new(barrel.offset[0], barrel.offset[1]).rotate(aim);
        let at = p + vec2(offset.x, offset.y) * unit;
        let recoil = barrel_recoils
            .and_then(|values| values.get(index))
            .copied()
            .unwrap_or(recoil);
        cannon(
            emit,
            at,
            angles[index],
            (barrel.length + r / unit * 0.45) * unit,
            visual_barrel_width(class, index, &angles, aim, mount_count) * unit,
            color,
            recoil * 60.0 * unit,
        );
        let turret_radius = (base_radius * 0.34).clamp(6.5, 12.0) * unit;
        polygon(
            emit,
            at,
            0,
            turret_radius + 2.5 * unit,
            0.0,
            alpha(OUTLINE, color.a),
        );
        polygon(
            emit,
            at,
            0,
            turret_radius,
            0.0,
            Color::new(0.65, 0.65, 0.65, color.a),
        );
    }
}
#[cfg(test)]
mod geometry_tests {
    use super::*;
    use tankyard_content::Content;

    #[test]
    fn catalog_mounts_mirror_across_the_hull_and_opposing_ports_share_an_axis() {
        for class in Content::builtin().classes {
            for barrel in &class.barrels {
                assert!(
                    class.barrels.iter().any(|other| {
                        (barrel.offset[0] - other.offset[0]).abs() < 0.001
                            && (barrel.offset[1] + other.offset[1]).abs() < 0.001
                            && (barrel.angle.cos() - other.angle.cos()).abs() < 0.001
                            && (barrel.angle.sin() + other.angle.sin()).abs() < 0.001
                            && (barrel.length - other.length).abs() < 0.001
                            && (barrel.width - other.width).abs() < 0.001
                            && barrel.profile == other.profile
                    }),
                    "{} has an unmatched offset or angled mount: {:?}",
                    class.id,
                    barrel.offset
                );
            }
        }
    }

    #[test]
    fn hull_rotation_keeps_barrel_proportions_stable() {
        let catalog = Content::builtin();
        for class in &catalog.classes {
            for spin in [0.0, 0.7] {
                let directions: Vec<_> = class
                    .barrels
                    .iter()
                    .map(|b| {
                        b.angle
                            + if b.pattern == FirePattern::Spin {
                                spin
                            } else {
                                0.0
                            }
                    })
                    .collect();
                for aim in [-3.0, -2.3, -1.57, -0.8, 0.0, 0.6, 1.57, 2.5, 3.1] {
                    let rotated: Vec<_> = directions.iter().map(|angle| angle + aim).collect();
                    for index in 0..class.barrels.len() {
                        let count = visual_mount_count(class);
                        let base = visual_barrel_width(class, index, &directions, 0.0, count);
                        let width = visual_barrel_width(class, index, &rotated, aim, count);
                        assert!(
                            (base - width).abs() < 0.001,
                            "{} barrel {index} changes width at aim {aim}: {base} -> {width}",
                            class.id
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn auto_smasher_has_a_round_hull_hexagonal_base_and_small_turret() {
        let content = Content::builtin();
        let class = content.class("auto_smasher").unwrap();
        let radius = 22.0 * class.multipliers[0].sqrt();
        let color = Color::new(0.0, 0.698, 0.882, 1.0);
        let mut geometry = Vec::new();
        class_model_geometry(
            class,
            Vec2::ZERO,
            radius,
            0.0,
            0.0,
            0.0,
            color,
            0.0,
            None,
            None,
            Some(1.0),
            &mut |p| geometry.push(p),
        );
        let body = geometry.iter().position(|p| matches!(p, Primitive::Polygon { sides: 0, color: c, radius: r, .. } if *c == color && (*r-radius).abs()<0.001)).unwrap();
        assert!(
            geometry[..body]
                .iter()
                .any(|p| matches!(p, Primitive::Polygon { sides: 6, radius: r, .. } if *r>radius))
        );
        assert!(
            geometry[body + 1..].iter().any(
                |p| matches!(p, Primitive::Polygon { sides: 0, radius: r, .. } if *r<radius*0.5)
            )
        );
        let width = visual_barrel_width(class, 0, &[0.0], 0.0, visual_mount_count(class));
        assert!(width < radius * 0.5);
        assert!(class.barrels[0].length + radius * 0.45 < radius);
    }

    #[test]
    fn catalog_geometry_stays_finite_during_rotation_recoil_and_zoom() {
        for class in &Content::builtin().classes {
            for aim in [-2.6, -1.57, -0.45, 0.0, 0.8, 1.57, 2.8] {
                for zoom in [0.3, 1.0, 2.0] {
                    let radius = 22.0 * class.multipliers[0].sqrt().clamp(0.85, 1.6) * zoom;
                    let aims: Vec<_> = class.barrels.iter().map(|_| aim + 0.4).collect();
                    class_model_geometry(
                        class,
                        Vec2::ZERO,
                        radius,
                        aim,
                        0.16,
                        0.06,
                        Color::new(0.0, 0.698, 0.882, 1.0),
                        1.2,
                        None,
                        Some(&aims),
                        Some(zoom),
                        &mut |p| match p {
                            Primitive::Polygon {
                                center,
                                radius,
                                angle,
                                color,
                                ..
                            } => {
                                assert!(
                                    center.is_finite()
                                        && radius.is_finite()
                                        && radius > 0.0
                                        && angle.is_finite()
                                );
                                assert!(color.a.is_finite());
                            }
                            Primitive::Triangle { vertices, color } => {
                                assert!(vertices.iter().all(|p| p.is_finite()));
                                let area =
                                    (vertices[1] - vertices[0]).perp_dot(vertices[2] - vertices[0]);
                                assert!(
                                    area.abs() > 0.00001,
                                    "{} has collapsed barrel geometry",
                                    class.id
                                );
                                assert!(color.a.is_finite());
                            }
                            Primitive::Quad { vertices, color } => {
                                assert!(vertices.iter().all(|p| p.is_finite()));
                                let first =
                                    (vertices[1] - vertices[0]).perp_dot(vertices[2] - vertices[0]);
                                let second =
                                    (vertices[2] - vertices[0]).perp_dot(vertices[3] - vertices[0]);
                                assert!(
                                    first > 0.00001 && second > 0.00001,
                                    "{} has inverted cannon geometry",
                                    class.id
                                );
                                assert!(color.a.is_finite());
                            }
                        },
                    );
                }
            }
        }
    }

    #[test]
    fn every_class_has_finite_visible_barrels() {
        for class in Content::builtin().classes {
            let angles: Vec<f32> = class.barrels.iter().map(|b| b.angle).collect();
            for i in 0..class.barrels.len() {
                let width =
                    visual_barrel_width(&class, i, &angles, 0.0, visual_mount_count(&class));
                assert!(
                    width.is_finite() && width > 0.0,
                    "{} barrel {}",
                    class.id,
                    i
                );
                assert!(width < 100.0, "{} barrel {}", class.id, i);
            }
        }
    }

    #[test]
    fn parallel_mounts_have_no_unintentional_overlap() {
        let catalog = Content::builtin();
        for class in &catalog.classes {
            let angles: Vec<f32> = class.barrels.iter().map(|b| b.angle).collect();
            for (i, a) in class.barrels.iter().enumerate() {
                for (j, b) in class.barrels.iter().enumerate().skip(i + 1) {
                    if a.pattern == FirePattern::Auto || b.pattern == FirePattern::Auto {
                        continue; // Turrets are intentionally layered over the main guns.
                    }
                    let delta = (a.angle - b.angle + std::f32::consts::PI)
                        .rem_euclid(std::f32::consts::TAU)
                        - std::f32::consts::PI;
                    if delta.abs() > 0.09 {
                        continue;
                    }
                    let normal = V2::new(-a.angle.sin(), a.angle.cos());
                    let across = (V2::new(a.offset[0] - b.offset[0], a.offset[1] - b.offset[1]))
                        .dot(normal)
                        .abs();
                    if across < 2.0 {
                        continue;
                    } // Coaxial guns layer by design.
                    let count = visual_mount_count(class);
                    let half_widths = (visual_barrel_width(class, i, &angles, 0.0, count)
                        * barrel_flare(a).max(1.0)
                        + visual_barrel_width(class, j, &angles, 0.0, count)
                            * barrel_flare(b).max(1.0))
                        * 0.5;
                    assert!(
                        half_widths <= across + 0.01,
                        "{} barrels {} and {} overlap: {half_widths} > {across}",
                        class.id,
                        i,
                        j
                    );
                }
            }
        }
    }

    #[test]
    fn long_barrels_and_automatic_turrets_fit_inside_preview_cards() {
        let rect = Rect::new(30.0, 50.0, 75.0, 48.0);
        for class in &Content::builtin().classes {
            for aim in [-1.57, -0.45, 0.0, 2.2] {
                let (center, radius) = preview_placement(class, rect, 16.0, aim);
                let mut check = |p: Vec2| {
                    assert!(
                        p.x >= rect.x - 0.001
                            && p.x <= rect.x + rect.w + 0.001
                            && p.y >= rect.y - 0.001
                            && p.y <= rect.y + rect.h + 0.001,
                        "{} preview escapes its card: {p:?}",
                        class.id
                    );
                };
                class_model_geometry(
                    class,
                    center,
                    radius,
                    aim,
                    0.0,
                    0.0,
                    WHITE,
                    0.0,
                    None,
                    None,
                    None,
                    &mut |primitive| match primitive {
                        Primitive::Polygon { center, radius, .. } => {
                            check(center - Vec2::splat(radius));
                            check(center + Vec2::splat(radius));
                        }
                        Primitive::Triangle { vertices, .. } => {
                            vertices.into_iter().for_each(&mut check)
                        }
                        Primitive::Quad { vertices, .. } => {
                            vertices.into_iter().for_each(&mut check)
                        }
                    },
                );
            }
        }
    }

    #[test]
    fn shared_muzzles_draw_once_and_use_all_emitter_recoil() {
        let catalog = Content::builtin();
        let mut factory = catalog.class("foundry").unwrap().clone();
        // Exercise shared emitters even when the stock Factory has only one.
        factory.barrels = vec![factory.barrels[0].clone(); 3];
        let render = |recoils: &[f32]| {
            let mut quads = Vec::new();
            class_model_geometry(
                &factory,
                Vec2::ZERO,
                22.0,
                0.0,
                0.0,
                0.0,
                WHITE,
                0.0,
                Some(recoils),
                None,
                None,
                &mut |primitive| {
                    if let Primitive::Quad { vertices, .. } = primitive {
                        quads.push(vertices);
                    }
                },
            );
            quads
        };
        let resting = render(&[0.0; 3]);
        let fired = render(&[0.0, 0.1, 0.0]);
        assert_eq!(resting.len(), 2); // One outer and one inner quad.
        assert_eq!(fired.len(), 2);
        assert!(fired[0][0].x < resting[0][0].x);
    }
}
