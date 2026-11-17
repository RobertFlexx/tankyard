//! On-screen widgets that scripts and map triggers show players: a line of
//! text, a bar, or a small panel with a title, an icon and choices, on the
//! screen or floating over a spot in the arena or a tank. The server
//! decides what each player sees and clients only draw it.
use serde::{Deserialize, Serialize};

/// Choices a widget offers at most.
pub const HUD_CHOICES: usize = 6;
/// Bytes in a widget's text.
pub const HUD_TEXT: usize = 120;
/// Bytes in a widget's title.
pub const HUD_TITLE: usize = 48;
/// Bytes in one choice.
pub const HUD_CHOICE: usize = 32;
/// The most segments a bar splits into.
pub const HUD_SEGMENTS: u8 = 20;
/// How far from the arena's origin a widget may be pinned, in world units.
pub const HUD_REACH: f32 = 100_000.;

/// A line of text, a progress bar or both, drawn on players' screens,
/// optionally on a panel and with choices a player can pick.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HudWidget {
    #[serde(default)]
    pub text: String,
    /// Position as a fraction of the screen's width and height.
    #[serde(default = "HudWidget::default_x")]
    pub x: f32,
    #[serde(default = "HudWidget::default_y")]
    pub y: f32,
    #[serde(default)]
    pub align: HudAlign,
    /// Text size in points at the default interface scale.
    #[serde(default = "HudWidget::default_size")]
    pub size: f32,
    /// Text color; the theme's text color when unset.
    #[serde(default)]
    pub color: Option<[u8; 3]>,
    /// A bar below the text, filled from 0 to 1.
    #[serde(default)]
    pub progress: Option<f32>,
    #[serde(default)]
    pub bar_color: Option<[u8; 3]>,
    /// The bar's width as a fraction of the screen's width.
    #[serde(default = "HudWidget::default_width")]
    pub width: f32,
    // Added in 1.4.0.
    /// What the widget sits on.
    #[serde(default)]
    pub style: HudStyle,
    /// A smaller line above the text.
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub icon: HudIcon,
    /// The panel's color; the theme's panel when unset.
    #[serde(default)]
    pub panel: Option<[u8; 3]>,
    /// How solid the panel is, from 0 to 1.
    #[serde(default = "HudWidget::default_opacity")]
    pub opacity: f32,
    /// The border, stripe and highlighted choice's color; the bar's color,
    /// or the theme's accent, when unset.
    #[serde(default)]
    pub accent: Option<[u8; 3]>,
    /// How the widget appears.
    #[serde(default)]
    pub enter: HudEnter,
    /// Whether it gently pulses to draw the eye.
    #[serde(default)]
    pub pulse: bool,
    /// Splits the bar into this many pieces, such as lives; 0 or 1 keeps
    /// it whole.
    #[serde(default)]
    pub segments: u8,
    /// Buttons a player can pick, up to [`HUD_CHOICES`].
    #[serde(default)]
    pub choices: Vec<String>,
    /// Which menu a pick answers. The server sets it.
    #[serde(default)]
    pub menu: u32,
    // Added in 1.5.0. Map files leave these out when unused (see
    // `MapUi`), but the wire format always carries them.
    /// Where the widget sits: on the screen at `x` and `y`, or floating
    /// over a spot in the arena or a tank as the view moves.
    #[serde(default)]
    pub pin: HudPin,
    /// Whether a pinned widget waits at the edge of the screen, pointing
    /// the way, while what it is pinned to is out of view. Otherwise it
    /// shows only while that is in view.
    #[serde(default)]
    pub point: bool,
    /// How choices are laid out.
    #[serde(default)]
    pub layout: HudLayout,
    /// A soft glow in the accent color around the widget.
    #[serde(default)]
    pub glow: bool,
}
impl Default for HudWidget {
    fn default() -> Self {
        Self {
            text: String::new(),
            x: Self::default_x(),
            y: Self::default_y(),
            align: HudAlign::Center,
            size: Self::default_size(),
            color: None,
            progress: None,
            bar_color: None,
            width: Self::default_width(),
            style: HudStyle::Plain,
            title: String::new(),
            icon: HudIcon::None,
            panel: None,
            opacity: Self::default_opacity(),
            accent: None,
            enter: HudEnter::Fade,
            pulse: false,
            segments: 0,
            choices: Vec::new(),
            menu: 0,
            pin: HudPin::Screen,
            point: false,
            layout: HudLayout::Auto,
            glow: false,
        }
    }
}
impl HudWidget {
    fn default_x() -> f32 {
        0.5
    }
    fn default_y() -> f32 {
        0.2
    }
    fn default_size() -> f32 {
        18.
    }
    fn default_width() -> f32 {
        0.2
    }
    fn default_opacity() -> f32 {
        0.85
    }
    /// Whether every field is in range. A widget needs text, a title, a bar
    /// or choices.
    pub fn valid(&self) -> bool {
        let shown = !self.text.trim().is_empty()
            || !self.title.trim().is_empty()
            || self.progress.is_some()
            || !self.choices.is_empty();
        shown
            && plain_text(&self.text, HUD_TEXT)
            && plain_text(&self.title, HUD_TITLE)
            && range(self.x, 0., 1.)
            && range(self.y, 0., 1.)
            && range(self.size, 10., 48.)
            && self.progress.is_none_or(|p| range(p, 0., 1.))
            && range(self.width, 0.02, 1.)
            && range(self.opacity, 0., 1.)
            && self.segments <= HUD_SEGMENTS
            && self.choices.len() <= HUD_CHOICES
            && self
                .choices
                .iter()
                .all(|choice| !choice.trim().is_empty() && plain_text(choice, HUD_CHOICE))
            && self.pin.valid()
    }
    /// Whether the widget floats over something in the arena.
    pub fn pinned(&self) -> bool {
        self.pin != HudPin::Screen
    }
}
fn plain_text(text: &str, max: usize) -> bool {
    text.len() <= max && !text.chars().any(char::is_control)
}
fn range(value: f32, low: f32, high: f32) -> bool {
    value.is_finite() && (low..=high).contains(&value)
}
/// `text` without control characters, cut to at most `max` bytes on a
/// character boundary.
pub fn clip_text(text: &str, max: usize) -> String {
    let mut out = String::new();
    for c in text.chars().filter(|c| !c.is_control()) {
        if out.len() + c.len_utf8() > max {
            break;
        }
        out.push(c);
    }
    out
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HudAlign {
    Left,
    #[default]
    Center,
    Right,
}
impl HudAlign {
    pub const ALL: [Self; 3] = [Self::Left, Self::Center, Self::Right];
    pub fn name(self) -> &'static str {
        match self {
            Self::Left => "Left",
            Self::Center => "Center",
            Self::Right => "Right",
        }
    }
    pub fn id(self) -> &'static str {
        match self {
            Self::Left => "left",
            Self::Center => "center",
            Self::Right => "right",
        }
    }
}

