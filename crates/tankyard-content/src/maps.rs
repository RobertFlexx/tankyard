//! Original arena presets; normalized geometry respects server size overrides.
use serde::{Deserialize, Serialize};
use std::collections::{HashSet, VecDeque};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum MapProcedure {
    #[default]
    None,
    Maze,
    Dense,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SpawnRegion {
    /// Zero means an unteamed spawn region; one through four select a faction.
    pub team: u8,
    pub rect: [f32; 4],
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TeamBase {
    pub team: u8,
    pub position: [f32; 2],
}

/// The furniture look of a block. `Map` follows the arena's own style.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BlockStyle {
    #[default]
    Map,
    Slate,
    Garden,
    Circuit,
    Bastion,
    Relay,
    Orrery,
    Sand,
}
impl BlockStyle {
    pub const ALL: [Self; 8] = [
        Self::Map,
        Self::Slate,
        Self::Garden,
        Self::Circuit,
        Self::Bastion,
        Self::Relay,
        Self::Orrery,
        Self::Sand,
    ];
    pub fn name(self) -> &'static str {
        match self {
            Self::Map => "Arena default",
            Self::Slate => "Slate",
            Self::Garden => "Hedge",
            Self::Circuit => "Circuit",
            Self::Bastion => "Rampart",
            Self::Relay => "Relay",
            Self::Orrery => "Orrery",
            Self::Sand => "Sandstone",
        }
    }
}

/// A solid block: a wall with its own look, colour and group.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MapBlock {
    /// Normalized x, y, width and height, like `walls`.
    pub rect: [f32; 4],
    #[serde(default)]
    pub style: BlockStyle,
    #[serde(default)]
    pub color: Option<[u8; 3]>,
    /// Zero for none; 1 to 32 lets triggers show and hide it.
    #[serde(default)]
    pub group: u8,
}

