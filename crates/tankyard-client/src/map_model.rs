//! Map furniture shares the collider's silhouette. Detail stays inside that
//! silhouette and simplifies at small sizes, without textures or allocations.
use crate::tank_model::Primitive;
use macroquad::prelude::{Color, Rect, Vec2, vec2};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WallStyle {
    Slate,
    Garden,
    Circuit,
    Bastion,
    Relay,
    Orrery,
    Sand,
}
impl WallStyle {
    pub fn for_map(id: &str) -> Self {
        match id {
            "dense" => Self::Garden,
            "maze" => Self::Circuit,
            "fortress" => Self::Bastion,
            "domination" => Self::Relay,
            "boss" => Self::Orrery,
            "survival" => Self::Sand,
            _ => Self::Slate,
        }
    }
}
fn color(rgb: [f32; 3]) -> Color {
    Color::new(rgb[0], rgb[1], rgb[2], 1.)
}
fn mix(a: Color, b: Color, t: f32) -> Color {
    Color::new(
        a.r + (b.r - a.r) * t,
        a.g + (b.g - a.g) * t,
        a.b + (b.b - a.b) * t,
        1.,
    )
}
fn quad(r: Rect, c: Color, emit: &mut impl FnMut(Primitive)) {
    emit(Primitive::Quad {
        vertices: [
            vec2(r.x, r.y),
            vec2(r.right(), r.y),
            vec2(r.right(), r.bottom()),
            vec2(r.x, r.bottom()),
        ],
        color: c,
    });
}
fn inset(r: Rect, amount: f32) -> Rect {
    let n = amount.min(r.w.min(r.h) * 0.45);
    Rect::new(r.x + n, r.y + n, r.w - 2. * n, r.h - 2. * n)
}
fn bevel(r: Rect, cut: f32, c: Color, emit: &mut impl FnMut(Primitive)) {
    let cut = cut.min(r.w.min(r.h) * 0.22);
    let points = [
        vec2(r.x + cut, r.y),
        vec2(r.right() - cut, r.y),
        vec2(r.right(), r.y + cut),
        vec2(r.right(), r.bottom() - cut),
        vec2(r.right() - cut, r.bottom()),
        vec2(r.x + cut, r.bottom()),
        vec2(r.x, r.bottom() - cut),
        vec2(r.x, r.y + cut),
    ];
    for i in 0..8 {
        emit(Primitive::Triangle {
            vertices: [r.center(), points[i], points[(i + 1) % 8]],
            color: c,
        });
    }
}
fn polygon(
    center: Vec2,
    radius: f32,
    sides: u8,
    angle: f32,
    c: Color,
    emit: &mut impl FnMut(Primitive),
) {
    emit(Primitive::Polygon {
        center,
        radius,
        sides,
        angle,
        color: c,
    });
}