/// What a widget sits on.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HudStyle {
    /// Text with a soft shadow, straight on the game.
    #[default]
    Plain,
    /// A rounded panel with a thin border.
    Panel,
    /// A panel with a colored stripe down its side.
    Card,
    /// A strip across the whole screen.
    Banner,
    /// A capsule, for short labels and counters.
    Pill,
}
impl HudStyle {
    pub const ALL: [Self; 5] = [
        Self::Plain,
        Self::Panel,
        Self::Card,
        Self::Banner,
        Self::Pill,
    ];
    pub fn name(self) -> &'static str {
        match self {
            Self::Plain => "Plain",
            Self::Panel => "Panel",
            Self::Card => "Card",
            Self::Banner => "Banner",
            Self::Pill => "Pill",
        }
    }
    pub fn id(self) -> &'static str {
        match self {
            Self::Plain => "plain",
            Self::Panel => "panel",
            Self::Card => "card",
            Self::Banner => "banner",
            Self::Pill => "pill",
        }
    }
}

/// A small symbol before a widget's text.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HudIcon {
    #[default]
    None,
    Star,
    Heart,
    Coin,
    Clock,
    Flag,
    Bolt,
    Shield,
    Key,
    Gem,
    Trophy,
    Target,
    Info,
    Alert,
    // Added in 1.5.0.
    Crown,
    Skull,
    Lock,
    Check,
    Cross,
    Fire,
    Bomb,
}
impl HudIcon {
    pub const ALL: [Self; 21] = [
        Self::None,
        Self::Star,
        Self::Heart,
        Self::Coin,
        Self::Clock,
        Self::Flag,
        Self::Bolt,
        Self::Shield,
        Self::Key,
        Self::Gem,
        Self::Trophy,
        Self::Target,
        Self::Info,
        Self::Alert,
        Self::Crown,
        Self::Skull,
        Self::Lock,
        Self::Check,
        Self::Cross,
        Self::Fire,
        Self::Bomb,
    ];
    pub fn name(self) -> &'static str {
        match self {
            Self::None => "None",
            Self::Star => "Star",
            Self::Heart => "Heart",
            Self::Coin => "Coin",
            Self::Clock => "Clock",
            Self::Flag => "Flag",
            Self::Bolt => "Bolt",
            Self::Shield => "Shield",
            Self::Key => "Key",
            Self::Gem => "Gem",
            Self::Trophy => "Trophy",
            Self::Target => "Target",
            Self::Info => "Info",
            Self::Alert => "Alert",
            Self::Crown => "Crown",
            Self::Skull => "Skull",
            Self::Lock => "Lock",
            Self::Check => "Check",
            Self::Cross => "Cross",
            Self::Fire => "Fire",
            Self::Bomb => "Bomb",
        }
    }
    pub fn id(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Star => "star",
            Self::Heart => "heart",
            Self::Coin => "coin",
            Self::Clock => "clock",
            Self::Flag => "flag",
            Self::Bolt => "bolt",
            Self::Shield => "shield",
            Self::Key => "key",
            Self::Gem => "gem",
            Self::Trophy => "trophy",
            Self::Target => "target",
            Self::Info => "info",
            Self::Alert => "alert",
            Self::Crown => "crown",
            Self::Skull => "skull",
            Self::Lock => "lock",
            Self::Check => "check",
            Self::Cross => "cross",
            Self::Fire => "fire",
            Self::Bomb => "bomb",
        }
    }
}

