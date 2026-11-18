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
/// Most columns or rows in a board.
pub const HUD_BOARD_SIDE: u8 = 32;
/// Most cells in a board.
pub const HUD_CELLS: usize = 128;
/// Bytes in one cell's text.
pub const HUD_CELL_TEXT: usize = 64;

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
    // Added in 1.6.0, and also left out of map files when unused.
    /// The panel's outline.
    #[serde(default)]
    pub shape: HudShape,
    /// How the panel's edge is drawn.
    #[serde(default)]
    pub border: HudBorder,
    /// How the panel is filled.
    #[serde(default)]
    pub fill: HudFill,
    /// The second color a fill mixes in: a gradient's foot, stripes, grid
    /// lines or dots. A shade of the panel when unset.
    #[serde(default)]
    pub fill_color: Option<[u8; 3]>,
    /// How the title and text stand out from what is behind them.
    #[serde(default)]
    pub lettering: HudLettering,
    /// Shows the title, text and choices in capital letters.
    #[serde(default)]
    pub caps: bool,
    /// The title's color; between the accent and the text color when unset.
    #[serde(default)]
    pub title_color: Option<[u8; 3]>,
    /// How the bar is drawn.
    #[serde(default)]
    pub bar_style: HudBar,
    /// How choices are drawn.
    #[serde(default)]
    pub buttons: HudButtons,
    /// A grid of buttons, labels, screens, lights and meters drawn under
    /// the title instead of text, a bar and choices.
    #[serde(default)]
    pub board: Option<HudBoard>,
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
            shape: HudShape::Rounded,
            border: HudBorder::Line,
            fill: HudFill::Solid,
            fill_color: None,
            lettering: HudLettering::Auto,
            caps: false,
            title_color: None,
            bar_style: HudBar::Capsule,
            buttons: HudButtons::Capsule,
            board: None,
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
    /// Whether every field is in range. A widget needs text, a title, a bar,
    /// choices or a board with cells; a board leaves out the text, bar and
    /// choices.
    pub fn valid(&self) -> bool {
        let shown = !self.text.trim().is_empty()
            || !self.title.trim().is_empty()
            || self.progress.is_some()
            || !self.choices.is_empty()
            || self
                .board
                .as_ref()
                .is_some_and(|board| !board.cells.is_empty());
        let board = self.board.as_ref().is_none_or(|board| {
            board.valid()
                && self.text.is_empty()
                && self.progress.is_none()
                && self.choices.is_empty()
        });
        shown
            && board
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
    /// Whether a player can pick from it: it has choices or board buttons.
    pub fn pickable(&self) -> bool {
        !self.choices.is_empty()
            || self.board.as_ref().is_some_and(|board| {
                board
                    .cells
                    .iter()
                    .any(|cell| cell.kind == HudCellKind::Button)
            })
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

/// A panel's outline.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HudShape {
    #[default]
    Rounded,
    /// Sharp corners.
    Square,
    /// Corners cut off at an angle.
    Chamfer,
    /// Sides leaning to the right.
    Slant,
    /// A point on the right, like a luggage tag.
    Tag,
    /// Notched ends, like a ribbon.
    Ribbon,
}
impl HudShape {
    pub const ALL: [Self; 6] = [
        Self::Rounded,
        Self::Square,
        Self::Chamfer,
        Self::Slant,
        Self::Tag,
        Self::Ribbon,
    ];
    pub fn name(self) -> &'static str {
        match self {
            Self::Rounded => "Rounded",
            Self::Square => "Square",
            Self::Chamfer => "Cut corners",
            Self::Slant => "Slanted",
            Self::Tag => "Tag",
            Self::Ribbon => "Ribbon",
        }
    }
    pub fn id(self) -> &'static str {
        match self {
            Self::Rounded => "rounded",
            Self::Square => "square",
            Self::Chamfer => "chamfer",
            Self::Slant => "slant",
            Self::Tag => "tag",
            Self::Ribbon => "ribbon",
        }
    }
}

