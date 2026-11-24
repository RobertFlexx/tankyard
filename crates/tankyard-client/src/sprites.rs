//! Particle sprites: white shapes with smooth edges on one generated sheet,
//! each kept at four sizes so a particle samples the one nearest its own and
//! stays clean from a speck to a puff. Drawn tinted, in whatever blend is
//! active, so the same sheet serves ordinary particles and light.
use macroquad::models::Vertex;
use macroquad::prelude::*;
use std::cell::RefCell;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Sprite {
    /// A round drop with a slightly softer rim.
    Disc,
    /// A square with its corners eased.
    Square,
    /// A triangle pointing up, its corners eased.
    Triangle,
    /// A five-pointed star.
    Star,
    /// A streak along +x, its head bright and its tail thinning out.
    Spark,
    /// A wide, soft glow for under bright things.
    Halo,
    /// A four-pointed flare: a catch of light.
    Glint,
}

impl Sprite {
    const ALL: [Self; 7] = [
        Self::Disc,
        Self::Square,
        Self::Triangle,
        Self::Star,
        Self::Spark,
        Self::Halo,
        Self::Glint,
    ];

    /// Whether the shape has a hard outline, worth sampling several times
    /// a pixel to smooth; the rest are smooth already.
    fn outlined(self) -> bool {
        matches!(
            self,
            Self::Disc | Self::Square | Self::Triangle | Self::Star
        )
    }

    /// How opaque the shape is at `(x, y)`, each from -1 to 1 across its
    /// cell, y pointing down.
    fn alpha(self, x: f32, y: f32) -> f32 {
        let d2 = x * x + y * y;
        match self {
            // A little lighter towards the rim, so a drop reads as round.
            Self::Disc => inside(d2 < 0.86 * 0.86) * (1. - 0.2 * d2),
            Self::Square => {
                let corner = 0.14;
                let qx = (x.abs() - (0.84 - corner)).max(0.);
                let qy = (y.abs() - (0.84 - corner)).max(0.);
                inside(qx * qx + qy * qy < corner * corner)
            }
            Self::Triangle => inside(in_polygon(x, y, &triangle(), 0.04)),
            Self::Star => inside(in_polygon(x, y, &star(), 0.)),
            Self::Spark => {
                // Bright and full at the head (+x), thinning along the tail.
                let along = (x + 1.) * 0.5;
                let width = 0.12 + 0.2 * along;
                let across = (-(y / width).powi(2)).exp();
                let body = smoothstep(0., 0.55, along) * (1. - smoothstep(0.8, 0.94, along));
                across * body * (0.45 + 0.55 * along)
            }
            Self::Halo => {
                let edge = (1. - d2).max(0.);
                edge * edge * (0.2 + 0.8 * (-d2 * 5.).exp())
            }
            Self::Glint => {
                let ray = |along: f32, across: f32| {
                    (-(across / 0.05).powi(2)).exp() * (1. - along.abs()).max(0.).powi(2)
                };
                let cross = ray(x, y).max(ray(y, x));
                let core = (-d2 * 40.).exp();
                (cross + core).min(1.) * (1. - d2).max(0.)
            }
        }
    }
}

fn inside(test: bool) -> f32 {
    if test { 1. } else { 0. }
}

fn smoothstep(edge0: f32, edge1: f32, x: f32) -> f32 {
    let t = ((x - edge0) / (edge1 - edge0)).clamp(0., 1.);
    t * t * (3. - 2. * t)
}

fn triangle() -> [Vec2; 3] {
    std::array::from_fn(|i| {
        let a = (-90. + i as f32 * 120.).to_radians();
        vec2(a.cos(), a.sin()) * 0.92
    })
}

fn star() -> [Vec2; 10] {
    std::array::from_fn(|i| {
        let a = (-90. + i as f32 * 36.).to_radians();
        let reach = if i % 2 == 0 { 0.94 } else { 0.4 };
        vec2(a.cos(), a.sin()) * reach
    })
}

