//! How a pack dresses the game's own interface: the main menu, the interface
//! colors and its wording. Like a resource pack, it applies only on the
//! player's machine; it never travels to servers or other players.
//!
//! ```json
//! "menu": {
//!   "title": "OVERCLOCK",
//!   "subtitle": "The rules change every 40 seconds",
//!   "splashes": ["Now with 8 protocols!", "Bring a shield"],
//!   "showcase": "vector",
//!   "background": "art/menu_tile.svg",
//!   "ambient": false
//! },
//! "theme": { "accent": "#b14cff", "background": "#120a1f", "text": "#f2eaff" },
//! "text": { "Play offline": "Solo run" }
//! ```
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::collections::BTreeMap;

pub const MAX_SPLASHES: usize = 64;
pub const MAX_TEXT_OVERRIDES: usize = 256;

/// An sRGB color, written `"#rgb"`, `"#rrggbb"` or `[r, g, b]`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rgb(pub [u8; 3]);

impl Rgb {
    pub fn parse(text: &str) -> Option<Self> {
        let hex = text.trim().strip_prefix('#')?;
        if !hex.bytes().all(|b| b.is_ascii_hexdigit()) {
            return None;
        }
        let digit = |i: usize| u8::from_str_radix(&hex[i..=i], 16).ok();
        match hex.len() {
            3 => Some(Self([digit(0)? * 17, digit(1)? * 17, digit(2)? * 17])),
            6 => {
                let byte = |i: usize| u8::from_str_radix(&hex[i..i + 2], 16).ok();
                Some(Self([byte(0)?, byte(2)?, byte(4)?]))
            }
            _ => None,
        }
    }
    pub fn to_f32(self) -> [f32; 3] {
        self.0.map(|c| f32::from(c) / 255.0)
    }
}

impl Serialize for Rgb {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let [r, g, b] = self.0;
        serializer.serialize_str(&format!("#{r:02x}{g:02x}{b:02x}"))
    }
}

impl<'de> Deserialize<'de> for Rgb {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum Form {
            Text(String),
            Channels([f64; 3]),
        }
        match Form::deserialize(deserializer)? {
            Form::Text(text) => Rgb::parse(&text).ok_or_else(|| {
                serde::de::Error::custom(format!("{text:?} is not a #rgb or #rrggbb color"))
            }),
            Form::Channels(channels) => {
                if channels.iter().all(|c| (0.0..=255.0).contains(c)) {
                    Ok(Rgb(channels.map(|c| c.round() as u8)))
                } else {
                    Err(serde::de::Error::custom("color channels must be 0..255"))
                }
            }
        }
    }
}

/// The main menu.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct MenuStyle {
    /// Replaces the "Tank Yard" title.
    pub title: Option<String>,
    /// A line under the title.
    pub subtitle: Option<String>,
    /// One is picked at random each time the game starts.
    pub splashes: Vec<String>,
    /// The tank class shown above the name field.
    pub showcase: Option<String>,
    /// An image in the pack, tiled behind the main menu.
    pub background: Option<String>,
    /// The drifting arena behind the main menu; `false` hides it.
    pub ambient: Option<bool>,
}

/// Interface colors. Unset colors keep the player's light or dark theme.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Theme {
    /// Selections, outlines, switches and progress.
    pub accent: Option<Rgb>,
    /// Behind menus.
    pub background: Option<Rgb>,
    /// Panels and buttons.
    pub surface: Option<Rgb>,
    pub line: Option<Rgb>,
    pub text: Option<Rgb>,
    /// Secondary text.
    pub muted: Option<Rgb>,
    /// Headings, tags and splash text.
    pub highlight: Option<Rgb>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Presentation {
    pub menu: MenuStyle,
    pub theme: Theme,
    /// Exact interface text to replace, such as `"Play offline"`.
    pub text: BTreeMap<String, String>,
}

