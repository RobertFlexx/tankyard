//! Turning a pack's art into tank textures. Images, vector art, icon and paint
//! editor files load directly; 3D models render top-down into a sprite with
//! the game's dark outline. Every decoder is bounded: large sources shrink to
//! fit instead of being rejected, and dimensions are checked before pixels
//! are allocated.
use image::{RgbaImage, imageops::FilterType};

/// Largest side of a texture handed to the game.
pub const MAX_SIDE: u32 = 256;
/// Largest side a source image may have before it is shrunk.
const MAX_SOURCE_SIDE: u32 = 4096;
const MAX_SOURCE_BYTES: u64 = 64 * 1024 * 1024;
const MAX_TRIANGLES: usize = 250_000;

pub const IMAGE_EXTENSIONS: &[&str] = &[
    "png", "jpg", "jpeg", "gif", "bmp", "tga", "webp", "ico", "icns", "svg", "ase", "aseprite",
    "psd", "kra",
];
pub const MODEL_EXTENSIONS: &[&str] = &["obj", "stl", "ply", "gltf", "glb"];

pub fn extension(name: &str) -> String {
    name.rsplit_once('.')
        .map(|(_, ext)| ext.to_ascii_lowercase())
        .unwrap_or_default()
}

/// Whether `name` is art the game can turn into a texture.
pub fn is_asset(name: &str) -> bool {
    let ext = extension(name);
    IMAGE_EXTENSIONS.contains(&ext.as_str()) || MODEL_EXTENSIONS.contains(&ext.as_str())
}

/// Whether `data` has the signature its extension promises. The scanner uses
/// this to catch files disguised as art without decoding them.
pub fn looks_like(ext: &str, data: &[u8]) -> bool {
    let text = || std::str::from_utf8(data).is_ok_and(|t| !t.contains('\0'));
    match ext {
        "png" => data.starts_with(b"\x89PNG\r\n\x1a\n"),
        "jpg" | "jpeg" => data.starts_with(&[0xff, 0xd8, 0xff]),
        "gif" => data.starts_with(b"GIF87a") || data.starts_with(b"GIF89a"),
        "bmp" => data.starts_with(b"BM"),
        "webp" => data.starts_with(b"RIFF") && data.get(8..12) == Some(b"WEBP"),
        "ico" => data.starts_with(&[0, 0, 1, 0]),
        "icns" => data.starts_with(b"icns"),
        // TGA has no signature; its decoder bounds what it reads.
        "tga" => data.len() >= 18,
        "svg" => text() && String::from_utf8_lossy(data).contains("<svg"),
        "ase" | "aseprite" => data.get(4..6) == Some(&[0xe0, 0xa5]),
        "psd" => data.starts_with(b"8BPS"),
        "kra" => {
            data.starts_with(b"PK\x03\x04")
                && data
                    .windows(19)
                    .take(256)
                    .any(|w| w == b"application/x-krita")
        }
        "glb" => data.starts_with(b"glTF"),
        "gltf" => serde_json::from_slice::<serde_json::Value>(data).is_ok(),
        "ply" => data.starts_with(b"ply"),
        "stl" => {
            data.get(80..84).is_some_and(|n| {
                84 + u32::from_le_bytes([n[0], n[1], n[2], n[3]]) as usize * 50 == data.len()
            }) || (text() && data.starts_with(b"solid"))
        }
        "obj" | "mtl" => text(),
        _ => false,
    }
}

/// Other files in the same pack, for formats that reference them (glTF
/// buffers, OBJ materials). Paths are pack-relative.
pub type Files<'a> = dyn Fn(&str) -> Option<Vec<u8>> + 'a;

/// Decode `bytes`, named `name` inside its pack, into an image no larger than
/// [`MAX_SIDE`] on either side.
pub fn decode(name: &str, bytes: &[u8], files: &Files) -> Result<RgbaImage, String> {
    let ext = extension(name);
    let image = match ext.as_str() {
        "svg" => svg(bytes)?,
        "icns" => icns(bytes)?,
        "ase" | "aseprite" => aseprite(bytes)?,
        "psd" => psd(bytes)?,
        "kra" => krita(bytes)?,
        "obj" => render(&obj(name, bytes, files)?, Up::Y),
        "stl" => render(&stl(bytes)?, Up::Z),
        "ply" => render(&ply(bytes)?, Up::Y),
        "gltf" | "glb" => render(&gltf(name, bytes, files)?, Up::Y),
        _ if IMAGE_EXTENSIONS.contains(&ext.as_str()) => raster(bytes)?,
        _ => return Err(format!(".{ext} files are not a supported asset format")),
    };
    Ok(fit(image))
}

