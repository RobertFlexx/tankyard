use macroquad::prelude::*;
use std::cell::Cell;

thread_local! {
    static VISION: Cell<VisionPalette> = const { Cell::new(VisionPalette::RedGreen) };
    static SYMBOLS: Cell<bool> = const { Cell::new(false) };
    static COLORBLIND: Cell<bool> = const { Cell::new(false) };
    static CONTRAST: Cell<bool> = const { Cell::new(false) };
    static REDUCED: Cell<bool> = const { Cell::new(false) };
}
/// Alternate team palettes; symbols remain available with original colors.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VisionPalette {
    BlueYellow,
    Separated,
    #[default]
    #[serde(other)]
    RedGreen,
}
impl VisionPalette {
    pub const ALL: [Self; 3] = [Self::RedGreen, Self::BlueYellow, Self::Separated];
    pub fn name(self) -> &'static str {
        match self {
            Self::RedGreen => "Red/green",
            Self::BlueYellow => "Blue/yellow",
            Self::Separated => "Separated",
        }
    }
    fn colors(self) -> [Color; 4] {
        let rgb = |r: u8, g: u8, b: u8| Color::from_rgba(r, g, b, 255);
        match self {
            Self::RedGreen => [BLUE, ORANGE, PURPLE, GREEN],
            Self::BlueYellow => [
                rgb(0, 160, 150),
                rgb(218, 76, 72),
                rgb(95, 88, 154),
                rgb(240, 174, 190),
            ],
            Self::Separated => [
                rgb(63, 126, 208),
                rgb(245, 180, 42),
                rgb(221, 113, 178),
                rgb(220, 230, 237),
            ],
        }
    }
}
pub fn configure_vision(palette: VisionPalette, symbols: bool) {
    VISION.with(|v| v.set(palette));
    SYMBOLS.with(|v| v.set(symbols));
}
pub fn team_symbols() -> bool {
    colorblind() || SYMBOLS.with(Cell::get)
}
pub fn team_palette() -> [Color; 4] {
    VISION.with(|v| v.get().colors())
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
/// Whether the system asks apps to keep motion down: Reduce motion on a
/// Mac, or "Show animations in Windows" turned off. Linux desktops have no
/// common setting, so there the game's own setting decides.
pub fn system_reduces_motion() -> bool {
    #[cfg(target_os = "macos")]
    {
        objc2_app_kit::NSWorkspace::sharedWorkspace().accessibilityDisplayShouldReduceMotion()
    }
    #[cfg(target_os = "windows")]
    {
        use windows_sys::Win32::UI::WindowsAndMessaging::{
            SPI_GETCLIENTAREAANIMATION, SystemParametersInfoW,
        };
        let mut animate: windows_sys::core::BOOL = 1;
        // SAFETY: SPI_GETCLIENTAREAANIMATION writes one BOOL.
        let read = unsafe {
            SystemParametersInfoW(SPI_GETCLIENTAREAANIMATION, 0, (&raw mut animate).cast(), 0)
        };
        read != 0 && animate == 0
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        false
    }
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
    let colors = team_palette();
    for (from, to) in [
        (crate::ui::CYAN, colors[0]),
        (crate::ui::RED, colors[1]),
        (
            crate::ui::AMBER,
            if VISION.with(Cell::get) == VisionPalette::RedGreen {
                YELLOW
            } else {
                colors[3]
            },
        ),
        (crate::ui::VIOLET, colors[2]),
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
    fn palettes_preserve_four_identities_and_symbols_work_with_original_colors() {
        for palette in VisionPalette::ALL {
            configure(true, false, false);
            configure_vision(palette, false);
            let colors = team_palette();
            for (i, color) in colors.iter().enumerate() {
                assert!(colors[..i].iter().all(|previous| previous != color));
                assert_eq!(crate::render::team_color(i as u8 + 1, false, true), *color);
            }
            let translucent = Color {
                a: 0.35,
                ..crate::ui::CYAN
            };
            let mapped = super::palette(translucent);
            assert_eq!(mapped.a, 0.35);
            assert_eq!(
                [mapped.r, mapped.g, mapped.b],
                [colors[0].r, colors[0].g, colors[0].b]
            );
            assert!(team_symbols());
        }
        configure(false, false, false);
        configure_vision(VisionPalette::Separated, true);
        assert!(team_symbols());
        assert_eq!(palette(crate::ui::CYAN), crate::ui::CYAN);
        configure_vision(VisionPalette::default(), false);
        assert!(!team_symbols());
    }
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