/// Whether `(x, y)` is inside `corners`, drawn `inset` smaller.
fn in_polygon(x: f32, y: f32, corners: &[Vec2], inset: f32) -> bool {
    let p = vec2(x, y) * (1. + inset);
    let mut inside = false;
    let mut j = corners.len() - 1;
    for i in 0..corners.len() {
        let (a, b) = (corners[i], corners[j]);
        if (a.y > p.y) != (b.y > p.y) && p.x < (b.x - a.x) * (p.y - a.y) / (b.y - a.y) + a.x {
            inside = !inside;
        }
        j = i;
    }
    inside
}

/// Each shape at these sizes in pixels, from the sharpest down.
const SIZES: [u16; 4] = [128, 64, 32, 16];
/// Where each size starts across a row, with room between them so smooth
/// sampling never reaches a neighbour.
const COLUMNS: [u16; 4] = [0, 132, 200, 236];
const WIDTH: u16 = 256;
const ROW: u16 = 132;
/// Samples a side within each pixel of an outlined shape.
const SUPERSAMPLE: usize = 4;

/// The sheet's pixels: white, with each shape in its alpha.
fn sheet_pixels() -> Vec<u8> {
    let height = ROW * Sprite::ALL.len() as u16;
    let mut pixels = [255u8, 255, 255, 0].repeat(usize::from(WIDTH) * usize::from(height));
    for (row, sprite) in Sprite::ALL.into_iter().enumerate() {
        for (&size, &column) in SIZES.iter().zip(&COLUMNS) {
            let size = usize::from(size);
            for py in 0..size {
                for px in 0..size {
                    let at =
                        |offset: f32, pixel: usize| (pixel as f32 + offset) / size as f32 * 2. - 1.;
                    let alpha = if sprite.outlined() {
                        let mut sum = 0.;
                        for sy in 0..SUPERSAMPLE {
                            for sx in 0..SUPERSAMPLE {
                                let offset = |s: usize| (s as f32 + 0.5) / SUPERSAMPLE as f32;
                                sum += sprite.alpha(at(offset(sx), px), at(offset(sy), py));
                            }
                        }
                        sum / (SUPERSAMPLE * SUPERSAMPLE) as f32
                    } else {
                        sprite.alpha(at(0.5, px), at(0.5, py))
                    };
                    let x = usize::from(column) + px;
                    let y = row * usize::from(ROW) + py;
                    pixels[(y * usize::from(WIDTH) + x) * 4 + 3] =
                        (alpha.clamp(0., 1.) * 255.).round() as u8;
                }
            }
        }
    }
    pixels
}

thread_local! {
    static SHEET: Texture2D = {
        let texture = Texture2D::from_rgba8(
            WIDTH,
            ROW * Sprite::ALL.len() as u16,
            &sheet_pixels(),
        );
        texture.set_filter(FilterMode::Linear);
        texture
    };
    static RING: RefCell<(Vec<Vertex>, Vec<u16>)> = const { RefCell::new((Vec::new(), Vec::new())) };
}

/// Where `sprite` lies on the sheet, at the size nearest `pixels` across
/// without going under it, as texture coordinates.
fn cell(sprite: Sprite, pixels: f32) -> Rect {
    let index = SIZES
        .iter()
        .rposition(|&size| f32::from(size) >= pixels * 0.9)
        .unwrap_or(0);
    let row = Sprite::ALL.iter().position(|&s| s == sprite).unwrap_or(0) as f32;
    let height = f32::from(ROW) * Sprite::ALL.len() as f32;
    let size = f32::from(SIZES[index]);
    Rect::new(
        f32::from(COLUMNS[index]) / f32::from(WIDTH),
        row * f32::from(ROW) / height,
        size / f32::from(WIDTH),
        size / height,
    )
}

/// `sprite` centered on `at`, `half` its half-size along its own x and y,
/// turned `turn` radians, tinted `color`, in the current blend.
pub fn draw(sprite: Sprite, at: Vec2, half: Vec2, turn: f32, color: Color) {
    if color.a <= 0.003 || half.x < 0.05 || half.y < 0.05 {
        return;
    }
    let uv = cell(sprite, (half.x * half.y).sqrt() * 2.);
    let (sin, cos) = turn.sin_cos();
    let x = vec2(cos, sin) * half.x;
    let y = vec2(-sin, cos) * half.y;
    let corner = |u: f32, v: f32| {
        let p = at + x * (u * 2. - 1.) + y * (v * 2. - 1.);
        Vertex::new(p.x, p.y, 0., uv.x + uv.w * u, uv.y + uv.h * v, color)
    };
    let vertices = [
        corner(0., 0.),
        corner(1., 0.),
        corner(1., 1.),
        corner(0., 1.),
    ];
    SHEET.with(|sheet| {
        crate::lighting::textured_geometry(&vertices, &[0, 1, 2, 0, 2, 3], Some(sheet));
    });
}