/// How a widget appears when it is first shown.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HudEnter {
    /// At once.
    None,
    #[default]
    Fade,
    /// Grows past its size and settles.
    Pop,
    /// Slides in from the nearer side.
    Slide,
    /// Drops in from above.
    Drop,
}
impl HudEnter {
    pub const ALL: [Self; 5] = [Self::None, Self::Fade, Self::Pop, Self::Slide, Self::Drop];
    pub fn name(self) -> &'static str {
        match self {
            Self::None => "None",
            Self::Fade => "Fade",
            Self::Pop => "Pop",
            Self::Slide => "Slide",
            Self::Drop => "Drop",
        }
    }
    pub fn id(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Fade => "fade",
            Self::Pop => "pop",
            Self::Slide => "slide",
            Self::Drop => "drop",
        }
    }
}

/// What a widget floats over.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HudPin {
    /// A fixed place on the screen, at the widget's `x` and `y`.
    #[default]
    Screen,
    /// A spot in the arena, in world units: centered on it.
    World([f32; 2]),
    /// A tank, by its id: just above it, following it around. The widget
    /// hides while the tank is dead or hidden from the viewer.
    Tank(u64),
    /// A map point before motion, carried by group `id` with its cover.
    Group { id: u8, at: [f32; 2] },
}
impl HudPin {
    fn valid(self) -> bool {
        match self {
            Self::Screen => true,
            Self::World(at) => at.iter().all(|v| range(*v, -HUD_REACH, HUD_REACH)),
            Self::Tank(id) => id != 0,
            Self::Group { id, at } => {
                (1..=crate::MAP_GROUPS).contains(&id)
                    && at.iter().all(|v| range(*v, -HUD_REACH, HUD_REACH))
            }
        }
    }
}

