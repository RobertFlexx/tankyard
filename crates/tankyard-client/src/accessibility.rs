use macroquad::prelude::*;
use std::cell::Cell;

thread_local! {
    static COLORBLIND: Cell<bool> = const { Cell::new(false) };
    static CONTRAST: Cell<bool> = const { Cell::new(false) };
    static REDUCED: Cell<bool> = const { Cell::new(false) };
}
pub const BLUE: Color = Color::new(0., 114. / 255., 178. / 255., 1.);
pub const ORANGE: Color = Color::new(230. / 255., 159. / 255., 0., 1.);
pub const PURPLE: Color = Color::new(204. / 255., 121. / 255., 167. / 255., 1.);
pub const GREEN: Color = Color::new(0., 158. / 255., 115. / 255., 1.);
pub const YELLOW: Color = Color::new(240. / 255., 228. / 255., 66. / 255., 1.);
pub const LABEL_BACK: Color = Color::new(0.025, 0.035, 0.045, 1.);
pub fn configure(colorblind: bool, contrast: bool, reduced: bool) {
    COLORBLIND.with(|c| c.set(colorblind));
    CONTRAST.with(|c| c.set(contrast));
    REDUCED.with(|c| c.set(reduced));
}
pub fn colorblind() -> bool {
    COLORBLIND.with(Cell::get)
}
pub fn contrast() -> bool {
    CONTRAST.with(Cell::get)
}
pub fn reduced_motion() -> bool {
    REDUCED.with(Cell::get)
}
pub fn motion_time(time: f32) -> f32 {
    if reduced_motion() { 0. } else { time }
}
pub fn palette(color: Color) -> Color {
    if !colorblind() {
        return color;
    }
    let rgb = [color.r, color.g, color.b];
    let mut replacement = color;
    for (from, to) in [
        (crate::ui::CYAN, BLUE),
        (crate::ui::RED, ORANGE),
        (crate::ui::AMBER, YELLOW),
        (crate::ui::VIOLET, PURPLE),
    ] {
        if rgb == [from.r, from.g, from.b] {
            replacement = to;
            break;
        }
    }
    Color {
        a: color.a,
        ..replacement
    }
}
fn luminance(c: Color) -> f32 {
    fn linear(v: f32) -> f32 {
        if v <= 0.04045 {
            v / 12.92
        } else {
            ((v + 0.055) / 1.055).powf(2.4)
        }
    }
    linear(c.r) * 0.2126 + linear(c.g) * 0.7152 + linear(c.b) * 0.0722
}
pub fn label_ink(color: Color) -> Color {
    let mut ink = color;
    while (luminance(ink) + 0.05) / (luminance(LABEL_BACK) + 0.05) < 7. {
        ink.r += (1. - ink.r) * 0.15;
        ink.g += (1. - ink.g) * 0.15;
        ink.b += (1. - ink.b) * 0.15;
    }
    ink
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn high_contrast_keeps_semantic_colors_readable_on_its_solid_backplate() {
        for c in [
            BLACK,
            WHITE,
            BLUE,
            ORANGE,
            PURPLE,
            GREEN,
            YELLOW,
            crate::ui::MUTED,
        ] {
            let ink = label_ink(c);
            assert!((luminance(ink) + 0.05) / (luminance(LABEL_BACK) + 0.05) >= 7.);
        }
    }
}