/// What a pad does to tanks over it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ZoneKind {
    /// Faster movement.
    Speed,
    /// Slower movement.
    Mud,
    /// Health regenerates.
    Repair,
    /// Health drains; it can destroy a tank.
    Hazard,
    /// Incoming damage is reduced.
    Shelter,
}
impl ZoneKind {
    pub const ALL: [Self; 5] = [
        Self::Speed,
        Self::Mud,
        Self::Repair,
        Self::Hazard,
        Self::Shelter,
    ];
    pub fn name(self) -> &'static str {
        match self {
            Self::Speed => "Speed pad",
            Self::Mud => "Mud",
            Self::Repair => "Repair pad",
            Self::Hazard => "Hazard",
            Self::Shelter => "Shelter",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MapZone {
    pub kind: ZoneKind,
    pub rect: [f32; 4],
    /// 0.25 to 2; 1 is the standard effect.
    #[serde(default = "unit")]
    pub strength: f32,
    #[serde(default)]
    pub group: u8,
}
fn unit() -> f32 {
    1.
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DecorKind {
    /// A flat floor panel.
    #[default]
    Tile,
    /// Diagonal warning stripes.
    Stripes,
    /// A soft round glow.
    Light,
    /// An outlined circle.
    Ring,
    /// A floor arrow, pointing along `rotation`.
    Arrow,
    /// Painted text.
    Label,
    /// A particle emitter; its look is `effect`.
    Emitter,
}
impl DecorKind {
    pub const ALL: [Self; 7] = [
        Self::Tile,
        Self::Stripes,
        Self::Light,
        Self::Ring,
        Self::Arrow,
        Self::Label,
        Self::Emitter,
    ];
    pub fn name(self) -> &'static str {
        match self {
            Self::Tile => "Floor tile",
            Self::Stripes => "Stripes",
            Self::Light => "Light",
            Self::Ring => "Ring",
            Self::Arrow => "Arrow",
            Self::Label => "Text",
            Self::Emitter => "Particles",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ParticleShape {
    #[default]
    Dot,
    Square,
    Spark,
    Ring,
    Triangle,
}
impl ParticleShape {
    pub const ALL: [Self; 5] = [
        Self::Dot,
        Self::Square,
        Self::Spark,
        Self::Ring,
        Self::Triangle,
    ];
}

/// A particle effect, drawn by clients only. Every field has a bounded range
/// so a map cannot ask for more particles than a client can afford.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ParticleEffect {
    pub shape: ParticleShape,
    /// Particles per second, 0.2 to 60.
    pub rate: f32,
    /// Seconds each particle lives, 0.1 to 8.
    pub lifetime: f32,
    /// Launch speed in world units per second, 0 to 800.
    pub speed: f32,
    /// Launch direction in degrees, 0 pointing right, 90 down.
    pub direction: f32,
    /// Spread around the direction in degrees, 0 to 360.
    pub spread: f32,
    /// Constant downward pull, -800 to 800; negative floats upward.
    pub gravity: f32,
    /// Speed lost per second as a fraction, 0 to 8.
    pub drag: f32,
    /// Start and end size in world units, 1 to 120.
    pub size: f32,
    pub end_size: f32,
    pub color: [u8; 3],
    pub end_color: [u8; 3],
    /// Fade out over the particle's life.
    pub fade: bool,
    /// Spin in degrees per second, -1080 to 1080.
    pub spin: f32,
}
impl Default for ParticleEffect {
    fn default() -> Self {
        Self {
            shape: ParticleShape::Dot,
            rate: 8.,
            lifetime: 1.6,
            speed: 60.,
            direction: 270.,
            spread: 50.,
            gravity: 0.,
            drag: 0.6,
            size: 9.,
            end_size: 3.,
            color: [255, 200, 90],
            end_color: [230, 90, 60],
            fade: true,
            spin: 0.,
        }
    }
}
impl ParticleEffect {
    pub fn validate(&self) -> Result<(), &'static str> {
        if range(self.rate, 0.2, 60.)
            && range(self.lifetime, 0.1, 8.)
            && range(self.speed, 0., 800.)
            && self.direction.is_finite()
            && range(self.spread, 0., 360.)
            && range(self.gravity, -800., 800.)
            && range(self.drag, 0., 8.)
            && range(self.size, 1., 120.)
            && range(self.end_size, 0., 120.)
            && range(self.spin, -1080., 1080.)
            && self.rate * self.lifetime <= 240.
        {
            Ok(())
        } else {
            Err("particle effect values are out of range")
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MapDecor {
    pub kind: DecorKind,
    pub rect: [f32; 4],
    #[serde(default = "decor_color")]
    pub color: [u8; 3],
    /// 0 is invisible, 255 opaque.
    #[serde(default = "opaque")]
    pub alpha: u8,
    /// Degrees; arrows point this way and text and stripes turn with it.
    #[serde(default)]
    pub rotation: f32,
    /// Brightness pulses per second, 0 to 4.
    #[serde(default)]
    pub pulse: f32,
    /// Degrees per second, -720 to 720.
    #[serde(default)]
    pub spin: f32,
    /// Drawn above tanks instead of on the floor.
    #[serde(default)]
    pub above: bool,
    #[serde(default)]
    pub text: String,
    #[serde(default)]
    pub effect: Option<ParticleEffect>,
    #[serde(default)]
    pub group: u8,
}
fn decor_color() -> [u8; 3] {
    [120, 160, 190]
}
fn opaque() -> u8 {
    255
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TriggerWhen {
    /// Repeats every `seconds`, starting `offset` seconds into the round.
    #[default]
    Every,
    /// When a living tank enters `rect`; `seconds` is the cooldown.
    Enter,
    /// Once, `seconds` after the round starts.
    Start,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TriggerAction {
    #[default]
    Toggle,
    Show,
    Hide,
    /// Shows `text` to everyone in the arena.
    Announce,
    /// Brings the `boss` named boss into the arena at `rect`.
    Boss,
}

/// A rule that runs during a round: GD-style toggles, timed gates, hazards
/// that come and go, announcements and boss arrivals.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MapTrigger {
    pub when: TriggerWhen,
    pub action: TriggerAction,
    /// The area an Enter trigger watches; where a boss arrives. Also where
    /// the editor shows the trigger.
    pub rect: [f32; 4],
    /// The group Toggle, Show and Hide act on.
    #[serde(default)]
    pub group: u8,
    #[serde(default)]
    pub seconds: f32,
    #[serde(default)]
    pub offset: f32,
    /// When above zero, the change reverts after this many seconds.
    #[serde(default)]
    pub duration: f32,
    #[serde(default)]
    pub text: String,
    #[serde(default)]
    pub boss: String,
    /// Fires at most once a round.
    #[serde(default)]
    pub once: bool,
}

/// Most groups a map can name; they fit one 32-bit mask in snapshots.
pub const MAP_GROUPS: u8 = 32;
/// Stable IDs accepted by map boss triggers. Core verifies this catalog stays
/// in sync with its boss enum.
pub const BOSS_IDS: [&str; 14] = [
    "sentinel",
    "prism",
    "foundry",
    "tempest",
    "bastion",
    "eclipse",
    "guardian",
    "summoner",
    "defender",
    "fallen_overlord",
    "fallen_booster",
    "siege_engine",
    "skirmisher",
    "trapper",
];

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MapDef {
    pub id: String,
    pub name: String,
    pub description: String,
    /// Suggested dimensions. Explicit server arena dimensions take precedence.
    pub width: f32,
    pub height: f32,
    /// Recommended capacity; the server's player limit remains authoritative.
    pub capacity: u16,
    pub recommended_modes: Vec<String>,
    /// Normalized x, y, width and height, each within the unit arena rectangle.
    pub walls: Vec<[f32; 4]>,
    pub procedural: MapProcedure,
    pub objective_positions: Vec<[f32; 2]>,
    pub spawn_regions: Vec<SpawnRegion>,
    pub team_bases: Vec<TeamBase>,
    pub background: String,
    /// The look of `walls` and of blocks left at `Map`. `Map` picks one from
    /// the arena's id, as built-in arenas do.
    #[serde(default)]
    pub style: BlockStyle,
    #[serde(default)]
    pub blocks: Vec<MapBlock>,
    #[serde(default)]
    pub zones: Vec<MapZone>,
    #[serde(default)]
    pub decor: Vec<MapDecor>,
    #[serde(default)]
    pub triggers: Vec<MapTrigger>,
    /// Groups hidden when a round starts, such as gates a trigger opens.
    #[serde(default)]
    pub hidden_groups: Vec<u8>,
}
impl MapDef {
    /// The bit of `group` in a hidden-group mask; group 0 has none.
    pub fn group_bit(group: u8) -> u32 {
        if (1..=MAP_GROUPS).contains(&group) {
            1 << (group - 1)
        } else {
            0
        }
    }
    /// Groups hidden at the start of a round, as a mask.
    pub fn starting_mask(&self) -> u32 {
        self.hidden_groups
            .iter()
            .fold(0, |mask, &group| mask | Self::group_bit(group))
    }
    /// Every solid rectangle (normalized) while the groups in `hidden` are
    /// hidden: plain walls first, then blocks in order.
    pub fn solid_rects(&self, hidden: u32) -> impl Iterator<Item = [f32; 4]> + '_ {
        self.walls.iter().copied().chain(
            self.blocks
                .iter()
                .filter(move |block| Self::group_bit(block.group) & hidden == 0)
                .map(|block| block.rect),
        )
    }
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct MapCatalog {
    maps: Vec<MapDef>,
}

pub fn builtin_maps() -> Vec<MapDef> {
    maps_from_json(include_str!("../../../assets/content/maps.json"))
        .expect("compiled-in arena definitions must be valid")
}

pub fn maps_from_json(source: &str) -> Result<Vec<MapDef>, String> {
    let catalog: MapCatalog =
        serde_json::from_str(source).map_err(|error| format!("invalid map JSON: {error}"))?;
    validate_maps(&catalog.maps)?;
    Ok(catalog.maps)
}

pub fn validate_maps(maps: &[MapDef]) -> Result<(), String> {
    if maps.is_empty() || maps.len() > 256 {
        return Err("a map catalog needs between 1 and 256 arenas".into());
    }
    // Wire size alone does not bound geometric work: a compact catalog can
    // request hundreds of large navigation grids against thousands of walls.
    // Reject the whole catalog before starting any connectivity traversal.
    let mut geometry_work = 0_u64;
    for map in maps {
        if !range(map.width, 900., 16000.)
            || !range(map.height, 700., 16000.)
            || map.walls.len() + map.blocks.len() > 2048
        {
            return Err(format!(
                "{}: invalid dimensions or excessive geometry",
                map.id
            ));
        }
        geometry_work += (map.width / 80.).ceil() as u64
            * (map.height / 80.).ceil() as u64
            * (map.walls.len() + map.blocks.len()).max(1) as u64;
        if geometry_work > 16_000_000 {
            return Err("map catalog exceeds the geometry validation budget".into());
        }
    }
    let mut ids = HashSet::new();
    for map in maps {
        if map.id.is_empty()
            || map.id.len() > 48
            || !map
                .id
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
            || !ids.insert(map.id.as_str())
        {
            return Err(format!("invalid or duplicate map identifier {}", map.id));
        }
        if map.name.trim().is_empty()
            || map.name.len() > 64
            || map.name.chars().any(char::is_control)
            || map.description.trim().is_empty()
            || map.description.len() > 2048
            || map.description.chars().any(char::is_control)
        {
            return Err(format!("{}: invalid arena display text", map.id));
        }
        if !range(map.width, 900., 16000.)
            || !range(map.height, 700., 16000.)
            || !(2..=128).contains(&map.capacity)
        {
            return Err(format!("{}: invalid dimensions or capacity", map.id));
        }
        if map.recommended_modes.is_empty() || map.recommended_modes.len() > 14 {
            return Err(format!(
                "{}: missing or oversized mode recommendations",
                map.id
            ));
        }
        let mut modes = HashSet::new();
        for mode in &map.recommended_modes {
            let canonical = mode.replace('-', "_");
            if !matches!(
                canonical.as_str(),
                "ffa"
                    | "two_teams"
                    | "four_teams"
                    | "maze"
                    | "domination"
                    | "survival"
                    | "tag"
                    | "infection"
                    | "king_of_the_hill"
                    | "mothership"
                    | "capture_core"
                    | "boss_rush"
                    | "deathmatch"
                    | "sandbox"
                    | "random_events"
                    | "coop_pve"
            ) || !modes.insert(canonical)
            {
                return Err(format!(
                    "{}: invalid or duplicate mode reference {mode}",
                    map.id
                ));
            }
        }
        if map.walls.len() > 2048
            || map.objective_positions.len() > 32
            || map.spawn_regions.len() > 32
            || map.team_bases.len() > 4
        {
            return Err(format!("{}: excessive arena geometry", map.id));
        }
        for &wall in &map.walls {
            validate_rect(wall).map_err(|error| format!("{} wall: {error}", map.id))?;
        }
        validate_map_features(map)?;
        for &position in &map.objective_positions {
            validate_point(position).map_err(|error| format!("{} objective: {error}", map.id))?;
            if !point_clear(position[0] * map.width, position[1] * map.height, map, 42.) {
                return Err(format!("{}: objective overlaps a wall or boundary", map.id));
            }
        }
        if map.spawn_regions.is_empty() {
            return Err(format!("{}: arena needs a spawn region", map.id));
        }
        for region in &map.spawn_regions {
            validate_rect(region.rect).map_err(|error| format!("{} spawn: {error}", map.id))?;
            let [x, y, w, h] = region.rect;
            if region.team > 4
                || w * map.width < 80.
                || h * map.height < 80.
                || !point_clear(
                    (x + w * 0.5) * map.width,
                    (y + h * 0.5) * map.height,
                    map,
                    37.,
                )
            {
                return Err(format!(
                    "{}: spawn region lacks hull clearance or has invalid team",
                    map.id
                ));
            }
        }
        let mut teams = HashSet::new();
        for base in &map.team_bases {
            validate_point(base.position).map_err(|error| format!("{} base: {error}", map.id))?;
            if !(1..=4).contains(&base.team) || !teams.insert(base.team) {
                return Err(format!("{}: invalid or duplicate team base", map.id));
            }
            if !point_clear(
                base.position[0] * map.width,
                base.position[1] * map.height,
                map,
                75.,
            ) {
                return Err(format!("{}: team base lacks clearance", map.id));
            }
            if !map.spawn_regions.iter().any(|region| {
                let [x, y, w, h] = region.rect;
                region.team == base.team
                    && base.position[0] >= x
                    && base.position[0] <= x + w
                    && base.position[1] >= y
                    && base.position[1] <= y + h
            }) {
                return Err(format!(
                    "{}: team base has no matching spawn region",
                    map.id
                ));
            }
        }
        if !matches!(
            map.background.as_str(),
            "slate" | "teal" | "sand" | "violet"
        ) {
            return Err(format!(
                "{}: unknown background palette {}",
                map.id, map.background
            ));
        }
        // Procedural generators validate their generated geometry in core. This
        // pass verifies the complete static geometry declared by every preset.
        validate_connectivity(map)?;
    }
    Ok(())
}

/// Blocks, pads, decor, triggers and groups: counts, ranges and text.
pub fn validate_map_features(map: &MapDef) -> Result<(), String> {
    let id = &map.id;
    if map.zones.len() > 256
        || map.decor.len() > 1024
        || map.triggers.len() > 128
        || map.hidden_groups.len() > usize::from(MAP_GROUPS)
    {
        return Err(format!("{id}: too many pads, decorations or triggers"));
    }
    let group = |group: u8| group <= MAP_GROUPS;
    for block in &map.blocks {
        validate_rect(block.rect).map_err(|error| format!("{id} block: {error}"))?;
        if !group(block.group) {
            return Err(format!("{id}: block group must be 0 to {MAP_GROUPS}"));
        }
    }
    for zone in &map.zones {
        validate_rect(zone.rect).map_err(|error| format!("{id} pad: {error}"))?;
        if !range(zone.strength, 0.25, 2.) || !group(zone.group) {
            return Err(format!(
                "{id}: pad strength must be 0.25 to 2 and its group 0 to {MAP_GROUPS}"
            ));
        }
    }
    let mut emitters = 0;
    for decor in &map.decor {
        validate_rect(decor.rect).map_err(|error| format!("{id} decoration: {error}"))?;
        if !decor.rotation.is_finite()
            || !range(decor.pulse, 0., 4.)
            || !range(decor.spin, -720., 720.)
            || !group(decor.group)
            || !display_text(&decor.text, 64)
        {
            return Err(format!("{id}: decoration settings are out of range"));
        }
        if let Some(effect) = &decor.effect {
            effect.validate().map_err(|e| format!("{id}: {e}"))?;
        }
        if decor.kind == DecorKind::Emitter {
            emitters += 1;
            decor
                .effect
                .unwrap_or_default()
                .validate()
                .map_err(|error| format!("{id}: {error}"))?;
        }
    }
    if emitters > 48 {
        return Err(format!("{id}: at most 48 particle emitters"));
    }
    for trigger in &map.triggers {
        validate_rect(trigger.rect).map_err(|error| format!("{id} trigger: {error}"))?;
        let timing = match trigger.when {
            TriggerWhen::Every => range(trigger.seconds, 0.5, 3600.),
            TriggerWhen::Enter => range(trigger.seconds, 0., 3600.),
            TriggerWhen::Start => range(trigger.seconds, 0., 86400.),
        };
        let target = match trigger.action {
            TriggerAction::Toggle | TriggerAction::Show | TriggerAction::Hide => {
                (1..=MAP_GROUPS).contains(&trigger.group)
            }
            TriggerAction::Announce => !trigger.text.trim().is_empty(),
            TriggerAction::Boss => BOSS_IDS.contains(&trigger.boss.as_str()),
        };
        if !timing
            || !target
            || !range(trigger.offset, 0., 3600.)
            || !range(trigger.duration, 0., 3600.)
            || !display_text(&trigger.text, 96)
        {
            return Err(format!(
                "{id}: a trigger has invalid timing, target or text"
            ));
        }
    }
    if map
        .hidden_groups
        .iter()
        .any(|&g| !(1..=MAP_GROUPS).contains(&g))
    {
        return Err(format!("{id}: hidden groups must be 1 to {MAP_GROUPS}"));
    }
    Ok(())
}

fn display_text(text: &str, limit: usize) -> bool {
    text.chars().count() <= limit && !text.chars().any(char::is_control)
}

fn range(value: f32, min: f32, max: f32) -> bool {
    value.is_finite() && value >= min && value <= max
}

fn validate_point(position: [f32; 2]) -> Result<(), &'static str> {
    if position.into_iter().all(|v| range(v, 0., 1.)) {
        Ok(())
    } else {
        Err("positions must be finite normalized coordinates")
    }
}

fn validate_rect([x, y, w, h]: [f32; 4]) -> Result<(), &'static str> {
    if range(x, 0., 1.)
        && range(y, 0., 1.)
        && range(w, 0.001, 1.)
        && range(h, 0.001, 1.)
        && x + w <= 1.00001
        && y + h <= 1.00001
    {
        Ok(())
    } else {
        Err("rectangles must be finite, positive and contained in the unit arena")
    }
}

fn point_clear(x: f32, y: f32, map: &MapDef, radius: f32) -> bool {
    if x < radius || y < radius || x > map.width - radius || y > map.height - radius {
        return false;
    }
    map.solid_rects(map.starting_mask()).all(|[wx, wy, w, h]| {
        let (wx, wy, w, h) = (
            wx * map.width,
            wy * map.height,
            w * map.width,
            h * map.height,
        );
        let nearest_x = x.clamp(wx, wx + w);
        let nearest_y = y.clamp(wy, wy + h);
        (x - nearest_x).powi(2) + (y - nearest_y).powi(2) >= radius * radius
    })
}

fn segment_clear(a: [f32; 2], b: [f32; 2], map: &MapDef) -> bool {
    map.solid_rects(map.starting_mask()).all(|[x, y, w, h]| {
        let mins = [x * map.width - 30., y * map.height - 30.];
        let maxs = [(x + w) * map.width + 30., (y + h) * map.height + 30.];
        let mut entry: f32 = 0.;
        let mut exit: f32 = 1.;
        for axis in 0..2 {
            let delta = b[axis] - a[axis];
            if delta.abs() < 0.001 {
                if a[axis] < mins[axis] || a[axis] > maxs[axis] {
                    return true;
                }
            } else {
                let first = (mins[axis] - a[axis]) / delta;
                let second = (maxs[axis] - a[axis]) / delta;
                entry = entry.max(first.min(second));
                exit = exit.min(first.max(second));
                if entry > exit {
                    return true;
                }
            }
        }
        false
    })
}

fn validate_connectivity(map: &MapDef) -> Result<(), String> {
    let cols = (map.width / 80.).ceil() as usize;
    let rows = (map.height / 80.).ceil() as usize;
    let center = |index: usize| {
        [
            (index % cols) as f32 * 80. + 40.,
            (index / cols) as f32 * 80. + 40.,
        ]
    };
    let free = (0..cols * rows)
        .map(|index| {
            let [x, y] = center(index);
            // Keep samples strictly outside the link test's expanded walls.
            // Exact tangency must not create an isolated line of "free" cells.
            point_clear(x, y, map, 37.)
        })
        .collect::<Vec<_>>();
    let Some(start) = free.iter().position(|&valid| valid) else {
        return Err(format!("{}: no navigable arena cells", map.id));
    };
    let mut visited = vec![false; free.len()];
    let mut queue = VecDeque::from([start]);
    visited[start] = true;
    while let Some(index) = queue.pop_front() {
        let (x, y) = (index % cols, index / cols);
        for (nx, ny) in [
            (x.wrapping_sub(1), y),
            (x + 1, y),
            (x, y.wrapping_sub(1)),
            (x, y + 1),
        ] {
            if nx >= cols || ny >= rows {
                continue;
            }
            let next = ny * cols + nx;
            if !visited[next] && free[next] && segment_clear(center(index), center(next), map) {
                visited[next] = true;
                queue.push_back(next);
            }
        }
    }
    if free
        .iter()
        .zip(&visited)
        .any(|(&free, &seen)| free && !seen)
    {
        return Err(format!(
            "{}: static arena has disconnected navigable regions",
            map.id
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compact_catalogs_cannot_request_unbounded_geometry_validation_work() {
        let mut map = builtin_maps().remove(0);
        map.width = 16000.;
        map.height = 16000.;
        map.walls = vec![[0.45, 0.45, 0.001, 0.001]; 2048];
        assert!(
            validate_maps(&[map.clone()])
                .unwrap_err()
                .contains("budget")
        );
        map.walls.truncate(64);
        let maps: Vec<_> = (0..8)
            .map(|index| {
                let mut copy = map.clone();
                copy.id = format!("arena_{index}");
                copy
            })
            .collect();
        assert!(validate_maps(&maps).unwrap_err().contains("budget"));
    }

    #[test]
    fn all_eight_arenas_have_valid_clear_connected_geometry() {
        let maps = builtin_maps();
        assert_eq!(maps.len(), 8);
        validate_maps(&maps).unwrap();
        assert!(maps.iter().any(|map| map.procedural == MapProcedure::Maze));
        assert!(maps.iter().any(|map| map.procedural == MapProcedure::Dense));
        assert_eq!(
            maps.iter()
                .find(|map| map.id == "domination")
                .unwrap()
                .objective_positions
                .len(),
            3
        );
    }

    #[test]
    fn malformed_references_geometry_and_sealed_maps_fail() {
        let mut maps = builtin_maps();
        maps[0].recommended_modes = vec!["imaginary".into()];
        assert!(validate_maps(&maps).is_err());
        let mut maps = builtin_maps();
        maps[0].width = f32::INFINITY;
        assert!(validate_maps(&maps).is_err());
        let mut maps = builtin_maps();
        maps[0].walls = vec![[0.49, 0., 0.02, 1.]];
        maps[0].spawn_regions[0].rect = [0.02, 0.02, 0.2, 0.2];
        assert!(validate_maps(&maps).unwrap_err().contains("disconnected"));
        let mut maps = builtin_maps();
        maps.push(maps[0].clone());
        assert!(validate_maps(&maps).unwrap_err().contains("duplicate"));
    }

    #[test]
    fn map_catalog_roundtrips_through_the_public_loader() {
        let catalog = MapCatalog {
            maps: builtin_maps(),
        };
        let json = serde_json::to_string(&catalog).unwrap();
        let maps = maps_from_json(&json).unwrap();
        assert_eq!(maps.len(), 8);
    }
}