/// How a widget's choices are arranged.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HudLayout {
    /// In a row, or one under another when the row does not fit.
    #[default]
    Auto,
    /// One under another, all as wide as the widest.
    Column,
    /// Two to a row, all the same size.
    Grid,
}
impl HudLayout {
    pub const ALL: [Self; 3] = [Self::Auto, Self::Column, Self::Grid];
    pub fn name(self) -> &'static str {
        match self {
            Self::Auto => "Row",
            Self::Column => "Column",
            Self::Grid => "Grid",
        }
    }
    pub fn id(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Column => "column",
            Self::Grid => "grid",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn widgets_need_something_to_show_and_fields_in_range() {
        let mut widget = HudWidget::default();
        assert!(!widget.valid(), "an empty widget shows nothing");
        widget.choices = vec!["Left".into(), "Right".into()];
        assert!(widget.valid(), "choices alone are a menu");
        widget.choices.push(String::new());
        assert!(!widget.valid(), "an empty choice");
        widget.choices = vec!["x".repeat(HUD_CHOICE + 1)];
        assert!(!widget.valid(), "a long choice");
        widget.choices = vec!["Go".into(); HUD_CHOICES + 1];
        assert!(!widget.valid(), "too many choices");
        widget.choices = vec!["Go".into(); HUD_CHOICES];
        assert!(widget.valid(), "six choices");
        widget = HudWidget {
            title: "Shop".into(),
            ..HudWidget::default()
        };
        assert!(widget.valid());
        for broken in [
            HudWidget {
                opacity: 1.5,
                ..widget.clone()
            },
            HudWidget {
                segments: HUD_SEGMENTS + 1,
                ..widget.clone()
            },
            HudWidget {
                text: "line\nbreak".into(),
                ..widget.clone()
            },
            HudWidget {
                x: f32::NAN,
                ..widget.clone()
            },
            HudWidget {
                pin: HudPin::World([f32::INFINITY, 0.]),
                ..widget.clone()
            },
            HudWidget {
                pin: HudPin::Tank(0),
                ..widget.clone()
            },
            HudWidget {
                pin: HudPin::Group {
                    id: 33,
                    at: [0., 0.],
                },
                ..widget.clone()
            },
            HudWidget {
                pin: HudPin::Group {
                    id: 1,
                    at: [f32::NAN, 0.],
                },
                ..widget.clone()
            },
        ] {
            assert!(!broken.valid(), "{broken:?}");
        }
    }

    #[test]
    fn clipped_text_stays_on_character_boundaries() {
        assert_eq!(clip_text("héllo\tthere", 3), "hé");
        assert_eq!(clip_text("plain", 120), "plain");
    }

    #[test]
    fn older_widget_fields_still_read() {
        let widget: HudWidget =
            serde_json::from_str(r#"{ "text": "Wave 3", "y": 0.1, "align": "left" }"#)
                .expect("reads");
        assert_eq!(widget.style, HudStyle::Plain);
        assert_eq!(widget.enter, HudEnter::Fade);
        assert!((widget.opacity - 0.85).abs() < 1e-6);
        assert!(widget.valid());
        assert_eq!(widget.pin, HudPin::Screen);
        assert_eq!(widget.layout, HudLayout::Auto);
    }

    #[test]
    fn pins_read_as_a_spot_or_a_tank() {
        let widget: HudWidget =
            serde_json::from_str(r#"{ "text": "Shop", "pin": { "world": [640, 480] } }"#)
                .expect("reads");
        assert_eq!(widget.pin, HudPin::World([640., 480.]));
        assert!(widget.pinned() && widget.valid());
        let widget: HudWidget =
            serde_json::from_str(r#"{ "text": "It", "pin": { "tank": 7 }, "point": true }"#)
                .expect("reads");
        assert_eq!(widget.pin, HudPin::Tank(7));
        assert!(widget.point);
    }
}