/// A ring of `radius` around `at`, `width` pixels thick, fading out over
/// `feather` pixels either side: about a pixel keeps it smooth at any size,
/// and a wide feather makes a soft band of light. In the current blend.
pub fn ring(at: Vec2, radius: f32, width: f32, feather: f32, color: Color) {
    if color.a <= 0.003 || radius < 0.5 {
        return;
    }
    let half = width * 0.5;
    let feather = feather.max(0.35);
    let edges = [
        ((radius - half - feather).max(0.), 0.),
        ((radius - half).max(0.), 1.),
        (radius + half, 1.),
        (radius + half + feather, 0.),
    ];
    let sides = usize::from(crate::graphics::circle_sides(radius + half + feather));
    RING.with(|buffers| {
        let (vertices, indices) = &mut *buffers.borrow_mut();
        vertices.clear();
        indices.clear();
        for i in 0..sides {
            let a = i as f32 / sides as f32 * std::f32::consts::TAU;
            let (sin, cos) = a.sin_cos();
            for (reach, strength) in edges {
                let p = at + vec2(cos, sin) * reach;
                vertices.push(Vertex::new(
                    p.x,
                    p.y,
                    0.,
                    0.,
                    0.,
                    Color {
                        a: color.a * strength,
                        ..color
                    },
                ));
            }
        }
        for i in 0..sides {
            let here = (i * 4) as u16;
            let next = (((i + 1) % sides) * 4) as u16;
            for band in 0..3 {
                let (a, b) = (here + band, next + band);
                indices.extend_from_slice(&[a, b, b + 1, a, b + 1, a + 1]);
            }
        }
        crate::lighting::geometry(vertices, indices);
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shapes_fill_their_middles_and_leave_their_edges_clear() {
        let pixels = sheet_pixels();
        let alpha = |x: usize, y: usize| pixels[(y * usize::from(WIDTH) + x) * 4 + 3];
        for (row, sprite) in Sprite::ALL.into_iter().enumerate() {
            for (&size, &column) in SIZES.iter().zip(&COLUMNS) {
                let (x0, y0) = (usize::from(column), row * usize::from(ROW));
                let size = usize::from(size);
                // Every border pixel is clear, so neighbours never bleed in.
                for i in 0..size {
                    for (x, y) in [(i, 0), (i, size - 1), (0, i), (size - 1, i)] {
                        assert!(alpha(x0 + x, y0 + y) < 24, "{sprite:?} {size} at {x},{y}");
                    }
                }
                if sprite != Sprite::Spark {
                    let middle = alpha(x0 + size / 2, y0 + size / 2);
                    assert!(middle > 200, "{sprite:?} {size} is {middle} in the middle");
                }
            }
        }
        // The pixels between cells are clear too.
        for y in 0..usize::from(ROW) * Sprite::ALL.len() {
            for x in [128, 129, 130, 131, 196, 197, 198, 199, 232, 233, 234, 235] {
                assert_eq!(alpha(x, y), 0);
            }
        }
    }

    #[test]
    fn a_particle_samples_the_smallest_size_at_least_as_big_as_it_is() {
        let height = f32::from(ROW) * Sprite::ALL.len() as f32;
        let width = |pixels: f32| cell(Sprite::Disc, pixels).w * f32::from(WIDTH);
        assert_eq!(width(4.), 16.);
        // A little larger than a size still uses it, rather than blurring
        // a size twice as big down.
        assert_eq!(width(17.), 16.);
        assert_eq!(width(18.), 32.);
        assert_eq!(width(60.), 64.);
        assert_eq!(width(500.), 128.);
        let star = cell(Sprite::Star, 30.);
        assert!((star.y * height - 3. * f32::from(ROW)).abs() < 1e-3);
    }
}
