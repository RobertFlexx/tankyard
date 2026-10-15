//! Render the game's actual tank primitives to SVG without opening a window.
use clap::Parser;
use macroquad::prelude::{Color, Vec2, vec2};
use std::{fmt::Write, path::PathBuf};
use tankyard_client::tank_model::{Primitive, class_model_geometry};
use tankyard_content::Content;

#[derive(Parser)]
#[command(about = "Export a tank model sheet using the native game's vector geometry")]
struct Args {
    #[arg(long, default_value = "artifacts/models/tanks.svg")]
    output: PathBuf,
    #[arg(long)]
    content: Option<PathBuf>,
    #[arg(long, value_delimiter = ',')]
    classes: Vec<String>,
    #[arg(long, default_value_t = -0.45)]
    aim: f32,
    #[arg(long, default_value_t = 0.0)]
    time: f32,
    #[arg(long, default_value_t = 8)]
    columns: usize,
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();
    if !args.aim.is_finite() || !args.time.is_finite() || !(1..=16).contains(&args.columns) {
        return Err("Use finite aim/time and 1 to 16 columns.".into());
    }
    let content = if let Some(path) = args.content {
        Content::from_json(&std::fs::read_to_string(path)?)?
    } else {
        Content::builtin()
    };
    for id in &args.classes {
        if content.class(id).is_none() {
            return Err(format!("Unknown class: {id}").into());
        }
    }
    let classes: Vec<_> = content
        .classes
        .iter()
        .filter(|class| args.classes.is_empty() || args.classes.contains(&class.id))
        .collect();
    let columns = args.columns.min(classes.len());
    let rows = classes.len().div_ceil(columns);
    let mut extent = Vec2::ONE;
    let color = Color::new(0.0, 0.698, 0.882, 1.0);
    for class in &classes {
        let radius = 22.0 * class.multipliers[0].sqrt().clamp(0.85, 1.6) * class.body_scale * 1.3;
        let mut include = |point: Vec2| {
            extent = extent.max(point.abs());
        };
        class_model_geometry(
            class,
            Vec2::ZERO,
            radius,
            args.aim,
            0.0,
            0.0,
            color,
            args.time,
            None,
            None,
            Some(1.3),
            &mut |primitive| match primitive {
                Primitive::Polygon { center, radius, .. } => {
                    include(center + Vec2::splat(radius));
                    include(center - Vec2::splat(radius));
                }
                Primitive::Triangle { vertices, .. } => {
                    for point in vertices {
                        include(point);
                    }
                }
                Primitive::Quad { vertices, .. } => {
                    for point in vertices {
                        include(point);
                    }
                }
            },
        );
    }
    let cell_width = (extent.x * 2.0 + 32.0).ceil() as usize;
    let cell_height = (extent.y * 2.0 + 64.0).ceil() as usize;
    let width = columns * cell_width;
    let height = rows * cell_height;
    let mut svg = format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{width}\" height=\"{height}\" viewBox=\"0 0 {width} {height}\"><rect width=\"100%\" height=\"100%\" fill=\"#cdcdcd\"/><defs><pattern id=\"grid\" width=\"20\" height=\"20\" patternUnits=\"userSpaceOnUse\"><path d=\"M20 0H0V20\" fill=\"none\" stroke=\"#c3c3c3\" stroke-width=\"1\"/></pattern></defs><rect width=\"100%\" height=\"100%\" fill=\"url(#grid)\"/>\n"
    );
    for (i, class) in classes.iter().enumerate() {
        let x = (i % columns) as f32 * cell_width as f32 + cell_width as f32 * 0.5;
        let y = (i / columns) as f32 * cell_height as f32 + extent.y + 16.0;
        let radius = 22.0 * class.multipliers[0].sqrt().clamp(0.85, 1.6) * class.body_scale * 1.3;
        class_model_geometry(
            class,
            vec2(x, y),
            radius,
            args.aim,
            0.0,
            0.0,
            color,
            args.time,
            None,
            None,
            Some(1.3),
            &mut |primitive| write_primitive(&mut svg, primitive),
        );
        writeln!(
            svg,
            "<text x=\"{x}\" y=\"{}\" text-anchor=\"middle\" font-family=\"DejaVu Sans,sans-serif\" font-weight=\"bold\" font-size=\"13\" fill=\"#404040\">{}</text>",
            (i / columns + 1) as f32 * cell_height as f32 - 18.0,
            escape(&class.name)
        )?;
    }
    svg.push_str("</svg>\n");
    if let Some(parent) = args
        .output
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
    {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&args.output, svg)?;
    println!(
        "Exported {} tanks to {}",
        classes.len(),
        args.output.display()
    );
    Ok(())
}
fn escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}
fn write_primitive(svg: &mut String, primitive: Primitive) {
    let (vertices, color) = match primitive {
        Primitive::Polygon {
            center,
            sides,
            radius,
            angle,
            color,
        } => {
            let count = if sides < 3 { 64 } else { usize::from(sides) };
            let vertices = (0..count)
                .map(|i| {
                    let a = angle + i as f32 * std::f32::consts::TAU / count as f32;
                    center + vec2(a.cos(), a.sin()) * radius
                })
                .collect::<Vec<_>>();
            (vertices, color)
        }
        Primitive::Triangle { vertices, color } => (vertices.to_vec(), color),
        Primitive::Quad { vertices, color } => (vertices.to_vec(), color),
    };
    let points = vertices
        .iter()
        .map(|p| format!("{:.3},{:.3}", p.x, p.y))
        .collect::<Vec<_>>()
        .join(" ");
    writeln!(
        svg,
        "<polygon points=\"{points}\" fill=\"rgb({},{},{})\" fill-opacity=\"{:.4}\"/>",
        (color.r * 255.0).round() as u8,
        (color.g * 255.0).round() as u8,
        (color.b * 255.0).round() as u8,
        color.a
    )
    .unwrap();
}