/// How a panel's edge is drawn.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HudBorder {
    /// A thin line.
    #[default]
    Line,
    None,
    Thick,
    /// Two thin lines.
    Double,
    Dashed,
    /// Brackets at the corners only.
    Corners,
}
impl HudBorder {
    pub const ALL: [Self; 6] = [
        Self::Line,
        Self::None,
        Self::Thick,
        Self::Double,
        Self::Dashed,
        Self::Corners,
    ];
    pub fn name(self) -> &'static str {
        match self {
            Self::Line => "Line",
            Self::None => "None",
            Self::Thick => "Thick",
            Self::Double => "Double",
            Self::Dashed => "Dashed",
            Self::Corners => "Corners",
        }
    }
    pub fn id(self) -> &'static str {
        match self {
            Self::Line => "line",
            Self::None => "none",
            Self::Thick => "thick",
            Self::Double => "double",
            Self::Dashed => "dashed",
            Self::Corners => "corners",
        }
    }
}

/// How a panel is filled.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HudFill {
    #[default]
    Solid,
    /// The panel color at the top, the fill color at the bottom.
    Gradient,
    /// See-through, with a sheen across the top.
    Glass,
    /// Diagonal stripes in the fill color.
    Stripes,
    /// A fine grid in the fill color.
    Grid,
    /// Rows of dots in the fill color.
    Dots,
}
impl HudFill {
    pub const ALL: [Self; 6] = [
        Self::Solid,
        Self::Gradient,
        Self::Glass,
        Self::Stripes,
        Self::Grid,
        Self::Dots,
    ];
    pub fn name(self) -> &'static str {
        match self {
            Self::Solid => "Solid",
            Self::Gradient => "Gradient",
            Self::Glass => "Glass",
            Self::Stripes => "Stripes",
            Self::Grid => "Grid",
            Self::Dots => "Dots",
        }
    }
    pub fn id(self) -> &'static str {
        match self {
            Self::Solid => "solid",
            Self::Gradient => "gradient",
            Self::Glass => "glass",
            Self::Stripes => "stripes",
            Self::Grid => "grid",
            Self::Dots => "dots",
        }
    }
}

/// How a widget's words stand out from what is behind them.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HudLettering {
    /// A soft shadow without a panel, none on one.
    #[default]
    Auto,
    /// No shadow.
    Clean,
    /// A soft shadow.
    Shadow,
    /// A solid copy in the accent color, offset like a painted sign.
    Block,
    /// A dark outline all the way round.
    Outline,
    /// A halo in the accent color.
    Glow,
}
impl HudLettering {
    pub const ALL: [Self; 6] = [
        Self::Auto,
        Self::Clean,
        Self::Shadow,
        Self::Block,
        Self::Outline,
        Self::Glow,
    ];
    pub fn name(self) -> &'static str {
        match self {
            Self::Auto => "Auto",
            Self::Clean => "Clean",
            Self::Shadow => "Shadow",
            Self::Block => "Block",
            Self::Outline => "Outline",
            Self::Glow => "Glow",
        }
    }
    pub fn id(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Clean => "clean",
            Self::Shadow => "shadow",
            Self::Block => "block",
            Self::Outline => "outline",
            Self::Glow => "glow",
        }
    }
}

/// How a progress bar is drawn.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HudBar {
    /// Rounded ends in a dark track.
    #[default]
    Capsule,
    /// Square ends.
    Flat,
    /// A slim line.
    Thin,
    /// Stripes moving along the fill.
    Striped,
    /// A fill that glows.
    Glow,
    /// A row of dots, one per segment.
    Pips,
}
impl HudBar {
    pub const ALL: [Self; 6] = [
        Self::Capsule,
        Self::Flat,
        Self::Thin,
        Self::Striped,
        Self::Glow,
        Self::Pips,
    ];
    pub fn name(self) -> &'static str {
        match self {
            Self::Capsule => "Capsule",
            Self::Flat => "Flat",
            Self::Thin => "Thin",
            Self::Striped => "Striped",
            Self::Glow => "Glow",
            Self::Pips => "Pips",
        }
    }
    pub fn id(self) -> &'static str {
        match self {
            Self::Capsule => "capsule",
            Self::Flat => "flat",
            Self::Thin => "thin",
            Self::Striped => "striped",
            Self::Glow => "glow",
            Self::Pips => "pips",
        }
    }
}