fn fit(image: RgbaImage) -> RgbaImage {
    if image.width() <= MAX_SIDE && image.height() <= MAX_SIDE {
        return image;
    }
    let scale = MAX_SIDE as f32 / image.width().max(image.height()) as f32;
    let (w, h) = (
        ((image.width() as f32 * scale).round() as u32).clamp(1, MAX_SIDE),
        ((image.height() as f32 * scale).round() as u32).clamp(1, MAX_SIDE),
    );
    image::imageops::resize(&image, w, h, FilterType::Lanczos3)
}

fn check_size(width: u64, height: u64) -> Result<(), String> {
    if width == 0 || height == 0 {
        return Err("the image is empty".into());
    }
    if width > u64::from(MAX_SOURCE_SIDE) || height > u64::from(MAX_SOURCE_SIDE) {
        return Err(format!(
            "the image is {width}×{height}; sources may be at most {MAX_SOURCE_SIDE} pixels on a side"
        ));
    }
    Ok(())
}

fn raster(bytes: &[u8]) -> Result<RgbaImage, String> {
    let mut reader = image::ImageReader::new(std::io::Cursor::new(bytes))
        .with_guessed_format()
        .map_err(|e| e.to_string())?;
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(MAX_SOURCE_SIDE);
    limits.max_image_height = Some(MAX_SOURCE_SIDE);
    limits.max_alloc = Some(MAX_SOURCE_BYTES);
    reader.limits(limits);
    Ok(reader
        .decode()
        .map_err(|e| format!("invalid image: {e}"))?
        .into_rgba8())
}

fn svg(bytes: &[u8]) -> Result<RgbaImage, String> {
    use resvg::{tiny_skia, usvg};
    // No system fonts or external files: text without outlines is skipped.
    let options = usvg::Options {
        resources_dir: None,
        ..Default::default()
    };
    let tree = usvg::Tree::from_data(bytes, &options).map_err(|e| format!("invalid SVG: {e}"))?;
    let size = tree.size();
    let scale = MAX_SIDE as f32 / size.width().max(size.height()).max(1.0);
    let (w, h) = (
        ((size.width() * scale).ceil() as u32).clamp(1, MAX_SIDE),
        ((size.height() * scale).ceil() as u32).clamp(1, MAX_SIDE),
    );
    let mut pixmap = tiny_skia::Pixmap::new(w, h).ok_or("invalid SVG size")?;
    resvg::render(
        &tree,
        tiny_skia::Transform::from_scale(scale, scale),
        &mut pixmap.as_mut(),
    );
    // tiny-skia stores premultiplied alpha.
    let mut rgba = pixmap.take();
    for pixel in rgba.as_chunks_mut::<4>().0 {
        let a = pixel[3];
        if a > 0 && a < 255 {
            for channel in &mut pixel[..3] {
                *channel = ((u16::from(*channel) * 255 + u16::from(a) / 2) / u16::from(a)) as u8;
            }
        }
    }
    RgbaImage::from_raw(w, h, rgba).ok_or_else(|| "invalid SVG".into())
}

fn icns(bytes: &[u8]) -> Result<RgbaImage, String> {
    let family = icns::IconFamily::read(std::io::Cursor::new(bytes))
        .map_err(|e| format!("invalid ICNS: {e}"))?;
    // The smallest icon at least as large as a texture, else the largest.
    let mut types = family.available_icons();
    types.sort_by_key(|t| t.pixel_width());
    let chosen = types
        .iter()
        .find(|t| t.pixel_width() >= MAX_SIDE)
        .or(types.last())
        .copied()
        .ok_or("the ICNS file has no icons")?;
    let icon = family
        .get_icon_with_type(chosen)
        .map_err(|e| format!("invalid ICNS: {e}"))?
        .convert_to(icns::PixelFormat::RGBA);
    RgbaImage::from_raw(icon.width(), icon.height(), icon.into_data().into_vec())
        .ok_or_else(|| "invalid ICNS".into())
}

fn aseprite(bytes: &[u8]) -> Result<RgbaImage, String> {
    // The header holds the canvas size; check it before any frames decode.
    let (w, h) = (
        u16::from_le_bytes([*bytes.get(8).ok_or("invalid Aseprite file")?, bytes[9]]),
        u16::from_le_bytes([*bytes.get(10).ok_or("invalid Aseprite file")?, bytes[11]]),
    );
    check_size(u64::from(w), u64::from(h))?;
    let file = asefile::AsepriteFile::read(std::io::Cursor::new(bytes))
        .map_err(|e| format!("invalid Aseprite file: {e}"))?;
    let frame = file.frame(0).image();
    RgbaImage::from_raw(frame.width(), frame.height(), frame.into_raw())
        .ok_or_else(|| "invalid Aseprite file".into())
}

fn psd(bytes: &[u8]) -> Result<RgbaImage, String> {
    if bytes.len() < 26 || &bytes[..4] != b"8BPS" {
        return Err("invalid Photoshop file".into());
    }
    let h = u32::from_be_bytes([bytes[14], bytes[15], bytes[16], bytes[17]]);
    let w = u32::from_be_bytes([bytes[18], bytes[19], bytes[20], bytes[21]]);
    check_size(u64::from(w), u64::from(h))?;
    let file = psd::Psd::from_bytes(bytes).map_err(|e| format!("invalid Photoshop file: {e}"))?;
    RgbaImage::from_raw(file.width(), file.height(), file.rgba())
        .ok_or_else(|| "invalid Photoshop file".into())
}

