//! Versioned data packs and bounded server-side behavior hooks.
use serde::{Deserialize, Serialize};
use std::{
    collections::HashSet,
    path::{Path, PathBuf},
};
use tankyard_content::{ClassDef, Content, MapDef, builtin_maps, validate_maps};

pub const API_VERSION: u32 = 1;
pub const MAX_PACK_BYTES: usize = 1024 * 1024;
pub const MAX_MODS: usize = 16;
pub const MAX_COMMANDS: usize = 512;

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct GameRules {
    pub movement_multiplier: f32,
    pub acceleration: f32,
    pub coast: f32,
    pub damage_multiplier: f32,
    pub health_multiplier: f32,
    pub regen_multiplier: f32,
    pub xp_multiplier: f32,
    pub score_multiplier: f32,
    pub respawn_seconds: f32,
}
impl Default for GameRules {
    fn default() -> Self {
        Self {
            movement_multiplier: 1.0,
            acceleration: 8.0,
            coast: 6.0,
            damage_multiplier: 1.0,
            health_multiplier: 1.0,
            regen_multiplier: 1.0,
            xp_multiplier: 1.0,
            score_multiplier: 1.0,
            respawn_seconds: 3.0,
        }
    }
}
impl GameRules {
    pub fn validate(&self) -> Result<(), String> {
        for value in [
            self.movement_multiplier,
            self.damage_multiplier,
            self.health_multiplier,
            self.regen_multiplier,
            self.xp_multiplier,
            self.score_multiplier,
        ] {
            if !finite_range(value, 0.0, 10.0) {
                return Err("rule multipliers must be finite and between 0 and 10".into());
            }
        }
        if self.health_multiplier < 0.1
            || self.movement_multiplier < 0.1
            || !finite_range(self.acceleration, 0.1, 30.0)
            || !finite_range(self.coast, 0.1, 30.0)
            || !finite_range(self.respawn_seconds, 0.5, 60.0)
        {
            return Err("invalid health, movement, acceleration, coast, or respawn rules".into());
        }
        Ok(())
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ModInfo {
    pub id: String,
    pub name: String,
    pub version: String,
    pub behavior_digest: u64,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TextureData {
    pub id: String,
    pub width: u16,
    pub height: u16,
    pub rgba: Vec<u8>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TextureSource {
    pub id: String,
    pub file: String,
}

/// Only validated data travels to clients. Behavior hooks stay on the server.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GameBundle {
    pub api_version: u32,
    pub content: Content,
    pub maps: Vec<MapDef>,
    pub rules: GameRules,
    pub mods: Vec<ModInfo>,
    #[serde(default)]
    pub mode_name: Option<String>,
    #[serde(default)]
    pub textures: Vec<TextureData>,
}
impl GameBundle {
    pub fn builtin() -> Self {
        Self {
            api_version: API_VERSION,
            content: Content::builtin(),
            maps: builtin_maps(),
            rules: GameRules::default(),
            mods: vec![],
            mode_name: None,
            textures: vec![],
        }
    }
    pub fn validate(&self) -> Result<(), String> {
        if self.api_version != API_VERSION {
            return Err("unsupported mod API version".into());
        }
        self.content.validate_modded()?;
        validate_maps(&self.maps)?;
        self.rules.validate()?;
        if self.textures.len() > 16 {
            return Err("at most 16 textures may be loaded".into());
        }
        let mut textures = HashSet::new();
        for t in &self.textures {
            if !identifier(&t.id)
                || !textures.insert(&t.id)
                || t.width == 0
                || t.height == 0
                || t.width > 256
                || t.height > 256
                || t.rgba.len() != usize::from(t.width) * usize::from(t.height) * 4
            {
                return Err("invalid texture ID, dimensions, or pixel count".into());
            }
        }
        if self.content.classes.iter().any(|c| {
            c.body_texture
                .as_ref()
                .is_some_and(|id| !textures.contains(id))
        }) {
            return Err("tank references a missing body texture".into());
        }
        if self
            .mode_name
            .as_ref()
            .is_some_and(|name| !display_text(name, 48))
        {
            return Err("invalid custom mode name".into());
        }
        if self.mods.len() > MAX_MODS {
            return Err("too many mods".into());
        }
        let mut ids = HashSet::new();
        for m in &self.mods {
            if !identifier(&m.id)
                || !display_text(&m.name, 64)
                || !display_text(&m.version, 32)
                || !ids.insert(&m.id)
            {
                return Err("invalid or duplicate mod metadata".into());
            }
        }
        if serde_json::to_vec(self).map_err(|e| e.to_string())?.len() > MAX_PACK_BYTES {
            return Err("game data exceeds 1 MiB".into());
        }
        Ok(())
    }
    pub fn fingerprint(&self) -> u64 {
        digest(&serde_json::to_vec(self).expect("validated game data is serializable"))
    }
    pub fn modded(&self) -> bool {
        !self.mods.is_empty()
            || self.rules != GameRules::default()
            || self.mode_name.is_some()
            || !self.textures.is_empty()
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    pub api_version: u32,
    pub id: String,
    pub name: String,
    pub version: String,
    #[serde(default)]
    pub dependencies: Vec<String>,
    #[serde(default)]
    pub classes: Vec<ClassDef>,
    #[serde(default)]
    pub maps: Vec<MapDef>,
    /// Later mods override the complete rule profile explicitly.
    #[serde(default)]
    pub rules: Option<GameRules>,
    #[serde(default)]
    pub hooks: Vec<Hook>,
    #[serde(default)]
    pub mode_name: Option<String>,
    #[serde(default)]
    pub textures: Vec<TextureSource>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Trigger {
    Tick,
    Spawn,
    Kill,
    Death,
    Shot,
    LevelUp,
    Evolve,
    Capture,
    RoundStart,
    RoundEnd,
}
#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Target {
    #[default]
    Actor,
    AllPlayers,
    AlivePlayers,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Hook {
    pub id: String,
    pub event: Trigger,
    #[serde(default)]
    pub target: Target,
    #[serde(default = "one")]
    pub every_ticks: u64,
    #[serde(default)]
    pub cooldown_ticks: u64,
    #[serde(default)]
    pub conditions: Vec<Condition>,
    pub actions: Vec<Effect>,
}
fn one() -> u64 {
    1
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum Condition {
    HealthBelow { fraction: f32 },
    LevelAtLeast { level: u8 },
    ClassIs { class: String },
    TeamIs { team: u8 },
    IsBot { value: bool },
    CounterAtLeast { name: String, value: f64 },
    Any { conditions: Vec<Condition> },
    Not { condition: Box<Condition> },
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum Effect {
    Control {
        movement: [f32; 2],
        aim: f32,
        fire: bool,
        ability: bool,
    },
    Heal {
        amount: f32,
    },
    Damage {
        amount: f32,
    },
    Award {
        xp: u32,
        score: u32,
    },
    SetClass {
        class: String,
    },
    SetTeam {
        team: u8,
    },
    Impulse {
        velocity: [f32; 2],
    },
    Teleport {
        position: [f32; 2],
    },
    Counter {
        name: String,
        add: f64,
    },
    Announce {
        message: String,
    },
    FinishRound {
        team: Option<u8>,
        reason: String,
    },
    SpawnTarget {
        position: [f32; 2],
        sides: u8,
        health: f32,
        xp: u32,
    },
    Fire {
        speed: f32,
        damage: f32,
        radius: f32,
        lifetime: f32,
        angle: f32,
    },
}
impl Effect {
    pub fn validate(&self, content: &Content) -> Result<(), String> {
        let valid = match self {
            Self::Control { movement, aim, .. } => {
                vector(*movement, -1.0, 1.0)
                    && movement[0] * movement[0] + movement[1] * movement[1] <= 1.001
                    && finite_range(*aim, -6.3, 6.3)
            }
            Self::Heal { amount } | Self::Damage { amount } => finite_range(*amount, 0.0, 10_000.0),
            Self::Award { xp, score } => *xp <= 1_000_000 && *score <= 1_000_000,
            Self::SetClass { class } => content.class(class).is_some(),
            Self::SetTeam { team } => *team <= 4,
            Self::Impulse { velocity } => vector(*velocity, -2000.0, 2000.0),
            Self::Teleport { position } | Self::SpawnTarget { position, .. } => {
                vector(*position, 0.0, 1.0)
            }
            Self::Counter { name, add } => identifier(name) && add.is_finite() && add.abs() <= 1e9,
            Self::Announce { message }
            | Self::FinishRound {
                reason: message, ..
            } => display_text(message, 160),
            Self::Fire {
                speed,
                damage,
                radius,
                lifetime,
                angle,
            } => {
                finite_range(*speed, 1.0, 1600.0)
                    && finite_range(*damage, 0.0, 10000.0)
                    && finite_range(*radius, 1.0, 60.0)
                    && finite_range(*lifetime, 0.05, 30.0)
                    && finite_range(*angle, -6.3, 6.3)
            }
        };
        if !valid {
            return Err("invalid hook action or class reference".into());
        }
        if let Self::FinishRound {
            team: Some(team), ..
        } = self
            && *team > 4
        {
            return Err("invalid winning team".into());
        }
        if let Self::SpawnTarget {
            sides, health, xp, ..
        } = self
            && (!(3..=12).contains(sides)
                || !finite_range(*health, 1.0, 10_000.0)
                || *xp > 1_000_000)
        {
            return Err("invalid target properties".into());
        }
        Ok(())
    }
}
impl Condition {
    fn validate(&self, content: &Content, depth: usize) -> bool {
        if depth > 4 {
            return false;
        }
        match self {
            Self::HealthBelow { fraction } => finite_range(*fraction, 0.0, 1.0),
            Self::LevelAtLeast { level } => (1..=45).contains(level),
            Self::ClassIs { class } => content.class(class).is_some(),
            Self::TeamIs { team } => *team <= 4,
            Self::IsBot { .. } => true,
            Self::CounterAtLeast { name, value } => {
                identifier(name) && value.is_finite() && value.abs() <= 1e9
            }
            Self::Any { conditions } => {
                !conditions.is_empty()
                    && conditions.len() <= 16
                    && conditions.iter().all(|c| c.validate(content, depth + 1))
            }
            Self::Not { condition } => condition.validate(content, depth + 1),
        }
    }
}
#[derive(Clone, Debug)]
pub struct LoadedMod {
    pub info: ModInfo,
    pub hooks: Vec<Hook>,
}
#[derive(Clone, Debug)]
pub struct LoadedMods {
    pub bundle: GameBundle,
    pub behaviors: Vec<LoadedMod>,
    pub rules_override: bool,
}
impl Default for LoadedMods {
    fn default() -> Self {
        Self {
            bundle: GameBundle::builtin(),
            behaviors: vec![],
            rules_override: false,
        }
    }
}

/// Dependency order is deterministic. Packs never execute downloaded code.
pub fn load(paths: &[PathBuf]) -> Result<LoadedMods, String> {
    if paths.len() > MAX_MODS {
        return Err("at most 16 mods can be enabled".into());
    }
    let mut manifests = Vec::with_capacity(paths.len());
    let mut ids = HashSet::new();
    for path in paths {
        let root = path
            .canonicalize()
            .map_err(|e| format!("{}: {e}", path.display()))?;
        let file = root
            .join("mod.json")
            .canonicalize()
            .map_err(|e| format!("{}: {e}", path.display()))?;
        if !file.starts_with(&root) {
            return Err("mod manifest symlinks cannot leave their directory".into());
        }
        let bytes = read_bounded(&file)?;
        let manifest: Manifest =
            serde_json::from_slice(&bytes).map_err(|e| format!("{}: {e}", path.display()))?;
        if manifest.api_version != API_VERSION
            || !identifier(&manifest.id)
            || !display_text(&manifest.name, 64)
            || !display_text(&manifest.version, 32)
            || !ids.insert(manifest.id.clone())
            || manifest.dependencies.len() > MAX_MODS
            || manifest.dependencies.iter().any(|d| !identifier(d))
        {
            return Err("invalid, duplicate, or incompatible mod manifest".into());
        }
        if manifest.classes.len() > 1024
            || manifest.maps.len() > 256
            || manifest.textures.len() > 16
            || manifest
                .textures
                .iter()
                .map(|t| &t.id)
                .collect::<HashSet<_>>()
                .len()
                != manifest.textures.len()
            || manifest
                .classes
                .iter()
                .map(|c| &c.id)
                .collect::<HashSet<_>>()
                .len()
                != manifest.classes.len()
            || manifest
                .maps
                .iter()
                .map(|m| &m.id)
                .collect::<HashSet<_>>()
                .len()
                != manifest.maps.len()
            || manifest.dependencies.iter().collect::<HashSet<_>>().len()
                != manifest.dependencies.len()
        {
            return Err("duplicate or excessive mod definitions".into());
        }
        let mut textures = Vec::new();
        for source in &manifest.textures {
            if !identifier(&source.id)
                || source.file.len() > 256
                || !Path::new(&source.file)
                    .components()
                    .all(|part| matches!(part, std::path::Component::Normal(_)))
            {
                return Err("invalid texture file path".into());
            }
            let path = root
                .join(&source.file)
                .canonicalize()
                .map_err(|e| e.to_string())?;
            if !path.starts_with(&root) {
                return Err("texture symlink cannot leave the mod directory".into());
            }
            let bytes = read_bounded(&path)?;
            let mut reader = image::io::Reader::with_format(
                std::io::Cursor::new(bytes),
                image::ImageFormat::Png,
            );
            let mut limits = image::io::Limits::default();
            limits.max_image_width = Some(256);
            limits.max_image_height = Some(256);
            limits.max_alloc = Some(1024 * 1024);
            reader.limits(limits);
            let pixels = reader
                .decode()
                .map_err(|e| format!("invalid mod PNG: {e}"))?
                .into_rgba8();
            textures.push(TextureData {
                id: source.id.clone(),
                width: pixels.width() as u16,
                height: pixels.height() as u16,
                rgba: pixels.into_raw(),
            });
        }
        manifests.push((manifest, textures));
    }
    let mut result = LoadedMods::default();
    let mut installed = HashSet::new();
    while !manifests.is_empty() {
        let Some(index) = manifests
            .iter()
            .position(|(m, _)| m.dependencies.iter().all(|d| installed.contains(d)))
        else {
            return Err("mod dependency is missing or cyclic".into());
        };
        let (manifest, textures) = manifests.remove(index);
        for texture in textures {
            if let Some(existing) = result
                .bundle
                .textures
                .iter_mut()
                .find(|t| t.id == texture.id)
            {
                *existing = texture;
            } else {
                result.bundle.textures.push(texture);
            }
        }
        for class in manifest.classes {
            if let Some(existing) = result
                .bundle
                .content
                .classes
                .iter_mut()
                .find(|c| c.id == class.id)
            {
                *existing = class;
            } else {
                result.bundle.content.classes.push(class);
            }
        }
        for map in manifest.maps {
            if let Some(existing) = result.bundle.maps.iter_mut().find(|m| m.id == map.id) {
                *existing = map;
            } else {
                result.bundle.maps.push(map);
            }
        }
        if let Some(rules) = manifest.rules {
            rules.validate()?;
            result.bundle.rules = rules;
            result.rules_override = true;
        }
        if manifest.mode_name.is_some() {
            result.bundle.mode_name = manifest.mode_name;
        }
        let info = ModInfo {
            id: manifest.id.clone(),
            name: manifest.name,
            version: manifest.version,
            behavior_digest: digest(
                &serde_json::to_vec(&manifest.hooks).map_err(|e| e.to_string())?,
            ),
        };
        installed.insert(manifest.id);
        result.bundle.mods.push(info.clone());
        result.behaviors.push(LoadedMod {
            info,
            hooks: manifest.hooks,
        });
    }
    result.bundle.validate()?;
    for m in &result.behaviors {
        validate_hooks(&m.hooks, &result.bundle.content)?;
    }
    Ok(result)
}
pub fn validate_hooks(hooks: &[Hook], content: &Content) -> Result<(), String> {
    if hooks.len() > 64 {
        return Err("a mod may register at most 64 hooks".into());
    }
    let mut ids = HashSet::new();
    for h in hooks {
        if !identifier(&h.id)
            || !ids.insert(&h.id)
            || h.every_ticks == 0
            || h.every_ticks > 216_000
            || h.cooldown_ticks > 216_000
            || h.conditions.len() > 16
            || h.actions.is_empty()
            || h.actions.len() > 16
            || !h.conditions.iter().all(|c| c.validate(content, 0))
        {
            return Err("invalid hook ID, frequency, conditions, or action count".into());
        }
        for a in &h.actions {
            a.validate(content)?;
        }
    }
    Ok(())
}
pub fn read_bounded(path: &Path) -> Result<Vec<u8>, String> {
    if std::fs::metadata(path).map_err(|e| e.to_string())?.len() > MAX_PACK_BYTES as u64 {
        return Err("mod file exceeds 1 MiB".into());
    }
    use std::io::Read;
    let file = std::fs::File::open(path).map_err(|e| e.to_string())?;
    let mut bytes = Vec::new();
    file.take((MAX_PACK_BYTES + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() > MAX_PACK_BYTES {
        return Err("mod file grew beyond 1 MiB".into());
    }
    Ok(bytes)
}
pub fn identifier(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 48
        && id
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
}
fn display_text(s: &str, max: usize) -> bool {
    !s.trim().is_empty() && s.len() <= max && !s.chars().any(char::is_control)
}
fn vector(v: [f32; 2], lo: f32, hi: f32) -> bool {
    v.iter().all(|x| finite_range(*x, lo, hi))
}
fn finite_range(v: f32, lo: f32, hi: f32) -> bool {
    v.is_finite() && (lo..=hi).contains(&v)
}
pub fn digest(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf29ce484222325, |h, b| {
        (h ^ u64::from(*b)).wrapping_mul(0x100000001b3)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn test_dir() -> PathBuf {
        static ID: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "tankyard-mod-test-{}-{}",
            std::process::id(),
            ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        std::fs::create_dir_all(&path).unwrap();
        path
    }
    fn manifest(root: &Path, id: &str, dependencies: &[&str]) -> PathBuf {
        let path = root.join(id);
        std::fs::create_dir_all(&path).unwrap();
        let data = serde_json::json!({"api_version":1,"id":id,"name":id,"version":"1","dependencies":dependencies});
        std::fs::write(path.join("mod.json"), serde_json::to_vec(&data).unwrap()).unwrap();
        path
    }
    #[test]
    fn dependencies_are_ordered_and_cycles_or_duplicate_packs_fail() {
        let root = test_dir();
        let base = manifest(&root, "base", &[]);
        let addon = manifest(&root, "addon", &["base"]);
        let loaded = load(&[addon.clone(), base.clone()]).unwrap();
        assert_eq!(loaded.bundle.mods[0].id, "base");
        assert!(load(std::slice::from_ref(&addon)).is_err());
        assert!(load(&[base.clone(), base.clone()]).is_err());
        manifest(&root, "base", &["addon"]);
        assert!(load(&[base, addon]).is_err());
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn texture_paths_and_pixel_payloads_are_bounded() {
        let root = test_dir();
        let pack = manifest(&root, "test", &[]);
        std::fs::write(pack.join("mod.json"),r#"{"api_version":1,"id":"test","name":"test","version":"1","textures":[{"id":"skin","file":"../secret.png"}]}"#).unwrap();
        assert!(load(&[pack]).unwrap_err().contains("texture file path"));
        let mut bundle = GameBundle::builtin();
        bundle.textures.push(TextureData {
            id: "skin".into(),
            width: 2,
            height: 2,
            rgba: vec![0; 12],
        });
        assert!(bundle.validate().is_err());
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn vanilla_bundle_roundtrips_and_rules_are_bounded() {
        let b = GameBundle::builtin();
        b.validate().unwrap();
        let saved = serde_json::to_vec(&b).unwrap();
        let roundtrip: GameBundle = serde_json::from_slice(&saved).unwrap();
        assert_eq!(b.fingerprint(), roundtrip.fingerprint());
        let mut b = b;
        b.rules.acceleration = f32::NAN;
        assert!(b.validate().is_err());
    }
    #[test]
    fn invalid_hook_actions_and_deep_conditions_are_rejected() {
        let c = Content::builtin();
        assert!(
            Effect::Teleport {
                position: [-1.0, 0.0]
            }
            .validate(&c)
            .is_err()
        );
        assert!(
            Effect::SetClass {
                class: "missing".into()
            }
            .validate(&c)
            .is_err()
        );
        let mut condition = Condition::IsBot { value: true };
        for _ in 0..6 {
            condition = Condition::Not {
                condition: Box::new(condition),
            };
        }
        assert!(!condition.validate(&c, 0));
    }
}
