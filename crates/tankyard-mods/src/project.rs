//! `tankyard.toml`: a pack's storefront details for the Mods page. It is
//! optional and never affects gameplay; `mod.json` stays the source of truth
//! for what a pack does.
//!
//! ```toml
//! [mod]
//! name = "Bounty Hunt"
//! version = "1.2.0"
//! description = "Hunt the leader for bonus score."
//! license = "MIT"
//! icon = "icon.png"          # any supported image, editor file or model
//! tags = ["mode", "pvp"]
//!
//! [creator]
//! name = "Your Name"
//! url = "https://example.com"
//!
//! [links]
//! source = "https://github.com/you/bounty-hunt"   # if open source
//! download = "https://example.com/bounty-hunt.zip"
//! issues = "https://github.com/you/bounty-hunt/issues"
//! homepage = "https://example.com"
//! ```
use crate::{PackFiles, TextureData, assets};
use serde::Deserialize;

pub const FILE: &str = "tankyard.toml";

#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Project {
    #[serde(rename = "mod")]
    pub details: Details,
    pub creator: Creator,
    pub links: Links,
}

#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Details {
    pub name: Option<String>,
    pub version: Option<String>,
    pub description: Option<String>,
    pub license: Option<String>,
    pub icon: Option<String>,
    pub tags: Vec<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Creator {
    pub name: Option<String>,
    pub url: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Links {
    pub source: Option<String>,
    pub download: Option<String>,
    pub issues: Option<String>,
    pub homepage: Option<String>,
}

impl Links {
    /// Labelled links in display order.
    pub fn all(&self) -> Vec<(&'static str, &str)> {
        [
            ("Source code", &self.source),
            ("Download", &self.download),
            ("Issues", &self.issues),
            ("Website", &self.homepage),
        ]
        .into_iter()
        .filter_map(|(label, url)| url.as_deref().map(|url| (label, url)))
        .collect()
    }
}

/// Only plain web links: they open in the player's browser.
pub fn web_link(url: &str) -> bool {
    url.len() <= 512
        && (url.starts_with("https://") || url.starts_with("http://"))
        && url.len() > "https://".len()
        && !url
            .chars()
            .any(|c| c.is_whitespace() || c.is_control() || c == '"')
}

fn clean(value: &mut Option<String>, max: usize) {
    if let Some(text) = value {
        *text = text
            .chars()
            .filter(|c| !c.is_control() || *c == '\n')
            .take(max)
            .collect::<String>()
            .trim()
            .to_owned();
        if text.is_empty() {
            *value = None;
        }
    }
}

impl Project {
    pub fn parse(text: &str) -> Result<Self, String> {
        if text.len() > 16 * 1024 {
            return Err(format!("{FILE} is larger than 16 KiB"));
        }
        let mut project: Self =
            toml::from_str(text).map_err(|e| format!("{FILE}: {}", e.message()))?;
        let d = &mut project.details;
        clean(&mut d.name, 48);
        clean(&mut d.version, 24);
        clean(&mut d.description, 600);
        clean(&mut d.license, 48);
        clean(&mut d.icon, 256);
        d.tags.truncate(8);
        for tag in &mut d.tags {
            *tag = tag.chars().filter(|c| !c.is_control()).take(20).collect();
        }
        d.tags.retain(|t| !t.trim().is_empty());
        clean(&mut project.creator.name, 48);
        for url in [
            &mut project.creator.url,
            &mut project.links.source,
            &mut project.links.download,
            &mut project.links.issues,
            &mut project.links.homepage,
        ] {
            clean(url, 512);
            if let Some(link) = url
                && !web_link(link)
            {
                return Err(format!("{FILE}: {link} is not an http(s) link"));
            }
        }
        if let Some(icon) = &d.icon
            && (!crate::pack_path(icon) || !assets::is_asset(icon))
        {
            return Err(format!(
                "{FILE}: icon {icon} is not an image or model in this pack"
            ));
        }
        Ok(project)
    }

    /// The pack's project details, if it has a `tankyard.toml`.
    pub fn read(pack: &PackFiles) -> Option<Result<Self, String>> {
        let bytes = pack.get(FILE)?;
        Some(
            std::str::from_utf8(bytes)
                .map_err(|_| format!("{FILE} must be UTF-8 text"))
                .and_then(Self::parse),
        )
    }

    /// Decode the pack's icon, if it names one.
    pub fn icon(&self, pack: &PackFiles) -> Option<Result<TextureData, String>> {
        let file = self.details.icon.as_deref()?;
        let bytes = pack.get(file)?;
        let files = |name: &str| pack.get(name).map(<[u8]>::to_vec);
        Some(crate::decode_texture("icon", file, bytes, &files))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn project_files_parse_and_reject_unsafe_links() {
        let project = Project::parse(
            r#"
            [mod]
            name = "Bounty Hunt"
            version = "1.2.0"
            icon = "art/icon.svg"
            tags = ["mode", "pvp"]
            [creator]
            name = "Robert"
            [links]
            source = "https://github.com/example/bounty"
            issues = "https://github.com/example/bounty/issues"
            "#,
        )
        .unwrap();
        assert_eq!(project.details.name.as_deref(), Some("Bounty Hunt"));
        assert_eq!(project.creator.name.as_deref(), Some("Robert"));
        assert_eq!(
            project.links.all(),
            vec![
                ("Source code", "https://github.com/example/bounty"),
                ("Issues", "https://github.com/example/bounty/issues"),
            ]
        );
        assert!(Project::parse("[links]\nsource = \"file:///etc/passwd\"").is_err());
        assert!(Project::parse("[links]\nissues = \"javascript:alert(1)\"").is_err());
        assert!(Project::parse("[mod]\nicon = \"../outside.png\"").is_err());
        assert!(Project::parse("[mod]\nunknown = 1").is_err());
    }
}
