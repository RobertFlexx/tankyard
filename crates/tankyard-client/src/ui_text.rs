//! Bounded text layout caches. Measuring a label must not rasterize its glyphs.
use std::{
    collections::{HashMap, VecDeque},
    sync::OnceLock,
};

const RASTERS: [u16; 15] = [
    8, 12, 16, 20, 24, 32, 40, 48, 64, 80, 96, 128, 160, 192, 256,
];
const MAX_LABELS: usize = 1024;
const MAX_LABEL_BYTES: usize = 1024;

pub fn raster_size(size: f32) -> u16 {
    RASTERS
        .iter()
        .copied()
        .find(|&pixels| f32::from(pixels) >= size)
        .unwrap_or(256)
}
fn font() -> &'static fontdue::Font {
    static FONT: OnceLock<fontdue::Font> = OnceLock::new();
    FONT.get_or_init(|| {
        fontdue::Font::from_bytes(
            include_bytes!("../../../assets/fonts/DejaVuSans-Bold.ttf") as &[u8],
            fontdue::FontSettings::default(),
        )
        .expect("bundled UI font")
    })
}
fn measure(text: &str, raster: u16, dpi: f32) -> Metrics {
    let pixels = (f32::from(raster) * dpi).ceil();
    let font = font();
    let mut previous = None;
    let mut width = 0.;
    let mut kerning = 0.;
    for character in text.chars() {
        if let Some(left) = previous {
            kerning += font.horizontal_kern(left, character, pixels).unwrap_or(0.);
        }
        width += font.metrics(character, pixels).advance_width;
        previous = Some(character);
    }
    Metrics {
        advance: width / dpi,
        kerning: kerning / dpi,
        prepared: false,
    }
}
#[derive(Clone, Copy)]
struct Metrics {
    advance: f32,
    kerning: f32,
    prepared: bool,
}
#[derive(Default)]
pub struct TextCache {
    dpi: u32,
    entries: HashMap<String, [Option<Metrics>; RASTERS.len()]>,
    order: VecDeque<String>,
}
impl TextCache {
    fn entry(&mut self, text: &str, raster: u16, dpi: f32) -> &mut Metrics {
        if self.dpi != dpi.to_bits() {
            self.entries.clear();
            self.order.clear();
            self.dpi = dpi.to_bits();
        }
        if !self.entries.contains_key(text) {
            if self.entries.len() == MAX_LABELS
                && let Some(oldest) = self.order.pop_front()
            {
                self.entries.remove(&oldest);
            }
            self.order.push_back(text.to_owned());
            self.entries.insert(text.to_owned(), [None; RASTERS.len()]);
        }
        let index = RASTERS.binary_search(&raster).expect("UI raster bucket");
        self.entries.get_mut(text).unwrap()[index].get_or_insert_with(|| measure(text, raster, dpi))
    }
    pub fn width(&mut self, text: &str, size: f32, dpi: f32) -> f32 {
        let raster = raster_size(size);
        let metrics = if text.len() > MAX_LABEL_BYTES {
            measure(text, raster, dpi)
        } else {
            *self.entry(text, raster, dpi)
        };
        // Macroquad 0.4.16 scales advances but adds kerning in raster units.
        // Match its rendered width, including fractional desktop DPI.
        metrics.advance * size / f32::from(raster) + metrics.kerning
    }
    /// Prime all missing glyphs before drawing the first one, so its atlas is
    /// uploaded once per label, rather than once for each new character.
    pub fn needs_glyphs(&mut self, text: &str, raster: u16, dpi: f32) -> bool {
        if text.len() > MAX_LABEL_BYTES {
            return true;
        }
        !std::mem::replace(&mut self.entry(text, raster, dpi).prepared, true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn repeated_scale_changes_reuse_a_small_set_of_glyph_sizes() {
        let sizes: std::collections::HashSet<_> =
            (1..=2560).map(|n| raster_size(n as f32 / 10.)).collect();
        assert_eq!(sizes.len(), RASTERS.len());
        for size in [9., 13.5, 24., 37.8, 180.] {
            assert!(f32::from(raster_size(size)) >= size);
        }
    }
    #[test]
    fn layout_matches_rendered_kerning_and_scales_at_multiple_desktop_dpis() {
        let mut cache = TextCache::default();
        for dpi in [1., 1.25, 2.] {
            assert!(
                cache.width("AV", 20., dpi)
                    < cache.width("A", 20., dpi) + cache.width("V", 20., dpi)
            );
            let a = cache.width("iiii 世界", 17., dpi);
            let b = cache.width("iiii 世界", 18., dpi);
            assert!((b / a - 18. / 17.).abs() < 0.0001);
        }
    }
    #[test]
    fn dynamic_text_stays_bounded_and_dpi_changes_invalidate_glyph_preparation() {
        let mut cache = TextCache::default();
        for score in 0..MAX_LABELS * 3 {
            assert!(cache.width(&format!("Score: {score}"), 16., 1.).is_finite());
        }
        assert_eq!(cache.entries.len(), MAX_LABELS);
        assert_eq!(cache.order.len(), MAX_LABELS);
        assert!(cache.needs_glyphs("Pilot", 16, 1.));
        assert!(!cache.needs_glyphs("Pilot", 16, 1.));
        assert!(cache.needs_glyphs("Pilot", 16, 2.));
        assert_eq!(cache.entries.len(), 1);
        cache.width(&"x".repeat(MAX_LABEL_BYTES + 1), 16., 2.);
        assert_eq!(cache.entries.len(), 1);
    }
}