/// How a widget's choices are drawn.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HudButtons {
    /// Filled, with round ends.
    #[default]
    Capsule,
    /// Filled, with the panel's corners.
    Box,
    /// An outline that fills when pointed at.
    Outline,
    /// Words over a line that thickens when pointed at.
    Underline,
}
impl HudButtons {
    pub const ALL: [Self; 4] = [Self::Capsule, Self::Box, Self::Outline, Self::Underline];
    pub fn name(self) -> &'static str {
        match self {
            Self::Capsule => "Capsule",
            Self::Box => "Box",
            Self::Outline => "Outline",
            Self::Underline => "Underline",
        }
    }
    pub fn id(self) -> &'static str {
        match self {
            Self::Capsule => "capsule",
            Self::Box => "box",
            Self::Outline => "outline",
            Self::Underline => "underline",
        }
    }
}

/// Ready-made combinations of every style field, for the workshop's Looks
/// buttons. A look sets colors, outline, border, fill, lettering, bar and
/// buttons together and leaves the words, place, bar amount and choices.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HudLook {
    /// Back to the player's own theme.
    Theme,
    Neon,
    Arcade,
    Parchment,
    Terminal,
    Royal,
    Glass,
    Hazard,
    Sticker,
    Frost,
    Ember,
    Jungle,
}
impl HudLook {
    pub const ALL: [Self; 12] = [
        Self::Theme,
        Self::Neon,
        Self::Arcade,
        Self::Parchment,
        Self::Terminal,
        Self::Royal,
        Self::Glass,
        Self::Hazard,
        Self::Sticker,
        Self::Frost,
        Self::Ember,
        Self::Jungle,
    ];
    pub fn name(self) -> &'static str {
        match self {
            Self::Theme => "Theme",
            Self::Neon => "Neon",
            Self::Arcade => "Arcade",
            Self::Parchment => "Parchment",
            Self::Terminal => "Terminal",
            Self::Royal => "Royal",
            Self::Glass => "Glass",
            Self::Hazard => "Hazard",
            Self::Sticker => "Sticker",
            Self::Frost => "Frost",
            Self::Ember => "Ember",
            Self::Jungle => "Jungle",
        }
    }
    /// Restyle `widget`. A plain widget gets a panel, since every look but
    /// Theme is drawn on one.
    pub fn apply(self, widget: &mut HudWidget) {
        use {HudBar as Bar, HudBorder as Edge, HudButtons as Buttons};
        use {HudFill as Fill, HudLettering as Letters, HudShape as Shape};
        type Rgb = Option<[u8; 3]>;
        #[allow(clippy::type_complexity)]
        let (panel, accent, text, title, second): (Rgb, Rgb, Rgb, Rgb, Rgb) = match self {
            Self::Theme => (None, None, None, None, None),
            Self::Neon => (
                Some([14, 8, 30]),
                Some([255, 60, 200]),
                Some([240, 240, 255]),
                Some([80, 240, 255]),
                Some([70, 30, 120]),
            ),
            Self::Arcade => (
                Some([30, 20, 72]),
                Some([255, 210, 40]),
                Some([255, 255, 255]),
                Some([255, 96, 96]),
                Some([70, 52, 140]),
            ),
            Self::Parchment => (
                Some([238, 224, 192]),
                Some([140, 90, 40]),
                Some([70, 45, 20]),
                Some([150, 60, 30]),
                Some([206, 178, 128]),
            ),
            Self::Terminal => (
                Some([6, 14, 8]),
                Some([60, 255, 120]),
                Some([130, 255, 160]),
                Some([60, 255, 120]),
                Some([24, 70, 36]),
            ),
            Self::Royal => (
                Some([44, 16, 74]),
                Some([236, 190, 80]),
                Some([250, 240, 220]),
                Some([236, 190, 80]),
                Some([14, 4, 30]),
            ),
            Self::Glass => (
                Some([200, 220, 255]),
                Some([255, 255, 255]),
                Some([255, 255, 255]),
                Some([214, 232, 255]),
                None,
            ),
            Self::Hazard => (
                Some([24, 24, 24]),
                Some([255, 200, 0]),
                Some([255, 222, 50]),
                Some([255, 255, 255]),
                Some([255, 200, 0]),
            ),
            Self::Sticker => (
                Some([255, 255, 255]),
                Some([24, 22, 36]),
                Some([24, 22, 36]),
                Some([230, 40, 90]),
                None,
            ),
            Self::Frost => (
                Some([214, 240, 255]),
                Some([80, 170, 255]),
                Some([20, 60, 100]),
                Some([40, 120, 210]),
                Some([150, 208, 250]),
            ),
            Self::Ember => (
                Some([44, 10, 6]),
                Some([255, 120, 30]),
                Some([255, 232, 204]),
                Some([255, 172, 64]),
                Some([120, 26, 10]),
            ),
            Self::Jungle => (
                Some([22, 52, 28]),
                Some([156, 222, 84]),
                Some([236, 246, 222]),
                Some([204, 242, 124]),
                Some([44, 96, 44]),
            ),
        };
        let (shape, border, fill, lettering, caps, glow, opacity, bar, buttons) = match self {
            Self::Theme => (
                Shape::Rounded,
                Edge::Line,
                Fill::Solid,
                Letters::Auto,
                false,
                false,
                0.85,
                Bar::Capsule,
                Buttons::Capsule,
            ),
            Self::Neon => (
                Shape::Chamfer,
                Edge::Line,
                Fill::Grid,
                Letters::Glow,
                true,
                true,
                0.9,
                Bar::Glow,
                Buttons::Outline,
            ),
            Self::Arcade => (
                Shape::Square,
                Edge::Thick,
                Fill::Dots,
                Letters::Block,
                true,
                false,
                0.95,
                Bar::Pips,
                Buttons::Box,
            ),
            Self::Parchment => (
                Shape::Ribbon,
                Edge::Double,
                Fill::Gradient,
                Letters::Clean,
                false,
                false,
                0.97,
                Bar::Flat,
                Buttons::Underline,
            ),
            Self::Terminal => (
                Shape::Square,
                Edge::Dashed,
                Fill::Grid,
                Letters::Glow,
                true,
                false,
                0.92,
                Bar::Flat,
                Buttons::Outline,
            ),
            Self::Royal => (
                Shape::Chamfer,
                Edge::Double,
                Fill::Gradient,
                Letters::Shadow,
                true,
                false,
                0.95,
                Bar::Capsule,
                Buttons::Box,
            ),
            Self::Glass => (
                Shape::Rounded,
                Edge::Line,
                Fill::Glass,
                Letters::Shadow,
                false,
                false,
                0.5,
                Bar::Thin,
                Buttons::Outline,
            ),
            Self::Hazard => (
                Shape::Slant,
                Edge::Thick,
                Fill::Stripes,
                Letters::Outline,
                true,
                false,
                0.95,
                Bar::Striped,
                Buttons::Box,
            ),
            Self::Sticker => (
                Shape::Tag,
                Edge::Thick,
                Fill::Solid,
                Letters::Clean,
                false,
                false,
                1.,
                Bar::Capsule,
                Buttons::Capsule,
            ),
            Self::Frost => (
                Shape::Rounded,
                Edge::Line,
                Fill::Gradient,
                Letters::Clean,
                false,
                true,
                0.9,
                Bar::Glow,
                Buttons::Capsule,
            ),
            Self::Ember => (
                Shape::Chamfer,
                Edge::Corners,
                Fill::Gradient,
                Letters::Glow,
                true,
                true,
                0.92,
                Bar::Glow,
                Buttons::Underline,
            ),
            Self::Jungle => (
                Shape::Rounded,
                Edge::Double,
                Fill::Dots,
                Letters::Block,
                false,
                false,
                0.93,
                Bar::Pips,
                Buttons::Capsule,
            ),
        };
        if self != Self::Theme && widget.style == HudStyle::Plain {
            widget.style = HudStyle::Panel;
        }
        widget.panel = panel;
        widget.accent = accent;
        widget.color = text;
        widget.title_color = title;
        widget.fill_color = second;
        widget.bar_color = None;
        widget.shape = shape;
        widget.border = border;
        widget.fill = fill;
        widget.lettering = lettering;
        widget.caps = caps;
        widget.glow = glow;
        widget.opacity = opacity;
        widget.bar_style = bar;
        widget.buttons = buttons;
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

/// A widget's grid of cells: a control panel, keypad or computer front.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HudBoard {
    /// Columns in the grid, from 1 to [`HUD_BOARD_SIDE`].
    #[serde(default = "HudBoard::default_side")]
    pub columns: u8,
    /// Rows in the grid, from 1 to [`HUD_BOARD_SIDE`].
    #[serde(default = "HudBoard::default_side")]
    pub rows: u8,
    /// Each grid square's side, in points at the default interface scale.
    #[serde(default = "HudBoard::default_cell")]
    pub cell: f32,
    /// Points between neighboring squares.
    #[serde(default = "HudBoard::default_gap")]
    pub gap: f32,
    /// What sits on the grid, drawn in order: later cells cover earlier
    /// ones and take the click where they overlap.
    #[serde(default)]
    pub cells: Vec<HudCell>,
}
impl Default for HudBoard {
    fn default() -> Self {
        Self {
            columns: Self::default_side(),
            rows: Self::default_side(),
            cell: Self::default_cell(),
            gap: Self::default_gap(),
            cells: Vec::new(),
        }
    }
}
impl HudBoard {
    pub const CELL_RANGE: [f32; 2] = [12., 96.];
    pub const GAP_RANGE: [f32; 2] = [0., 24.];
    fn default_side() -> u8 {
        4
    }
    fn default_cell() -> f32 {
        40.
    }
    fn default_gap() -> f32 {
        4.
    }
    /// Whether the grid's size is in range and every cell fits on it.
    pub fn valid(&self) -> bool {
        (1..=HUD_BOARD_SIDE).contains(&self.columns)
            && (1..=HUD_BOARD_SIDE).contains(&self.rows)
            && range(self.cell, Self::CELL_RANGE[0], Self::CELL_RANGE[1])
            && range(self.gap, Self::GAP_RANGE[0], Self::GAP_RANGE[1])
            && self.cells.len() <= HUD_CELLS
            && self
                .cells
                .iter()
                .all(|cell| cell.valid() && cell.fits(self.columns, self.rows))
    }
    /// The topmost cell covering grid square `column`, `row`.
    pub fn cell_at(&self, column: u8, row: u8) -> Option<usize> {
        self.cells.iter().rposition(|cell| cell.covers(column, row))
    }
    /// Squares from the board's top-left corner to `at`'s corner, and the
    /// size of `span` squares, counting the gaps they cover: in grid units
    /// where a square is `cell` and a gap `gap` wide.
    pub fn place(&self, at: [u8; 2], span: [u8; 2]) -> [f32; 4] {
        let step = self.cell + self.gap;
        let size = |n: u8| f32::from(n) * step - self.gap;
        [
            f32::from(at[0]) * step,
            f32::from(at[1]) * step,
            size(span[0].max(1)),
            size(span[1].max(1)),
        ]
    }
    /// The grid's width and height, in the same units as [`HudBoard::place`].
    pub fn extent(&self) -> [f32; 2] {
        let [_, _, w, h] = self.place([0, 0], [self.columns, self.rows]);
        [w, h]
    }
}