/// Krita documents are zip files that carry a flattened PNG of the canvas.
fn krita(bytes: &[u8]) -> Result<RgbaImage, String> {
    use std::io::Read;
    let mut archive = zip::ZipArchive::new(std::io::Cursor::new(bytes))
        .map_err(|e| format!("invalid Krita file: {e}"))?;
    let mut merged = archive
        .by_name("mergedimage.png")
        .map_err(|_| "the Krita file has no merged image; save it again from Krita")?;
    if merged.size() > MAX_SOURCE_BYTES {
        return Err("the Krita image is too large".into());
    }
    let mut png = Vec::new();
    merged
        .by_ref()
        .take(MAX_SOURCE_BYTES)
        .read_to_end(&mut png)
        .map_err(|e| e.to_string())?;
    raster(&png)
}

// --------------------------------------------------------------------------
// 3D models

#[derive(Clone, Copy, Debug)]
pub struct Triangle {
    pub points: [[f32; 3]; 3],
    pub color: [f32; 4],
}

const DEFAULT_COLOR: [f32; 4] = [0.62, 0.66, 0.70, 1.0];

#[derive(Clone, Copy)]
enum Up {
    Y,
    Z,
}

fn push(triangles: &mut Vec<Triangle>, triangle: Triangle) -> Result<(), String> {
    if triangles.len() >= MAX_TRIANGLES {
        return Err(format!("models may have at most {MAX_TRIANGLES} triangles"));
    }
    if triangle.points.iter().flatten().all(|v| v.is_finite()) {
        triangles.push(triangle);
    }
    Ok(())
}

/// Resolve `uri` beside the pack file `name`, refusing paths that leave it.
fn sibling(name: &str, uri: &str) -> Option<String> {
    let dir = name.rsplit_once('/').map_or("", |(dir, _)| dir);
    let path = if dir.is_empty() {
        uri.to_owned()
    } else {
        format!("{dir}/{uri}")
    };
    crate::pack_path(&path).then_some(path)
}

fn text(bytes: &[u8], what: &str) -> Result<String, String> {
    String::from_utf8(bytes.to_vec()).map_err(|_| format!("{what} files must be text"))
}