impl Presentation {
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }

    /// Layer a later pack's presentation over this one: its settings win,
    /// splashes accumulate and text overrides merge.
    pub fn merge(&mut self, later: Presentation) {
        let menu = later.menu;
        macro_rules! take {
            ($target:expr, $($field:ident),+) => {
                $(if let Some(value) = menu.$field { $target.$field = Some(value); })+
            };
        }
        take!(self.menu, title, subtitle, showcase, background, ambient);
        self.menu.splashes.extend(menu.splashes);
        self.menu.splashes.truncate(MAX_SPLASHES);
        let theme = later.theme;
        macro_rules! color {
            ($($field:ident),+) => {
                $(if theme.$field.is_some() { self.theme.$field = theme.$field; })+
            };
        }
        color!(accent, background, surface, line, text, muted, highlight);
        for (from, to) in later.text {
            if self.text.len() < MAX_TEXT_OVERRIDES || self.text.contains_key(&from) {
                self.text.insert(from, to);
            }
        }
    }

    /// Check limits. `class` says whether a tank class exists in the loaded
    /// content. The background image is checked when it is decoded.
    pub fn validate(&self, class: impl Fn(&str) -> bool) -> Result<(), String> {
        let text = |value: &Option<String>, max: usize, what: &str| match value {
            Some(value) if !line(value, max) => Err(format!(
                "menu {what} must be 1..{max} bytes of text on one line"
            )),
            _ => Ok(()),
        };
        text(&self.menu.title, 48, "title")?;
        text(&self.menu.subtitle, 120, "subtitle")?;
        if self.menu.splashes.len() > MAX_SPLASHES {
            return Err(format!("at most {MAX_SPLASHES} menu splashes"));
        }
        if let Some(splash) = self.menu.splashes.iter().find(|s| !line(s, 80)) {
            return Err(format!(
                "menu splash {splash:?} must be 1..80 bytes on one line"
            ));
        }
        if let Some(id) = &self.menu.showcase
            && !class(id)
        {
            return Err(format!("menu showcase {id:?} is not a loaded tank"));
        }
        if self.text.len() > MAX_TEXT_OVERRIDES {
            return Err(format!("at most {MAX_TEXT_OVERRIDES} text overrides"));
        }
        for (from, to) in &self.text {
            if !line(from, 120) || !line(to, 120) {
                return Err(format!(
                    "text override {from:?} must map 1..120 bytes to 1..120 bytes"
                ));
            }
        }
        Ok(())
    }
}

fn line(text: &str, max: usize) -> bool {
    !text.trim().is_empty() && text.len() <= max && !text.chars().any(char::is_control)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn colors_parse_in_every_form() {
        assert_eq!(Rgb::parse("#fff"), Some(Rgb([255, 255, 255])));
        assert_eq!(Rgb::parse("#B14CFF"), Some(Rgb([0xb1, 0x4c, 0xff])));
        assert_eq!(Rgb::parse("b14cff"), None);
        assert_eq!(Rgb::parse("#b14cf"), None);
        let theme: Theme =
            serde_json::from_str(r##"{"accent": "#123", "text": [10, 20, 30]}"##).unwrap();
        assert_eq!(theme.accent, Some(Rgb([0x11, 0x22, 0x33])));
        assert_eq!(theme.text, Some(Rgb([10, 20, 30])));
        assert!(serde_json::from_str::<Theme>(r#"{"accent": "red"}"#).is_err());
        assert!(serde_json::from_str::<Theme>(r##"{"glow": "#fff"}"##).is_err());
    }

    #[test]
    fn later_packs_layer_over_earlier_ones() {
        let mut base: Presentation = serde_json::from_str(
            r##"{"menu": {"title": "A", "splashes": ["one"]},
                 "theme": {"accent": "#111", "text": "#222"},
                 "text": {"Play offline": "Solo"}}"##,
        )
        .unwrap();
        base.merge(
            serde_json::from_str(
                r##"{"menu": {"subtitle": "B", "splashes": ["two"]},
                     "theme": {"accent": "#333"},
                     "text": {"Mods": "Packs"}}"##,
            )
            .unwrap(),
        );
        assert_eq!(base.menu.title.as_deref(), Some("A"));
        assert_eq!(base.menu.subtitle.as_deref(), Some("B"));
        assert_eq!(base.menu.splashes, ["one", "two"]);
        assert_eq!(base.theme.accent, Some(Rgb([0x33, 0x33, 0x33])));
        assert_eq!(base.theme.text, Some(Rgb([0x22, 0x22, 0x22])));
        assert_eq!(base.text.len(), 2);
    }

    #[test]
    fn validation_checks_references_and_limits() {
        let mut p = Presentation::default();
        p.menu.showcase = Some("vector".into());
        assert!(p.validate(|c| c == "vector").is_ok());
        assert!(p.validate(|_| false).is_err());
        p.menu.title = Some("x".repeat(49));
        assert!(p.validate(|_| true).is_err());
        p.menu.title = None;
        p.text.insert("Play".into(), "Line\nbreak".into());
        assert!(p.validate(|_| true).is_err());
    }
}
