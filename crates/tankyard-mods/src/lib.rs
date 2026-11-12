//! Versioned data packs, sandboxed script packs and bounded server-side
//! behavior hooks. Packs are folders or `.zip` archives.
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, HashSet},
    path::{Path, PathBuf},
};
use tankyard_content::{ClassDef, Content, MapDef, builtin_maps, validate_maps};

pub mod assets;
pub mod pack;
pub mod presentation;
pub mod project;
pub mod scan;
pub mod staff;
pub use pack::PackFiles;
pub use presentation::Presentation;

/// Wire version of validated game data and of JSON-only manifests.
pub const API_VERSION: u32 = 1;
/// Manifests that also run sandboxed Lua scripts.
pub const SCRIPT_API_VERSION: u32 = 2;
pub const MAX_SCRIPTS: usize = 16;
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
    pub description: Option<String>,
    #[serde(default)]
    pub author: Option<String>,
    /// Lua entry scripts, run in order inside the sandbox (API version 2).
    #[serde(default)]
    pub scripts: Vec<String>,
    /// Lua entry scripts for the Arena Workshop: tools and checks that run
    /// in the same sandbox while a map is edited (API version 2).
    #[serde(default)]
    pub editor: Vec<String>,
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
    /// The main menu, interface colors and wording on the player's machine.
    #[serde(default)]
    pub menu: Option<presentation::MenuStyle>,
    #[serde(default)]
    pub theme: Option<presentation::Theme>,
    #[serde(default)]
    pub text: BTreeMap<String, String>,
    /// A signed, password-sealed staff credential instead of game content.
    #[serde(default)]
    pub staff: Option<staff::StaffSection>,
}