fn obj(name: &str, bytes: &[u8], files: &Files) -> Result<Vec<Triangle>, String> {
    let source = text(bytes, "OBJ")?;
    let mut materials = std::collections::HashMap::new();
    let mut vertices: Vec<([f32; 3], Option<[f32; 3]>)> = Vec::new();
    let mut color = DEFAULT_COLOR;
    let mut triangles = Vec::new();
    let number = |part: Option<&str>| -> f32 { part.and_then(|v| v.parse().ok()).unwrap_or(0.0) };
    for line in source.lines() {
        let mut parts = line.split_whitespace();
        match parts.next() {
            Some("v") => {
                let p = [
                    number(parts.next()),
                    number(parts.next()),
                    number(parts.next()),
                ];
                let rest: Vec<f32> = parts.filter_map(|v| v.parse().ok()).collect();
                let vertex_color = (rest.len() >= 3).then(|| [rest[0], rest[1], rest[2]]);
                if vertices.len() >= MAX_TRIANGLES * 3 {
                    return Err("the OBJ model has too many vertices".into());
                }
                vertices.push((p, vertex_color));
            }
            Some("mtllib") => {
                for library in parts {
                    if let Some(data) = sibling(name, library).and_then(|path| files(&path)) {
                        mtl(&String::from_utf8_lossy(&data), &mut materials);
                    }
                }
            }
            Some("usemtl") => {
                color = parts
                    .next()
                    .and_then(|m| materials.get(m))
                    .copied()
                    .unwrap_or(DEFAULT_COLOR);
            }
            Some("f") => {
                let corners = parts
                    .map(|corner| {
                        let index: i64 = corner
                            .split('/')
                            .next()
                            .and_then(|i| i.parse().ok())
                            .ok_or("invalid OBJ face")?;
                        let resolved = if index < 0 {
                            vertices.len() as i64 + index
                        } else {
                            index - 1
                        };
                        usize::try_from(resolved)
                            .ok()
                            .and_then(|i| vertices.get(i))
                            .copied()
                            .ok_or("an OBJ face refers to a missing vertex")
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                // Fan-triangulate polygons.
                for i in 1..corners.len().saturating_sub(1) {
                    let tri = [corners[0], corners[i], corners[i + 1]];
                    let face = match (tri[0].1, tri[1].1, tri[2].1) {
                        (Some(a), Some(b), Some(c)) => {
                            let avg = |k: usize| (a[k] + b[k] + c[k]) / 3.0;
                            [avg(0), avg(1), avg(2), 1.0]
                        }
                        _ => color,
                    };
                    push(
                        &mut triangles,
                        Triangle {
                            points: [tri[0].0, tri[1].0, tri[2].0],
                            color: face,
                        },
                    )?;
                }
            }
            _ => {}
        }
    }
    Ok(triangles)
}

fn mtl(source: &str, materials: &mut std::collections::HashMap<String, [f32; 4]>) {
    let mut current = None;
    for line in source.lines() {
        let mut parts = line.split_whitespace();
        match parts.next() {
            Some("newmtl") => {
                current = parts.next().map(str::to_owned);
                if let Some(name) = &current {
                    materials.insert(name.clone(), DEFAULT_COLOR);
                }
            }
            Some("Kd") => {
                let rgb: Vec<f32> = parts.filter_map(|v| v.parse().ok()).collect();
                if let (Some(name), [r, g, b, ..]) = (&current, rgb.as_slice()) {
                    materials.insert(name.clone(), [*r, *g, *b, 1.0]);
                }
            }
            _ => {}
        }
    }
}

fn stl(bytes: &[u8]) -> Result<Vec<Triangle>, String> {
    let mut triangles = Vec::new();
    let binary_count = bytes
        .get(80..84)
        .map(|n| u32::from_le_bytes([n[0], n[1], n[2], n[3]]) as usize);
    if let Some(count) = binary_count.filter(|&n| 84 + n * 50 == bytes.len()) {
        for face in bytes[84..].as_chunks::<50>().0.iter().take(count) {
            let f = |i: usize| f32::from_le_bytes([face[i], face[i + 1], face[i + 2], face[i + 3]]);
            let point = |k: usize| [f(12 + k * 12), f(16 + k * 12), f(20 + k * 12)];
            push(
                &mut triangles,
                Triangle {
                    points: [point(0), point(1), point(2)],
                    color: DEFAULT_COLOR,
                },
            )?;
        }
        return Ok(triangles);
    }
    let source = text(bytes, "STL")?;
    let mut corners = Vec::with_capacity(3);
    for line in source.lines() {
        let mut parts = line.split_whitespace();
        if parts.next() == Some("vertex") {
            let v: Vec<f32> = parts.filter_map(|v| v.parse().ok()).collect();
            if v.len() != 3 {
                return Err("invalid STL vertex".into());
            }
            corners.push([v[0], v[1], v[2]]);
            if corners.len() == 3 {
                push(
                    &mut triangles,
                    Triangle {
                        points: [corners[0], corners[1], corners[2]],
                        color: DEFAULT_COLOR,
                    },
                )?;
                corners.clear();
            }
        }
    }
    if triangles.is_empty() {
        return Err("the STL file has no triangles".into());
    }
    Ok(triangles)
}

/// A PLY element: name, count, and properties as (name, type, list count type).
type PlyElement = (String, usize, Vec<(String, String, Option<String>)>);

/// ASCII and little-endian binary PLY with optional vertex colors.
fn ply(bytes: &[u8]) -> Result<Vec<Triangle>, String> {
    let end = bytes
        .windows(10)
        .position(|w| w == b"end_header")
        .ok_or("invalid PLY header")?;
    let header = std::str::from_utf8(&bytes[..end]).map_err(|_| "invalid PLY header")?;
    let mut body = &bytes[end + 10..];
    while let [b'\r' | b'\n', rest @ ..] = body {
        body = rest;
    }
    let mut lines = header.lines();
    if lines.next().map(str::trim) != Some("ply") {
        return Err("invalid PLY header".into());
    }
    let mut ascii = false;
    let mut elements: Vec<PlyElement> = Vec::new();
    for line in lines {
        let parts: Vec<&str> = line.split_whitespace().collect();
        match parts.as_slice() {
            ["format", "ascii", ..] => ascii = true,
            ["format", "binary_little_endian", ..] => {}
            ["format", other, ..] => return Err(format!("PLY format {other} is not supported")),
            ["element", name, count] => {
                let count: usize = count.parse().map_err(|_| "invalid PLY element")?;
                if count > MAX_TRIANGLES * 3 {
                    return Err("the PLY model is too large".into());
                }
                elements.push((name.to_string(), count, Vec::new()));
            }
            ["property", "list", count, item, name] => {
                if let Some(element) = elements.last_mut() {
                    element
                        .2
                        .push((name.to_string(), item.to_string(), Some(count.to_string())));
                }
            }
            ["property", kind, name] => {
                if let Some(element) = elements.last_mut() {
                    element.2.push((name.to_string(), kind.to_string(), None));
                }
            }
            _ => {}
        }
    }
    let mut tokens = ascii.then(|| {
        std::str::from_utf8(body)
            .unwrap_or_default()
            .split_ascii_whitespace()
    });
    let mut offset = 0usize;
    let mut read = |kind: &str| -> Result<f64, String> {
        if let Some(tokens) = &mut tokens {
            return tokens
                .next()
                .and_then(|t| t.parse().ok())
                .ok_or_else(|| "truncated PLY data".to_owned());
        }
        let size = match kind {
            "char" | "uchar" | "int8" | "uint8" => 1,
            "short" | "ushort" | "int16" | "uint16" => 2,
            "int" | "uint" | "float" | "int32" | "uint32" | "float32" => 4,
            "double" | "float64" => 8,
            _ => return Err(format!("PLY type {kind} is not supported")),
        };
        let b = body
            .get(offset..offset + size)
            .ok_or("truncated PLY data")?;
        offset += size;
        Ok(match kind {
            "char" | "int8" => f64::from(b[0] as i8),
            "uchar" | "uint8" => f64::from(b[0]),
            "short" | "int16" => f64::from(i16::from_le_bytes([b[0], b[1]])),
            "ushort" | "uint16" => f64::from(u16::from_le_bytes([b[0], b[1]])),
            "int" | "int32" => f64::from(i32::from_le_bytes([b[0], b[1], b[2], b[3]])),
            "uint" | "uint32" => f64::from(u32::from_le_bytes([b[0], b[1], b[2], b[3]])),
            "float" | "float32" => f64::from(f32::from_le_bytes([b[0], b[1], b[2], b[3]])),
            _ => f64::from_le_bytes([b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7]]),
        })
    };
    let mut vertices: Vec<([f32; 3], [f32; 4])> = Vec::new();
    let mut triangles = Vec::new();
    for (element, count, properties) in &elements {
        for _ in 0..*count {
            let mut position = [0.0f32; 3];
            let mut color = DEFAULT_COLOR;
            let mut indices = Vec::new();
            for (name, kind, list) in properties {
                if let Some(count_kind) = list {
                    let n = read(count_kind)? as usize;
                    if n > 64 {
                        return Err("a PLY face has too many corners".into());
                    }
                    for _ in 0..n {
                        indices.push(read(kind)? as usize);
                    }
                    continue;
                }
                let value = read(kind)?;
                let unit = |v: f64| {
                    if kind.contains("char") || kind.contains("int8") {
                        v as f32 / 255.0
                    } else {
                        v as f32
                    }
                };
                match name.as_str() {
                    "x" => position[0] = value as f32,
                    "y" => position[1] = value as f32,
                    "z" => position[2] = value as f32,
                    "red" | "r" => color[0] = unit(value),
                    "green" | "g" => color[1] = unit(value),
                    "blue" | "b" => color[2] = unit(value),
                    _ => {}
                }
            }
            match element.as_str() {
                "vertex" => vertices.push((position, color)),
                "face" => {
                    let corner = |i: usize| {
                        vertices
                            .get(i)
                            .copied()
                            .ok_or("a PLY face refers to a missing vertex")
                    };
                    for i in 1..indices.len().saturating_sub(1) {
                        let (a, b, c) = (
                            corner(indices[0])?,
                            corner(indices[i])?,
                            corner(indices[i + 1])?,
                        );
                        let avg = |k: usize| (a.1[k] + b.1[k] + c.1[k]) / 3.0;
                        push(
                            &mut triangles,
                            Triangle {
                                points: [a.0, b.0, c.0],
                                color: [avg(0), avg(1), avg(2), 1.0],
                            },
                        )?;
                    }
                }
                _ => {}
            }
        }
    }
    Ok(triangles)
}

fn base64(data: &str) -> Result<Vec<u8>, String> {
    let mut out = Vec::with_capacity(data.len() * 3 / 4);
    let (mut buffer, mut bits) = (0u32, 0u32);
    for byte in data
        .bytes()
        .filter(|b| !b.is_ascii_whitespace() && *b != b'=')
    {
        let value = match byte {
            b'A'..=b'Z' => byte - b'A',
            b'a'..=b'z' => byte - b'a' + 26,
            b'0'..=b'9' => byte - b'0' + 52,
            b'+' | b'-' => 62,
            b'/' | b'_' => 63,
            _ => return Err("invalid base64 data".into()),
        };
        buffer = (buffer << 6) | u32::from(value);
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((buffer >> bits) as u8);
        }
    }
    Ok(out)
}