/// What a board cell is.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HudCellKind {
    /// A line of text.
    #[default]
    Label,
    /// A button that signals its channel when picked, lit while its
    /// counter or group says so.
    Button,
    /// Text on a dark screen.
    Screen,
    /// Seven-segment digits on a dark screen, for numbers and short words.
    Digits,
    /// A lamp, lit while its counter or group says so, with a caption.
    Light,
    /// A bar that fills as its counter climbs toward `max`.
    Meter,
}
impl HudCellKind {
    pub const ALL: [Self; 6] = [
        Self::Label,
        Self::Button,
        Self::Screen,
        Self::Digits,
        Self::Light,
        Self::Meter,
    ];
    pub fn name(self) -> &'static str {
        match self {
            Self::Label => "Label",
            Self::Button => "Button",
            Self::Screen => "Screen",
            Self::Digits => "Digits",
            Self::Light => "Light",
            Self::Meter => "Meter",
        }
    }
    pub fn id(self) -> &'static str {
        match self {
            Self::Label => "label",
            Self::Button => "button",
            Self::Screen => "screen",
            Self::Digits => "digits",
            Self::Light => "light",
            Self::Meter => "meter",
        }
    }
    /// Whether a counter or group can light it.
    pub fn lights(self) -> bool {
        matches!(self, Self::Button | Self::Light)
    }
    /// Whether it sits on a dark screen.
    pub fn screen(self) -> bool {
        matches!(self, Self::Screen | Self::Digits)
    }
}