pub fn wall_geometry(
    r: Rect,
    style: WallStyle,
    dark: bool,
    quality: u8,
    emit: &mut impl FnMut(Primitive),
) {
    if r.w <= 0. || r.h <= 0. {
        return;
    }
    let accent = color(match style {
        WallStyle::Garden => [0.38, 0.58, 0.40],
        WallStyle::Circuit => [0.57, 0.52, 0.76],
        WallStyle::Bastion | WallStyle::Sand => [0.70, 0.58, 0.36],
        WallStyle::Relay => [0.30, 0.64, 0.63],
        WallStyle::Orrery => [0.63, 0.48, 0.72],
        WallStyle::Slate => [0.43, 0.58, 0.67],
    });
    let stone = if dark {
        color([0.22, 0.27, 0.32])
    } else {
        color([0.72, 0.75, 0.76])
    };
    let body = mix(
        stone,
        accent,
        if style == WallStyle::Garden {
            0.24
        } else {
            0.12
        },
    );
    let rim = mix(body, color([0.08, 0.12, 0.15]), 0.52);
    let light = mix(
        body,
        color([0.94, 0.96, 0.95]),
        if dark { 0.15 } else { 0.42 },
    );
    let short = r.w.min(r.h);
    if short < 7. || quality == 0 {
        quad(r, rim, emit);
        quad(
            inset(r, short * 0.12),
            if style == WallStyle::Garden {
                accent
            } else {
                body
            },
            emit,
        );
        return;
    }
    let edge = (short * 0.08).clamp(1., 4.);
    bevel(r, edge * 2., rim, emit);
    let face = inset(r, edge);
    bevel(face, edge * 1.4, body, emit);
    let inner = inset(face, edge * 1.5);
    quad(
        Rect::new(inner.x, inner.y, inner.w, edge * 0.65),
        light,
        emit,
    );
    quad(
        Rect::new(inner.x, inner.y, edge * 0.65, inner.h),
        light,
        emit,
    );
    if style == WallStyle::Garden {
        let soil = if dark {
            color([0.12, 0.18, 0.14])
        } else {
            color([0.38, 0.43, 0.32])
        };
        bevel(inner, edge * 1.2, soil, emit);
        let bed = inset(inner, edge);
        let horizontal = bed.w >= bed.h;
        let count = ((bed.w.max(bed.h) / bed.w.min(bed.h).max(1.)).ceil() as usize).clamp(1, 6);
        for index in 0..count {
            let t = (index as f32 + 0.5) / count as f32;
            let center = if horizontal {
                vec2(bed.x + bed.w * t, bed.center().y)
            } else {
                vec2(bed.center().x, bed.y + bed.h * t)
            };
            let radius = (bed.w.min(bed.h) * 0.42).min(bed.w.max(bed.h) / count as f32 * 0.4);
            let green = mix(
                accent,
                color([0.18, 0.35, 0.24]),
                0.18 + (index % 3) as f32 * 0.09,
            );
            polygon(center, radius, 8, 0., mix(green, soil, 0.4), emit);
            polygon(
                center + vec2(-radius * 0.12, -radius * 0.15),
                radius * 0.82,
                7,
                0.2,
                green,
                emit,
            );
            if quality > 1 && radius > 5. {
                polygon(
                    center + vec2(-radius * 0.24, -radius * 0.26),
                    radius * 0.40,
                    6,
                    0.4,
                    mix(green, light, 0.36),
                    emit,
                );
                if index % 3 == 1 {
                    polygon(
                        center + vec2(radius * 0.15, -radius * 0.10),
                        radius * 0.15,
                        5,
                        0.,
                        color([0.82, 0.69, 0.41]),
                        emit,
                    );
                }
            }
        }
        return;
    }
    let horizontal = inner.w >= inner.h;
    let long = inner.w.max(inner.h);
    let short = inner.w.min(inner.h);
    if matches!(
        style,
        WallStyle::Bastion | WallStyle::Sand | WallStyle::Slate
    ) {
        // Recessed armor courses give long cover structure without noisy tiles.
        let count = (long / short.max(22.)).floor().clamp(1., 8.) as usize;
        for index in 1..count {
            let at = index as f32 / count as f32;
            let line = if horizontal {
                Rect::new(
                    inner.x + inner.w * at,
                    inner.y + edge,
                    edge * 0.55,
                    (inner.h - edge * 2.).max(0.),
                )
            } else {
                Rect::new(
                    inner.x + edge,
                    inner.y + inner.h * at,
                    (inner.w - edge * 2.).max(0.),
                    edge * 0.55,
                )
            };
            quad(line, mix(body, rim, 0.40), emit);
        }
    } else {
        let track = if horizontal {
            Rect::new(
                inner.x + short * 0.25,
                inner.center().y - edge * 0.6,
                (inner.w - short * 0.5).max(0.),
                edge * 1.2,
            )
        } else {
            Rect::new(
                inner.center().x - edge * 0.6,
                inner.y + short * 0.25,
                edge * 1.2,
                (inner.h - short * 0.5).max(0.),
            )
        };
        quad(track, mix(body, rim, 0.5), emit);
        for t in [0.16, 0.84] {
            let center = if horizontal {
                vec2(inner.x + inner.w * t, inner.center().y)
            } else {
                vec2(inner.center().x, inner.y + inner.h * t)
            };
            polygon(
                center,
                (short * 0.17).min(5.),
                4,
                std::f32::consts::FRAC_PI_4,
                accent,
                emit,
            );
        }
    }
    if quality > 1 && short > 14. {
        let radius = (short * 0.06).clamp(1., 2.5);
        let pad = radius * 2.;
        for center in [
            vec2(inner.x + pad, inner.y + pad),
            vec2(inner.right() - pad, inner.bottom() - pad),
        ] {
            polygon(center, radius, 8, 0., rim, emit);
            polygon(
                center + vec2(-radius * 0.15, -radius * 0.15),
                radius * 0.55,
                6,
                0.,
                light,
                emit,
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn furniture_detail_stays_inside_its_collider_and_has_a_bounded_cost() {
        for map in [
            "dense",
            "maze",
            "fortress",
            "domination",
            "competitive",
            "boss",
            "survival",
        ] {
            for (w, h) in [
                (2., 100.),
                (100., 2.),
                (12., 12.),
                (90., 70.),
                (36., 1200.),
                (1200., 36.),
                (400., 300.),
            ] {
                for quality in 0..=2 {
                    let r = Rect::new(17., 29., w, h);
                    let mut count = 0;
                    let check = |p: Vec2| {
                        assert!(
                            p.x >= r.x - 0.001
                                && p.x <= r.right() + 0.001
                                && p.y >= r.y - 0.001
                                && p.y <= r.bottom() + 0.001,
                            "{map} {w}x{h}: {p:?}"
                        )
                    };
                    wall_geometry(r, WallStyle::for_map(map), false, quality, &mut |p| {
                        count += 1;
                        match p {
                            Primitive::Polygon { center, radius, .. } => {
                                check(center - Vec2::splat(radius));
                                check(center + Vec2::splat(radius));
                            }
                            Primitive::Triangle { vertices, .. } => {
                                vertices.into_iter().for_each(check)
                            }
                            Primitive::Quad { vertices, .. } => {
                                vertices.into_iter().for_each(check)
                            }
                        }
                    });
                    assert!(count <= 70);
                }
            }
        }
    }
}