type Matrix = [[f32; 4]; 4];

fn multiply(a: &Matrix, b: &Matrix) -> Matrix {
    let mut out = [[0.0; 4]; 4];
    for (c, column) in out.iter_mut().enumerate() {
        for (r, cell) in column.iter_mut().enumerate() {
            *cell = (0..4).map(|k| a[k][r] * b[c][k]).sum();
        }
    }
    out
}

fn transform(m: &Matrix, p: [f32; 3]) -> [f32; 3] {
    let at = |r: usize| m[0][r] * p[0] + m[1][r] * p[1] + m[2][r] * p[2] + m[3][r];
    [at(0), at(1), at(2)]
}

fn gltf(name: &str, bytes: &[u8], files: &Files) -> Result<Vec<Triangle>, String> {
    let document = gltf::Gltf::from_slice(bytes).map_err(|e| format!("invalid glTF: {e}"))?;
    let mut buffers = Vec::new();
    for buffer in document.buffers() {
        let data = match buffer.source() {
            gltf::buffer::Source::Bin => {
                document.blob.clone().ok_or("the GLB has no binary chunk")?
            }
            gltf::buffer::Source::Uri(uri) => {
                if let Some(encoded) = uri
                    .strip_prefix("data:")
                    .and_then(|d| d.split_once(";base64,"))
                {
                    base64(encoded.1)?
                } else {
                    sibling(name, uri)
                        .and_then(|path| files(&path))
                        .ok_or_else(|| format!("glTF buffer {uri} is missing from the pack"))?
                }
            }
        };
        if data.len() < buffer.length() {
            return Err("a glTF buffer is shorter than declared".into());
        }
        buffers.push(data);
    }
    let mut triangles = Vec::new();
    let scene = document
        .default_scene()
        .or_else(|| document.scenes().next())
        .ok_or("the glTF file has no scene")?;
    let identity: Matrix = [
        [1., 0., 0., 0.],
        [0., 1., 0., 0.],
        [0., 0., 1., 0.],
        [0., 0., 0., 1.],
    ];
    let mut stack: Vec<(gltf::Node, Matrix, usize)> =
        scene.nodes().map(|n| (n, identity, 0)).collect();
    while let Some((node, parent, depth)) = stack.pop() {
        if depth > 64 {
            return Err("the glTF node tree is too deep".into());
        }
        let world = multiply(&parent, &node.transform().matrix());
        if let Some(mesh) = node.mesh() {
            for primitive in mesh.primitives() {
                if primitive.mode() != gltf::mesh::Mode::Triangles {
                    continue;
                }
                let reader = primitive.reader(|b| buffers.get(b.index()).map(Vec::as_slice));
                let Some(positions) = reader.read_positions() else {
                    continue;
                };
                let positions: Vec<[f32; 3]> = positions.take(MAX_TRIANGLES * 3).collect();
                let colors: Option<Vec<[f32; 4]>> =
                    reader.read_colors(0).map(|c| c.into_rgba_f32().collect());
                let base = primitive
                    .material()
                    .pbr_metallic_roughness()
                    .base_color_factor();
                let indices: Vec<usize> = match reader.read_indices() {
                    Some(indices) => indices.into_u32().map(|i| i as usize).collect(),
                    None => (0..positions.len()).collect(),
                };
                for corner in indices.as_chunks::<3>().0 {
                    let point = |i: usize| positions.get(i).map(|p| transform(&world, *p));
                    let (Some(a), Some(b), Some(c)) =
                        (point(corner[0]), point(corner[1]), point(corner[2]))
                    else {
                        return Err("a glTF triangle refers to a missing vertex".into());
                    };
                    let mut color = base;
                    if let Some(colors) = &colors {
                        for k in 0..4 {
                            let avg = corner
                                .iter()
                                .map(|&i| colors.get(i).map_or(1.0, |c| c[k]))
                                .sum::<f32>()
                                / 3.0;
                            color[k] *= avg;
                        }
                    }
                    push(
                        &mut triangles,
                        Triangle {
                            points: [a, b, c],
                            color,
                        },
                    )?;
                }
            }
        }
        stack.extend(node.children().map(|child| (child, world, depth + 1)));
    }
    if triangles.is_empty() {
        return Err("the glTF file has no triangle meshes".into());
    }
    Ok(triangles)
}