/// One cell on a board.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HudCell {
    #[serde(default)]
    pub kind: HudCellKind,
    /// The column and row of its top-left square, from 0.
    #[serde(default)]
    pub at: [u8; 2],
    /// How many columns and rows it covers, from 1.
    #[serde(default = "HudCell::one")]
    pub span: [u8; 2],
    /// What it shows: a label's, button's or screen's words, the digits,
    /// a light's caption or a meter's. Fills `{…}` fields as widget text
    /// does.
    #[serde(default)]
    pub text: String,
    /// The channel a button signals when picked; 0 signals none.
    #[serde(default)]
    pub channel: u8,
    /// The counter that lights a light or button, or fills a meter, from
    /// 1; 0 for none.
    #[serde(default)]
    pub counter: u8,
    /// Which bit of `counter` lights it, from 1 for the lowest; 0 lights it
    /// whenever the counter isn't 0.
    #[serde(default)]
    pub bit: u8,
    /// A group that lights it while shown, from 1; 0 for none. Used instead
    /// of `counter` when both are set.
    #[serde(default)]
    pub group: u8,
    /// The counter value that fills a meter.
    #[serde(default = "HudCell::default_max")]
    pub max: i32,
    /// Its text, segments, lamp or bar color; the widget's when unset.
    #[serde(default)]
    pub color: Option<[u8; 3]>,
    /// Its background color; the widget's look decides when unset.
    #[serde(default)]
    pub fill: Option<[u8; 3]>,
    /// Text size against what fits the cell, from 0.25 to 2; 0 fits it.
    #[serde(default)]
    pub size: f32,
    #[serde(default)]
    pub align: HudAlign,
    /// Whether a light or button is on. Servers work it out from `counter`
    /// or `group` for map widgets; scripts set it.
    #[serde(default)]
    pub lit: bool,
    /// How full a meter is, from 0 to 1. Servers work it out from `counter`
    /// and `max` for map widgets; scripts set it.
    #[serde(default)]
    pub progress: f32,
}
impl Default for HudCell {
    fn default() -> Self {
        Self {
            kind: HudCellKind::Label,
            at: [0, 0],
            span: Self::one(),
            text: String::new(),
            channel: 0,
            counter: 0,
            bit: 0,
            group: 0,
            max: Self::default_max(),
            color: None,
            fill: None,
            size: 0.,
            align: HudAlign::Center,
            lit: false,
            progress: 0.,
        }
    }
}
impl HudCell {
    pub const SIZE_RANGE: [f32; 2] = [0.25, 2.];
    /// The highest bit a cell can watch.
    pub const BITS: u8 = 30;
    fn one() -> [u8; 2] {
        [1, 1]
    }
    fn default_max() -> i32 {
        100
    }
    /// Whether its own fields are in range, wherever it sits.
    pub fn valid(&self) -> bool {
        self.span[0] >= 1
            && self.span[1] >= 1
            && plain_text(&self.text, HUD_CELL_TEXT)
            && self.bit <= Self::BITS
            && self.group <= crate::MAP_GROUPS
            && (1..=crate::COUNTER_LIMIT).contains(&self.max)
            && (self.size == 0. || range(self.size, Self::SIZE_RANGE[0], Self::SIZE_RANGE[1]))
            && range(self.progress, 0., 1.)
    }
    /// Whether it lies within a grid of `columns` by `rows` squares.
    pub fn fits(&self, columns: u8, rows: u8) -> bool {
        u16::from(self.at[0]) + u16::from(self.span[0]) <= u16::from(columns)
            && u16::from(self.at[1]) + u16::from(self.span[1]) <= u16::from(rows)
    }
    /// Whether it covers grid square `column`, `row`.
    pub fn covers(&self, column: u8, row: u8) -> bool {
        (self.at[0]..self.at[0].saturating_add(self.span[0])).contains(&column)
            && (self.at[1]..self.at[1].saturating_add(self.span[1])).contains(&row)
    }
    /// Whether a counter or group decides if it is lit.
    pub fn watches(&self) -> bool {
        self.kind.lights() && (self.group > 0 || self.counter > 0)
    }
    /// Whether it is lit with `counters` (counter 1 first) and `hidden`
    /// groups (group 1 is bit 0).
    pub fn lit_by(&self, counters: &[i32], hidden: u32) -> bool {
        if self.group > 0 {
            return hidden & crate::MapDef::group_bit(self.group) == 0;
        }
        let Some(&value) = usize::from(self.counter)
            .checked_sub(1)
            .and_then(|slot| counters.get(slot))
        else {
            return false;
        };
        match self.bit {
            0 => value != 0,
            bit => (value >> (bit - 1)) & 1 == 1,
        }
    }
    /// How full a meter is with `counters`, from 0 to 1.
    pub fn fill_with(&self, counters: &[i32]) -> f32 {
        let value = usize::from(self.counter)
            .checked_sub(1)
            .and_then(|slot| counters.get(slot))
            .copied()
            .unwrap_or(0);
        (value as f32 / self.max.max(1) as f32).clamp(0., 1.)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn looks_restyle_without_touching_words_place_or_choices() {
        let base = HudWidget {
            text: "Gate opens".into(),
            title: "Vault".into(),
            x: 0.3,
            y: 0.7,
            progress: Some(0.4),
            choices: vec!["Go".into()],
            ..HudWidget::default()
        };
        let mut seen = Vec::new();
        for look in HudLook::ALL {
            let mut widget = base.clone();
            look.apply(&mut widget);
            assert!(widget.valid(), "{look:?}");
            assert_eq!(
                (
                    &widget.text,
                    &widget.title,
                    widget.x,
                    widget.y,
                    widget.progress,
                    &widget.choices
                ),
                (
                    &base.text,
                    &base.title,
                    base.x,
                    base.y,
                    base.progress,
                    &base.choices
                )
            );
            // Every look differs from every other in several ways.
            for (other, styled) in &seen {
                let styled: &HudWidget = styled;
                let differences = [
                    widget.panel != styled.panel,
                    widget.accent != styled.accent,
                    widget.shape != styled.shape,
                    widget.border != styled.border,
                    widget.fill != styled.fill,
                    widget.lettering != styled.lettering,
                    widget.bar_style != styled.bar_style,
                    widget.buttons != styled.buttons,
                ]
                .into_iter()
                .filter(|&d| d)
                .count();
                assert!(differences >= 3, "{look:?} and {other:?}");
            }
            seen.push((look, widget));
        }
        // Theme undoes a look.
        let mut widget = base.clone();
        HudLook::Hazard.apply(&mut widget);
        HudLook::Theme.apply(&mut widget);
        assert_eq!(
            widget,
            HudWidget {
                style: HudStyle::Panel,
                ..base.clone()
            }
        );
    }

    #[test]
    fn map_files_leave_out_unused_styles_and_read_them_back() {
        let mut widget = HudWidget {
            text: "Hi".into(),
            ..HudWidget::default()
        };
        let plain = serde_json::to_value(crate::MapUi {
            widget: widget.clone(),
            ..crate::MapUi::default()
        })
        .unwrap();
        for key in [
            "shape",
            "border",
            "fill",
            "lettering",
            "caps",
            "bar_style",
            "buttons",
        ] {
            assert!(plain["widget"].get(key).is_none(), "{key} is left out");
        }
        HudLook::Neon.apply(&mut widget);
        let ui = crate::MapUi {
            widget: widget.clone(),
            ..crate::MapUi::default()
        };
        let back: crate::MapUi =
            serde_json::from_value(serde_json::to_value(&ui).unwrap()).unwrap();
        assert_eq!(back.widget, widget);
    }

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
