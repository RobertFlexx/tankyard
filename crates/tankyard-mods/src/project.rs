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
//!
//! [source]
//! viewable = false           # hide every file's contents in View files
//! private = ["lib/", "*.lua"]  # or hide only these files
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
    pub source: Source,
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

/// Whether players may read the pack's files in the Mods page's file
/// browser. It is a courtesy, not protection: the files are still in the
/// pack, and the safety review still reads every one of them.
#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Source {
    pub viewable: bool,
    /// Files to keep private: exact paths, `folder/`, or patterns where `*`
    /// matches within a folder name and `**` across folders.
    pub private: Vec<String>,
}

impl Default for Source {
    fn default() -> Self {
        Self {
            viewable: true,
            private: Vec::new(),
        }
    }
}

const MAX_PRIVATE: usize = 64;

impl Source {
    /// Whether `file`'s contents may be shown.
    pub fn readable(&self, file: &str) -> bool {
        self.viewable && !self.private.iter().any(|pattern| matches(pattern, file))
    }
    /// Whether any file is hidden at all.
    pub fn restricted(&self) -> bool {
        !self.viewable || !self.private.is_empty()
    }
}

/// Match a pack path against a `private` pattern. A pattern ending in `/`
/// covers everything in that folder.
pub fn matches(pattern: &str, path: &str) -> bool {
    let pattern = match pattern.strip_suffix('/') {
        Some(folder) => format!("{folder}/**"),
        None => pattern.to_owned(),
    };
    let (p, s) = (pattern.as_bytes(), path.as_bytes());
    // memo[i][j]: whether p[i..] matches s[j..]. Patterns and paths are
    // short, so this stays small and never backtracks exponentially.
    let mut memo = vec![vec![None; s.len() + 1]; p.len() + 1];
    fn go(p: &[u8], s: &[u8], i: usize, j: usize, memo: &mut Vec<Vec<Option<bool>>>) -> bool {
        if let Some(known) = memo[i][j] {
            return known;
        }
        let result = match &p[i..] {
            [] => j == s.len(),
            [b'*', b'*', rest @ ..] => {
                // `**/` may also match no folders at all.
                let next = i + 2 + usize::from(rest.first() == Some(&b'/'));
                (j..=s.len()).any(|k| {
                    (k == j || s[k - 1] == b'/' || next == p.len()) && go(p, s, next, k, memo)
                })
            }
            [b'*', ..] => (j..=s.len())
                .take_while(|&k| k == j || s[k - 1] != b'/')
                .any(|k| go(p, s, i + 1, k, memo)),
            [b'?', ..] => s.get(j).is_some_and(|&c| c != b'/') && go(p, s, i + 1, j + 1, memo),
            [c, ..] => s.get(j) == Some(c) && go(p, s, i + 1, j + 1, memo),
        };
        memo[i][j] = Some(result);
        result
    }
    go(p, s, 0, 0, &mut memo)
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
        let private = &project.source.private;
        if private.len() > MAX_PRIVATE {
            return Err(format!(
                "{FILE}: [source] private lists more than {MAX_PRIVATE} patterns"
            ));
        }
        if let Some(bad) = private.iter().find(|p| {
            p.is_empty()
                || p.len() > 200
                || p.starts_with('/')
                || p.contains('\\')
                || p.split('/').any(|part| part == "..")
                || p.chars().any(char::is_control)
        }) {
            return Err(format!(
                "{FILE}: [source] private pattern {bad:?} is not a path in this pack"
            ));
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
    #[test]
    fn source_is_viewable_unless_the_creator_says_otherwise() {
        let open = Project::parse("[mod]\nname = \"Open\"").unwrap();
        assert!(open.source.viewable && !open.source.restricted());
        assert!(open.source.readable("main.lua"));

        let closed = Project::parse("[source]\nviewable = false").unwrap();
        assert!(closed.source.restricted());
        assert!(!closed.source.readable("main.lua"));
        assert!(!closed.source.readable("art/icon.png"));
    }

    #[test]
    fn private_patterns_hide_only_matching_files() {
        let project = Project::parse(
            r#"
            [source]
            private = ["lib/", "*.secret", "**/keys.json", "hud?.lua", "exact/file.lua"]
            "#,
        )
        .unwrap();
        let source = &project.source;
        assert!(source.restricted());
        for hidden in [
            "lib/content.lua",
            "lib/deep/util.lua",
            "notes.secret",
            "keys.json",
            "data/keys.json",
            "a/b/keys.json",
            "hud2.lua",
            "exact/file.lua",
        ] {
            assert!(!source.readable(hidden), "{hidden} should be private");
        }
        for shown in [
            "main.lua",
            "library.lua",
            "art/notes.secret",
            "hud.lua",
            "hud10.lua",
            "exact/file.lua.bak",
            "mod.json",
        ] {
            assert!(source.readable(shown), "{shown} should be readable");
        }
    }

    #[test]
    fn patterns_match_like_paths() {
        assert!(matches("*.lua", "main.lua"));
        assert!(!matches("*.lua", "lib/main.lua"));
        assert!(matches("**/*.lua", "main.lua"));
        assert!(matches("**/*.lua", "lib/x/main.lua"));
        assert!(matches("lib/**", "lib/a/b.lua"));
        assert!(matches("lib/**/b.lua", "lib/b.lua"));
        assert!(matches("lib/**/b.lua", "lib/a/c/b.lua"));
        assert!(!matches("lib/**/b.lua", "libx/b.lua"));
        assert!(matches("**", "anything/at/all"));
        assert!(!matches("art/", "artwork/icon.png"));
        // Many stars stay fast: the matcher remembers what it has tried.
        let path = format!("{}b", "a/".repeat(90));
        assert!(!matches(&"*a".repeat(40), &path));
        assert!(matches(&format!("{}b", "**/".repeat(20)), &path));
    }

    #[test]
    fn unsafe_or_excessive_private_patterns_are_rejected() {
        for bad in [
            r#""""#,
            r#""/etc/passwd""#,
            r#""../outside.lua""#,
            r#""lib/../../x""#,
            r#""a\\b""#,
        ] {
            let text = format!("[source]\nprivate = [{bad}]");
            assert!(Project::parse(&text).is_err(), "{bad} was accepted");
        }
        let many = (0..65)
            .map(|i| format!("\"f{i}.lua\""))
            .collect::<Vec<_>>()
            .join(", ");
        assert!(Project::parse(&format!("[source]\nprivate = [{many}]")).is_err());
        assert!(Project::parse("[source]\nviewable = \"no\"").is_err());
        assert!(Project::parse("[source]\nhidden = true").is_err());
    }
}