/// Render a model from above into a square sprite, lit from the upper left,
/// with the dark outline every tank in the game has.
fn render(triangles: &[Triangle], up: Up) -> RgbaImage {
    const SIZE: usize = MAX_SIDE as usize * 2;
    // View space: x right, y down, depth toward the camera. The model's
    // forward axis faces right, matching an unrotated tank.
    let view = |p: [f32; 3]| match up {
        Up::Y => [p[2], -p[0], p[1]],
        Up::Z => [p[0], -p[1], p[2]],
    };
    let points: Vec<[[f32; 3]; 3]> = triangles
        .iter()
        .map(|t| [view(t.points[0]), view(t.points[1]), view(t.points[2])])
        .collect();
    let (mut lo, mut hi) = ([f32::MAX; 2], [f32::MIN; 2]);
    for p in points.iter().flatten() {
        for k in 0..2 {
            lo[k] = lo[k].min(p[k]);
            hi[k] = hi[k].max(p[k]);
        }
    }
    let span = (hi[0] - lo[0]).max(hi[1] - lo[1]).max(1e-6);
    // Leave room for the outline.
    let margin = SIZE as f32 * 0.06;
    let scale = (SIZE as f32 - 2.0 * margin) / span;
    let center = [(lo[0] + hi[0]) * 0.5, (lo[1] + hi[1]) * 0.5];
    let to_pixel = |p: [f32; 3]| {
        [
            (p[0] - center[0]) * scale + SIZE as f32 * 0.5,
            (p[1] - center[1]) * scale + SIZE as f32 * 0.5,
            p[2],
        ]
    };
    let mut depth = vec![f32::MIN; SIZE * SIZE];
    let mut color = vec![[0f32; 4]; SIZE * SIZE];
    let light = {
        let l = [-0.45f32, -0.55, 0.70];
        let n = (l[0] * l[0] + l[1] * l[1] + l[2] * l[2]).sqrt();
        [l[0] / n, l[1] / n, l[2] / n]
    };
    for (tri, source) in points.iter().zip(triangles) {
        let [a, b, c] = [to_pixel(tri[0]), to_pixel(tri[1]), to_pixel(tri[2])];
        let e1 = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
        let e2 = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
        let mut normal = [
            e1[1] * e2[2] - e1[2] * e2[1],
            e1[2] * e2[0] - e1[0] * e2[2],
            e1[0] * e2[1] - e1[1] * e2[0],
        ];
        let length = (normal[0].powi(2) + normal[1].powi(2) + normal[2].powi(2)).sqrt();
        if length <= f32::EPSILON {
            continue;
        }
        normal = normal.map(|v| v / length);
        // Two-sided lighting: models from many tools disagree on winding.
        let lambert = (normal[0] * light[0] + normal[1] * light[1] + normal[2] * light[2]).abs();
        let shade = 0.55 + 0.45 * lambert;
        let area = e1[0] * e2[1] - e1[1] * e2[0];
        if area.abs() <= f32::EPSILON {
            continue;
        }
        let x0 = a[0].min(b[0]).min(c[0]).floor().max(0.0) as usize;
        let x1 = (a[0].max(b[0]).max(c[0]).ceil() as usize).min(SIZE - 1);
        let y0 = a[1].min(b[1]).min(c[1]).floor().max(0.0) as usize;
        let y1 = (a[1].max(b[1]).max(c[1]).ceil() as usize).min(SIZE - 1);
        for y in y0..=y1 {
            for x in x0..=x1 {
                let (px, py) = (x as f32 + 0.5, y as f32 + 0.5);
                let w0 = ((b[0] - px) * (c[1] - py) - (b[1] - py) * (c[0] - px)) / area;
                let w1 = ((c[0] - px) * (a[1] - py) - (c[1] - py) * (a[0] - px)) / area;
                let w2 = 1.0 - w0 - w1;
                if w0 < 0.0 || w1 < 0.0 || w2 < 0.0 {
                    continue;
                }
                let z = w0 * a[2] + w1 * b[2] + w2 * c[2];
                let i = y * SIZE + x;
                if z > depth[i] {
                    depth[i] = z;
                    color[i] = [
                        source.color[0] * shade,
                        source.color[1] * shade,
                        source.color[2] * shade,
                        source.color[3],
                    ];
                }
            }
        }
    }
    // A dark outline around the silhouette, about 2.5% of the sprite wide.
    let covered = |x: i64, y: i64| {
        (0..SIZE as i64).contains(&x)
            && (0..SIZE as i64).contains(&y)
            && depth[y as usize * SIZE + x as usize] > f32::MIN
    };
    let radius = (SIZE as f32 * 0.025).round() as i64;
    let mut pixels = vec![[0f32; 4]; SIZE * SIZE];
    for y in 0..SIZE as i64 {
        for x in 0..SIZE as i64 {
            let i = y as usize * SIZE + x as usize;
            if covered(x, y) {
                let edge = (-radius..=radius).any(|dy| {
                    (-radius..=radius)
                        .any(|dx| dx * dx + dy * dy <= radius * radius && !covered(x + dx, y + dy))
                });
                pixels[i] = if edge {
                    [0.33, 0.33, 0.33, 1.0]
                } else {
                    color[i]
                };
            } else if (-radius..=radius).any(|dy| {
                (-radius..=radius)
                    .any(|dx| dx * dx + dy * dy <= radius * radius && covered(x + dx, y + dy))
            }) {
                pixels[i] = [0.33, 0.33, 0.33, 1.0];
            }
        }
    }
    // Downsample 2×2 for smooth edges.
    let side = MAX_SIDE as usize;
    let mut out = RgbaImage::new(MAX_SIDE, MAX_SIDE);
    for y in 0..side {
        for x in 0..side {
            let mut sum = [0f32; 4];
            for (dx, dy) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
                let p = pixels[(y * 2 + dy) * SIZE + x * 2 + dx];
                for k in 0..3 {
                    sum[k] += p[k] * p[3];
                }
                sum[3] += p[3];
            }
            let a = sum[3] / 4.0;
            let rgb = |k: usize| {
                if sum[3] > 0.0 {
                    (sum[k] / sum[3]).clamp(0.0, 1.0)
                } else {
                    0.0
                }
            };
            out.put_pixel(
                x as u32,
                y as u32,
                image::Rgba([
                    (rgb(0) * 255.0).round() as u8,
                    (rgb(1) * 255.0).round() as u8,
                    (rgb(2) * 255.0).round() as u8,
                    (a * 255.0).round() as u8,
                ]),
            );
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn none(_: &str) -> Option<Vec<u8>> {
        None
    }

    fn png(w: u32, h: u32) -> Vec<u8> {
        let mut bytes = Vec::new();
        RgbaImage::from_pixel(w, h, image::Rgba([10, 200, 30, 255]))
            .write_to(
                &mut std::io::Cursor::new(&mut bytes),
                image::ImageFormat::Png,
            )
            .unwrap();
        bytes
    }

    #[test]
    fn large_images_shrink_to_texture_size_keeping_aspect() {
        let image = decode("art/big.png", &png(1024, 512), &none).unwrap();
        assert_eq!(image.dimensions(), (256, 128));
        assert!(decode("art/huge.png", &png(5000, 8), &none).is_err());
    }

    #[test]
    fn vector_art_renders_to_a_texture() {
        let svg = br##"<svg xmlns="http://www.w3.org/2000/svg" width="40" height="40"><circle cx="20" cy="20" r="18" fill="#00b2e1"/></svg>"##;
        let image = decode("icon.svg", svg, &none).unwrap();
        assert_eq!(image.dimensions(), (256, 256));
        let middle = image.get_pixel(128, 128);
        assert_eq!(middle.0[3], 255);
        assert!(middle.0[2] > 200);
        assert_eq!(image.get_pixel(0, 0).0[3], 0);
    }

    #[test]
    fn models_render_top_down_with_materials_and_an_outline() {
        let obj =
            b"mtllib tank.mtl\nusemtl red\nv -1 0 -1\nv 1 0 -1\nv 1 0 1\nv -1 0 1\nf 1 2 3 4\n";
        let mtl = b"newmtl red\nKd 1 0 0\n".to_vec();
        let files = |path: &str| (path == "models/tank.mtl").then(|| mtl.clone());
        let image = decode("models/tank.obj", obj, &files).unwrap();
        let middle = image.get_pixel(128, 128).0;
        assert!(middle[0] > 120 && middle[1] < 40, "{middle:?}");
        let edge = image.get_pixel(128, 9).0;
        assert!(edge[3] > 0 && edge[0] < 120, "{edge:?}");
        assert_eq!(image.get_pixel(0, 0).0[3], 0);
        let stl = b"solid t\nfacet normal 0 0 1\nouter loop\nvertex 0 0 0\nvertex 1 0 0\nvertex 0 1 0\nendloop\nendfacet\nendsolid\n";
        assert!(decode("t.stl", stl, &none).is_ok());
        let ply = b"ply\nformat ascii 1.0\nelement vertex 3\nproperty float x\nproperty float y\nproperty float z\nproperty uchar red\nproperty uchar green\nproperty uchar blue\nelement face 1\nproperty list uchar int vertex_indices\nend_header\n0 0 0 0 255 0\n1 0 0 0 255 0\n0 0 1 0 255 0\n3 0 1 2\n";
        let image = decode("t.ply", ply, &none).unwrap();
        assert!(image.pixels().any(|p| p.0[1] > 200 && p.0[0] < 60));
    }

    #[test]
    fn gltf_models_with_embedded_buffers_load() {
        // One triangle; positions as a base64 data URI.
        let floats: [f32; 9] = [0., 0., 0., 1., 0., 0., 0., 0., 1.];
        let raw: Vec<u8> = floats.iter().flat_map(|f| f.to_le_bytes()).collect();
        const TABLE: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
        let mut encoded = String::new();
        for chunk in raw.chunks(3) {
            let n = chunk
                .iter()
                .enumerate()
                .fold(0u32, |n, (i, b)| n | u32::from(*b) << (16 - 8 * i));
            for i in 0..=chunk.len() {
                encoded.push(TABLE[(n >> (18 - 6 * i) & 63) as usize] as char);
            }
        }
        let json = format!(
            r#"{{"asset":{{"version":"2.0"}},"scene":0,"scenes":[{{"nodes":[0]}}],"nodes":[{{"mesh":0}}],
            "meshes":[{{"primitives":[{{"attributes":{{"POSITION":0}}}}]}}],
            "accessors":[{{"bufferView":0,"componentType":5126,"count":3,"type":"VEC3","min":[0,0,0],"max":[1,0,1]}}],
            "bufferViews":[{{"buffer":0,"byteLength":36}}],
            "buffers":[{{"byteLength":36,"uri":"data:application/octet-stream;base64,{encoded}"}}]}}"#
        );
        let image = decode("ship.gltf", json.as_bytes(), &none).unwrap();
        assert!(image.pixels().filter(|p| p.0[3] == 255).count() > 1000);
    }
}