impl Manifest {
    /// Workshop plugins and nothing a match would load: such packs stay out
    /// of games so enabling them never marks a match as modded.
    pub fn editor_only(&self) -> bool {
        !self.editor.is_empty()
            && self.scripts.is_empty()
            && self.classes.is_empty()
            && self.maps.is_empty()
            && self.rules.is_none()
            && self.hooks.is_empty()
            && self.mode_name.is_none()
            && self.textures.is_empty()
            && self.menu.is_none()
            && self.theme.is_none()
            && self.text.is_empty()
            && self.staff.is_none()
    }
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
    /// A tank lost health; the actor is the victim and the target its attacker.
    Damage,
    /// A map trigger's Script event action.
    MapEvent,
}
impl Trigger {
    pub const ALL: [Self; 12] = [
        Self::Tick,
        Self::Spawn,
        Self::Kill,
        Self::Death,
        Self::Shot,
        Self::LevelUp,
        Self::Evolve,
        Self::Capture,
        Self::RoundStart,
        Self::RoundEnd,
        Self::Damage,
        Self::MapEvent,
    ];
    pub fn name(self) -> &'static str {
        match self {
            Self::Tick => "tick",
            Self::Spawn => "spawn",
            Self::Kill => "kill",
            Self::Death => "death",
            Self::Shot => "shot",
            Self::LevelUp => "level_up",
            Self::Evolve => "evolve",
            Self::Capture => "capture",
            Self::RoundStart => "round_start",
            Self::RoundEnd => "round_end",
            Self::Damage => "damage",
            Self::MapEvent => "map_event",
        }
    }
    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|trigger| trigger.name() == name)
    }
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
    /// Jump to a level; skill points follow the normal level budget.
    SetLevel {
        level: u8,
    },
    /// Raise one skill without spending a point, up to the normal maximum.
    RaiseStat {
        stat: u8,
    },
    /// Authoritative movement, reload and outgoing damage multipliers.
    SetScales {
        speed: f32,
        reload: f32,
        damage: f32,
    },
    /// Multiplier applied to incoming damage; 1 restores normal damage.
    SetArmor {
        multiplier: f32,
    },
    SetShield {
        seconds: f32,
    },
    SetProtection {
        seconds: f32,
    },
    SetVelocity {
        velocity: [f32; 2],
    },
    Kill,
    SetScore {
        score: u32,
    },
    SetTeamScore {
        team: u8,
        score: u32,
    },
    /// Spawn a boss by its display or enum name, such as "Guardian".
    SpawnBoss {
        boss: String,
    },
    /// Remove the actor's projectiles, or every non-guardian projectile for actor 0.
    ClearProjectiles,
    /// Show or change a HUD widget on the actor's screen (everyone's for
    /// actor 0); without a widget, remove it.
    Hud {
        id: String,
        #[serde(default)]
        widget: Option<HudWidget>,
    },
    /// Fire the map's On signal triggers on `channel` (1 to 32).
    MapSignal {
        channel: u8,
    },
    /// Set map counter `counter` (1 to 8) to `value`, or add `value` to it.
    MapCounter {
        counter: u8,
        value: i32,
        #[serde(default)]
        add: bool,
    },
    /// Show or hide map group `group` (1 to 32).
    MapGroup {
        group: u8,
        shown: bool,
    },
    /// Start the triggers of map group `group` afresh, as a Reset triggers
    /// action does; 0 resets every trigger.
    MapReset {
        group: u8,
    },
}
/// Widgets one pack may show to one player (or to everyone).
pub const MAX_HUD_WIDGETS: usize = 16;
/// Widgets a player receives from every pack together.
pub const MAX_HUD_SENT: usize = 64;
/// A line of text, a progress bar or both, drawn on players' screens. The
/// server decides what each player sees and clients only draw it, so a HUD
/// can't drift from the game.
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
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HudAlign {
    Left,
    #[default]
    Center,
    Right,
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
    pub fn valid(&self) -> bool {
        (display_text(&self.text, 120) || (self.text.is_empty() && self.progress.is_some()))
            && finite_range(self.x, 0., 1.)
            && finite_range(self.y, 0., 1.)
            && finite_range(self.size, 10., 48.)
            && self.progress.is_none_or(|p| finite_range(p, 0., 1.))
            && finite_range(self.width, 0.02, 1.)
    }
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
            Self::SetLevel { level } => (1..=tankyard_content::MAX_LEVEL).contains(level),
            Self::RaiseStat { stat } => *stat < 8,
            Self::SetScales {
                speed,
                reload,
                damage,
            } => {
                finite_range(*speed, 0.25, 6.0)
                    && finite_range(*reload, 0.25, 6.0)
                    && finite_range(*damage, 0.25, 8.0)
            }
            Self::SetArmor { multiplier } => finite_range(*multiplier, 0.0, 10.0),
            Self::SetShield { seconds } | Self::SetProtection { seconds } => {
                finite_range(*seconds, 0.0, 30.0)
            }
            Self::SetVelocity { velocity } => vector(*velocity, -2000.0, 2000.0),
            Self::Kill | Self::ClearProjectiles => true,
            Self::SetScore { score } => *score <= 1_000_000_000,
            Self::SetTeamScore { team, score } => (1..=4).contains(team) && *score <= 1_000_000,
            Self::SpawnBoss { boss } => display_text(boss, 48),
            Self::Hud { id, widget } => {
                identifier(id) && widget.as_ref().is_none_or(HudWidget::valid)
            }
            Self::MapSignal { channel } => (1..=tankyard_content::MAP_GROUPS).contains(channel),
            Self::MapCounter { counter, value, .. } => {
                (1..=tankyard_content::MAP_COUNTERS).contains(counter)
                    && (-tankyard_content::COUNTER_LIMIT..=tankyard_content::COUNTER_LIMIT)
                        .contains(value)
            }
            Self::MapGroup { group, .. } => (1..=tankyard_content::MAP_GROUPS).contains(group),
            Self::MapReset { group } => *group <= tankyard_content::MAP_GROUPS,
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
            Self::LevelAtLeast { level } => (1..=tankyard_content::MAX_LEVEL).contains(level),
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
#[derive(Clone, Debug, Default)]
pub struct ScriptPack {
    /// Entry scripts in run order.
    pub entries: Vec<String>,
    /// Every Lua file in the pack, available to `require`.
    pub sources: BTreeMap<String, String>,
    /// Art files (images, editor files, models) a script may register as
    /// textures.
    pub images: BTreeMap<String, Vec<u8>>,
}
impl ScriptPack {
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}
#[derive(Clone, Debug)]
pub struct LoadedMod {
    pub info: ModInfo,
    pub hooks: Vec<Hook>,
    pub scripts: ScriptPack,
}

/// Content a script pack declares while it is loaded. The loader validates
/// and merges it exactly like manifest definitions, in dependency order.
#[derive(Clone, Debug, Default)]
pub struct ScriptDefinitions {
    pub classes: Vec<ClassDef>,
    pub maps: Vec<MapDef>,
    pub rules: Option<GameRules>,
    pub mode_name: Option<String>,
    pub textures: Vec<TextureData>,
    pub presentation: Presentation,
}
/// Runs a script pack's definition phase. Supplied by the script runtime.
pub type DefineScripts<'a> =
    dyn FnMut(&ModInfo, &ScriptPack, &Content) -> Result<ScriptDefinitions, String> + 'a;
#[derive(Clone, Debug)]
pub struct LoadedMods {
    pub bundle: GameBundle,
    pub behaviors: Vec<LoadedMod>,
    pub rules_override: bool,
    /// Local interface styling; not part of the bundle sent to players.
    pub presentation: Presentation,
    /// The decoded `menu.background` image, also kept local.
    pub menu_background: Option<TextureData>,
}
impl Default for LoadedMods {
    fn default() -> Self {
        Self {
            bundle: GameBundle::builtin(),
            behaviors: vec![],
            rules_override: false,
            presentation: Presentation::default(),
            menu_background: None,
        }
    }
}

/// Load JSON-only packs. Script packs need [`load_with`] and a runtime.
pub fn load(paths: &[PathBuf]) -> Result<LoadedMods, String> {
    load_with(paths, None)
}

/// Read a pack's manifest and files. Used by loaders, tools and menus.
pub fn read_pack(path: &Path) -> Result<(Manifest, PackFiles), String> {
    let pack = PackFiles::read(path)?;
    let manifest = parse_manifest(&pack).map_err(|e| format!("{}: {e}", path.display()))?;
    Ok((manifest, pack))
}

pub fn parse_manifest(pack: &PackFiles) -> Result<Manifest, String> {
    let bytes = pack.get("mod.json").ok_or("missing mod.json")?;
    if bytes.len() > MAX_PACK_BYTES {
        return Err("mod.json exceeds 1 MiB".into());
    }
    let manifest: Manifest = serde_json::from_slice(bytes).map_err(|e| e.to_string())?;
    let scripts_allowed = manifest.api_version == SCRIPT_API_VERSION;
    if !(manifest.api_version == API_VERSION || scripts_allowed)
        || !identifier(&manifest.id)
        || !display_text(&manifest.name, 64)
        || !display_text(&manifest.version, 32)
        || manifest
            .description
            .as_ref()
            .is_some_and(|text| !display_text(text, 280))
        || manifest
            .author
            .as_ref()
            .is_some_and(|text| !display_text(text, 64))
        || manifest.dependencies.len() > MAX_MODS
        || manifest.dependencies.iter().any(|d| !identifier(d))
        || ((!manifest.scripts.is_empty() || !manifest.editor.is_empty()) && !scripts_allowed)
        || [&manifest.scripts, &manifest.editor]
            .into_iter()
            .any(|entries| {
                entries.len() > MAX_SCRIPTS
                    || entries.iter().collect::<HashSet<_>>().len() != entries.len()
                    || entries
                        .iter()
                        .any(|script| !script.ends_with(".lua") || !pack.contains(script))
            })
    {
        return Err("invalid, duplicate, or incompatible mod manifest".into());
    }
    if !manifest.scripts.is_empty() && !manifest.hooks.is_empty() {
        return Err("script packs define behavior in Lua; remove the JSON hooks".into());
    }
    if let Some(section) = &manifest.staff {
        staff::validate(&manifest, section, pack)?;
    }
    Ok(manifest)
}

/// Dependency order is deterministic. Script packs run only inside the
/// supplied sandboxed runtime; nothing here executes downloaded code.
pub fn load_with(
    paths: &[PathBuf],
    mut define: Option<&mut DefineScripts<'_>>,
) -> Result<LoadedMods, String> {
    if paths.len() > MAX_MODS {
        return Err("at most 16 mods can be enabled".into());
    }
    let mut manifests = Vec::with_capacity(paths.len());
    let mut ids = HashSet::new();
    for path in paths {
        let (manifest, pack) = read_pack(path)?;
        if manifest.staff.is_some() {
            return Err(format!(
                "{}: staff credential packs unlock tools in the game client and carry no game content",
                path.display()
            ));
        }
        if !ids.insert(manifest.id.clone()) {
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
            if !identifier(&source.id) || !pack_path(&source.file) {
                return Err("invalid texture file path".into());
            }
            let bytes = pack
                .get(&source.file)
                .ok_or_else(|| format!("{}: missing texture {}", path.display(), source.file))?;
            let files = |name: &str| pack.get(name).map(<[u8]>::to_vec);
            textures.push(
                decode_texture(&source.id, &source.file, bytes, &files)
                    .map_err(|e| format!("{}: {}: {e}", path.display(), source.file))?,
            );
        }
        let menu_background = match manifest.menu.as_ref().and_then(|m| m.background.as_deref()) {
            Some(file) => Some(
                menu_background(file, |name| pack.get(name).map(<[u8]>::to_vec))
                    .map_err(|e| format!("{}: {e}", path.display()))?,
            ),
            None => None,
        };
        let mut scripts = ScriptPack {
            entries: manifest.scripts.clone(),
            ..Default::default()
        };
        if !scripts.entries.is_empty() {
            for (name, data) in pack.files() {
                if name.ends_with(".lua") {
                    let text = std::str::from_utf8(data)
                        .map_err(|_| format!("{name}: Lua files must be UTF-8 text"))?;
                    scripts.sources.insert(name.to_owned(), text.to_owned());
                } else if assets::is_asset(name) || name.ends_with(".mtl") || name.ends_with(".bin")
                {
                    scripts.images.insert(name.to_owned(), data.to_vec());
                }
            }
        }
        manifests.push((manifest, textures, scripts, menu_background));
    }
    let mut result = LoadedMods::default();
    let mut installed = HashSet::new();
    while !manifests.is_empty() {
        let Some(index) = manifests
            .iter()
            .position(|(m, ..)| m.dependencies.iter().all(|d| installed.contains(d)))
        else {
            return Err("mod dependency is missing or cyclic".into());
        };
        let (manifest, textures, scripts, mut background) = manifests.remove(index);
        let mut info = ModInfo {
            id: manifest.id.clone(),
            name: manifest.name.clone(),
            version: manifest.version.clone(),
            behavior_digest: digest(
                &serde_json::to_vec(&manifest.hooks).map_err(|e| e.to_string())?,
            ),
        };
        let mut definitions = ScriptDefinitions {
            classes: manifest.classes,
            maps: manifest.maps,
            rules: manifest.rules,
            mode_name: manifest.mode_name,
            textures,
            presentation: Presentation {
                menu: manifest.menu.unwrap_or_default(),
                theme: manifest.theme.unwrap_or_default(),
                text: manifest.text,
            },
        };
        if !scripts.is_empty() {
            let Some(define) = define.as_mut() else {
                return Err(format!(
                    "{} contains Lua scripts; this tool cannot run them",
                    info.id
                ));
            };
            let scripted = define(&info, &scripts, &result.bundle.content)?;
            definitions.classes.extend(scripted.classes);
            definitions.maps.extend(scripted.maps);
            definitions.textures.extend(scripted.textures);
            if scripted.rules.is_some() {
                definitions.rules = scripted.rules;
            }
            if scripted.mode_name.is_some() {
                definitions.mode_name = scripted.mode_name;
            }
            if let Some(file) = scripted.presentation.menu.background.as_deref() {
                background = Some(
                    menu_background(file, |name| scripts.images.get(name).cloned())
                        .map_err(|e| format!("{}: {e}", info.id))?,
                );
            }
            definitions.presentation.merge(scripted.presentation);
            let mut fingerprint = info.behavior_digest;
            for (name, source) in &scripts.sources {
                fingerprint ^= digest(name.as_bytes()).rotate_left(7) ^ digest(source.as_bytes());
            }
            info.behavior_digest = fingerprint;
        }
        for texture in definitions.textures {
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
        for class in definitions.classes {
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
        for map in definitions.maps {
            if let Some(existing) = result.bundle.maps.iter_mut().find(|m| m.id == map.id) {
                *existing = map;
            } else {
                result.bundle.maps.push(map);
            }
        }
        if let Some(rules) = definitions.rules {
            rules.validate()?;
            result.bundle.rules = rules;
            result.rules_override = true;
        }
        if definitions.mode_name.is_some() {
            result.bundle.mode_name = definitions.mode_name;
        }
        result.presentation.merge(definitions.presentation);
        if background.is_some() {
            result.menu_background = background;
        }
        installed.insert(manifest.id);
        result.bundle.mods.push(info.clone());
        result.behaviors.push(LoadedMod {
            info,
            hooks: manifest.hooks,
            scripts,
        });
    }
    result.bundle.validate()?;
    result
        .presentation
        .validate(|id| result.bundle.content.class(id).is_some())?;
    for m in &result.behaviors {
        validate_hooks(&m.hooks, &result.bundle.content)?;
    }
    Ok(result)
}

/// Decode a pack's menu background image, `file` inside the pack.
fn menu_background(
    file: &str,
    read: impl Fn(&str) -> Option<Vec<u8>>,
) -> Result<TextureData, String> {
    if !pack_path(file) {
        return Err(format!("menu background {file} is not a file in this pack"));
    }
    let bytes = read(file).ok_or_else(|| format!("menu background {file} is missing"))?;
    decode_texture("menu_background", file, &bytes, &read)
        .map_err(|e| format!("menu background {file}: {e}"))
}

/// A relative path that stays inside its pack.
pub fn pack_path(path: &str) -> bool {
    path.len() <= 256
        && !path.is_empty()
        && Path::new(path)
            .components()
            .all(|part| matches!(part, std::path::Component::Normal(_)))
}

/// Decode a body skin from any supported art file, `file` inside its pack.
/// `files` reads other files in the pack that the art refers to.
pub fn decode_texture(
    id: &str,
    file: &str,
    bytes: &[u8],
    files: &assets::Files,
) -> Result<TextureData, String> {
    if !identifier(id) {
        return Err("invalid texture ID".into());
    }
    let pixels = assets::decode(file, bytes, files)?;
    Ok(TextureData {
        id: id.to_owned(),
        width: pixels.width() as u16,
        height: pixels.height() as u16,
        rgba: pixels.into_raw(),
    })
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
    read_bounded_limit(path, MAX_PACK_BYTES).map_err(|e| e.replace("2 MiB", "1 MiB"))
}
pub(crate) fn read_bounded_limit(path: &Path, limit: usize) -> Result<Vec<u8>, String> {
    let megabytes = limit / (1024 * 1024);
    if std::fs::metadata(path).map_err(|e| e.to_string())?.len() > limit as u64 {
        return Err(format!("mod file exceeds {megabytes} MiB"));
    }
    use std::io::Read;
    let file = std::fs::File::open(path).map_err(|e| e.to_string())?;
    let mut bytes = Vec::new();
    file.take((limit + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() > limit {
        return Err(format!("mod file grew beyond {megabytes} MiB"));
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
        for value in [i32::MIN, i32::MAX] {
            assert!(
                Effect::MapCounter {
                    counter: 1,
                    value,
                    add: false,
                }
                .validate(&c)
                .is_err()
            );
        }
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
