use crate::{Rect, V2};
use serde::{Deserialize, Serialize};
use tankyard_content::ProjectileKind;
pub type EntityId = u64;
pub const TICK_RATE: u32 = 60;
pub const DT: f32 = 1.0 / 60.0;
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Mode {
    Ffa,
    TwoTeams,
    FourTeams,
    Maze,
    Domination,
    Survival,
    Tag,
    Mothership,
    CaptureCore,
    BossRush,
    Deathmatch,
    Sandbox,
    RandomEvents,
    CoopPve,
}
impl Mode {
    pub const ALL: [Self; 14] = [
        Self::Ffa,
        Self::TwoTeams,
        Self::FourTeams,
        Self::Maze,
        Self::Domination,
        Self::Survival,
        Self::Tag,
        Self::Mothership,
        Self::CaptureCore,
        Self::BossRush,
        Self::Deathmatch,
        Self::Sandbox,
        Self::RandomEvents,
        Self::CoopPve,
    ];
    pub fn name(self) -> &'static str {
        match self {
            Self::Ffa => "Free For All",
            Self::TwoTeams => "Two Teams",
            Self::FourTeams => "Four Teams",
            Self::Maze => "Maze",
            Self::Domination => "Domination",
            Self::Survival => "Survival",
            Self::Tag => "Tag",
            Self::Mothership => "Mothership",
            Self::CaptureCore => "Capture the Core",
            Self::BossRush => "Boss Rush",
            Self::Deathmatch => "Team Deathmatch",
            Self::Sandbox => "Sandbox",
            Self::RandomEvents => "Random Events",
            Self::CoopPve => "Co-op PvE",
        }
    }
    pub fn teams(self) -> u8 {
        match self {
            Self::FourTeams => 4,
            Self::TwoTeams
            | Self::Domination
            | Self::Tag
            | Self::Mothership
            | Self::CaptureCore
            | Self::Deathmatch => 2,
            Self::BossRush | Self::CoopPve => 1,
            _ => 0,
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct WorldConfig {
    #[serde(default)]
    pub rules: tankyard_mods::GameRules,
    #[serde(default)]
    pub modded: bool,
    #[serde(default)]
    pub mode_name: Option<String>,
    pub width: f32,
    pub height: f32,
    pub mode: Mode,
    pub seed: u64,
    pub target_count: usize,
    pub max_projectiles: usize,
    pub friendly_fire: bool,
    pub spawn_protection: f32,
    pub score_limit: u32,
    pub time_limit: f32,
    pub lives: u8,
    pub map: String,
}
impl Default for WorldConfig {
    fn default() -> Self {
        Self {
            rules: tankyard_mods::GameRules::default(),
            modded: false,
            mode_name: None,
            width: 3600.,
            height: 2600.,
            mode: Mode::Ffa,
            seed: 42,
            target_count: 180,
            max_projectiles: 6000,
            friendly_fire: false,
            spawn_protection: 15.,
            score_limit: 100,
            time_limit: 600.,
            lives: 1,
            map: "open".into(),
        }
    }
}
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct PlayerInput {
    pub sequence: u32,
    pub movement: V2,
    pub aim: f32,
    pub aim_distance: f32,
    pub fire: bool,
    pub alt_fire: bool,
    pub ability: bool,
}
impl Default for PlayerInput {
    fn default() -> Self {
        Self {
            sequence: 0,
            movement: V2::ZERO,
            aim: 0.,
            aim_distance: 500.,
            fire: false,
            alt_fire: false,
            ability: false,
        }
    }
}
impl PlayerInput {
    pub fn valid(self) -> bool {
        self.movement.finite()
            && self.aim.is_finite()
            && self.aim_distance.is_finite()
            && (0.0..=5000.0).contains(&self.aim_distance)
            && self.movement.length_squared() <= 2.01
            && self.aim.abs() < 10000.
    }
}
#[derive(Debug, Serialize, Deserialize)]
pub struct Tank {
    pub id: EntityId,
    pub name: String,
    pub pos: V2,
    pub vel: V2,
    pub aim: f32,
    pub radius: f32,
    pub health: f32,
    pub max_health: f32,
    pub xp: u32,
    pub score: u32,
    pub level: u8,
    pub stats: [u8; 8],
    pub points: u8,
    pub class: String,
    pub team: u8,
    pub bot: bool,
    pub alive: bool,
    pub respawn_timer: f32,
    pub protection: f32,
    pub shield: f32,
    pub ability_cooldown: f32,
    pub last_input: u32,
    pub kills: u32,
    pub deaths: u32,
    pub lives: u8,
    pub input: PlayerInput,
    pub cooldowns: Vec<f32>,
    pub shot_count: u32,
    pub damaged_at: u64,
    pub last_attacker: EntityId,
    pub charge: f32,
    pub boost: f32,
    #[serde(default)]
    pub barrel_shots: Vec<u32>,
    #[serde(default)]
    pub barrel_aims: Vec<f32>,
    #[serde(default)]
    pub carrying: bool,
    #[serde(default)]
    pub deployed_drones: u8,
    #[serde(default)]
    pub spawn_immunity: bool,
}
// Reuse owned buffers when refreshing native client snapshots.
impl Clone for Tank {
    fn clone(&self) -> Self {
        Self {
            id: self.id,
            name: self.name.clone(),
            pos: self.pos,
            vel: self.vel,
            aim: self.aim,
            radius: self.radius,
            health: self.health,
            max_health: self.max_health,
            xp: self.xp,
            score: self.score,
            level: self.level,
            stats: self.stats,
            points: self.points,
            class: self.class.clone(),
            team: self.team,
            bot: self.bot,
            alive: self.alive,
            respawn_timer: self.respawn_timer,
            protection: self.protection,
            shield: self.shield,
            ability_cooldown: self.ability_cooldown,
            last_input: self.last_input,
            kills: self.kills,
            deaths: self.deaths,
            lives: self.lives,
            input: self.input,
            cooldowns: self.cooldowns.clone(),
            shot_count: self.shot_count,
            damaged_at: self.damaged_at,
            last_attacker: self.last_attacker,
            charge: self.charge,
            boost: self.boost,
            barrel_shots: self.barrel_shots.clone(),
            barrel_aims: self.barrel_aims.clone(),
            carrying: self.carrying,
            deployed_drones: self.deployed_drones,
            spawn_immunity: self.spawn_immunity,
        }
    }
    fn clone_from(&mut self, source: &Self) {
        self.id.clone_from(&source.id);
        self.name.clone_from(&source.name);
        self.pos.clone_from(&source.pos);
        self.vel.clone_from(&source.vel);
        self.aim.clone_from(&source.aim);
        self.radius.clone_from(&source.radius);
        self.health.clone_from(&source.health);
        self.max_health.clone_from(&source.max_health);
        self.xp.clone_from(&source.xp);
        self.score.clone_from(&source.score);
        self.level.clone_from(&source.level);
        self.stats.clone_from(&source.stats);
        self.points.clone_from(&source.points);
        self.class.clone_from(&source.class);
        self.team.clone_from(&source.team);
        self.bot.clone_from(&source.bot);
        self.alive.clone_from(&source.alive);
        self.respawn_timer.clone_from(&source.respawn_timer);
        self.protection.clone_from(&source.protection);
        self.shield.clone_from(&source.shield);
        self.ability_cooldown.clone_from(&source.ability_cooldown);
        self.last_input.clone_from(&source.last_input);
        self.kills.clone_from(&source.kills);
        self.deaths.clone_from(&source.deaths);
        self.lives.clone_from(&source.lives);
        self.input.clone_from(&source.input);
        self.cooldowns.clone_from(&source.cooldowns);
        self.shot_count.clone_from(&source.shot_count);
        self.damaged_at.clone_from(&source.damaged_at);
        self.last_attacker.clone_from(&source.last_attacker);
        self.charge.clone_from(&source.charge);
        self.boost.clone_from(&source.boost);
        self.barrel_shots.clone_from(&source.barrel_shots);
        self.barrel_aims.clone_from(&source.barrel_aims);
        self.carrying.clone_from(&source.carrying);
        self.deployed_drones.clone_from(&source.deployed_drones);
        self.spawn_immunity.clone_from(&source.spawn_immunity);
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct MinionGun {
    pub aim: f32,
    pub cooldown: f32,
    pub shots: u32,
}
impl MinionGun {
    pub const BARREL_LENGTH: f32 = 1.8;
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Projectile {
    pub id: EntityId,
    pub owner: EntityId,
    pub team: u8,
    pub pos: V2,
    pub previous: V2,
    pub vel: V2,
    pub cruise_speed: f32,
    pub radius: f32,
    pub damage: f32,
    pub penetration: f32,
    pub lifetime: f32,
    pub kind: ProjectileKind,
    pub ricochets: u8,
    pub age: f32,
    pub hit: Vec<EntityId>,
    #[serde(default)]
    pub gun: Option<MinionGun>,
    #[serde(default)]
    pub homing_target: EntityId,
}
// Reuse owned buffers when refreshing native client snapshots.
impl Clone for Projectile {
    fn clone(&self) -> Self {
        Self {
            id: self.id,
            owner: self.owner,
            team: self.team,
            pos: self.pos,
            previous: self.previous,
            vel: self.vel,
            cruise_speed: self.cruise_speed,
            radius: self.radius,
            damage: self.damage,
            penetration: self.penetration,
            lifetime: self.lifetime,
            kind: self.kind,
            ricochets: self.ricochets,
            age: self.age,
            hit: self.hit.clone(),
            gun: self.gun,
            homing_target: self.homing_target,
        }
    }
    fn clone_from(&mut self, source: &Self) {
        self.id.clone_from(&source.id);
        self.owner.clone_from(&source.owner);
        self.team.clone_from(&source.team);
        self.pos.clone_from(&source.pos);
        self.previous.clone_from(&source.previous);
        self.vel.clone_from(&source.vel);
        self.cruise_speed.clone_from(&source.cruise_speed);
        self.radius.clone_from(&source.radius);
        self.damage.clone_from(&source.damage);
        self.penetration.clone_from(&source.penetration);
        self.lifetime.clone_from(&source.lifetime);
        self.kind.clone_from(&source.kind);
        self.ricochets.clone_from(&source.ricochets);
        self.age.clone_from(&source.age);
        self.hit.clone_from(&source.hit);
        self.gun.clone_from(&source.gun);
        self.homing_target.clone_from(&source.homing_target);
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum TargetKind {
    Square,
    Triangle,
    Pentagon,
    Fortress,
    Rare,
    Mobile,
    Reinforced,
    Custom(u8),
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Target {
    pub id: EntityId,
    pub pos: V2,
    pub vel: V2,
    pub radius: f32,
    pub health: f32,
    pub max_health: f32,
    pub kind: TargetKind,
    pub angle: f32,
    pub xp: u32,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum BossKind {
    Sentinel,
    Prism,
    Foundry,
    Tempest,
    Bastion,
    Eclipse,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Boss {
    pub id: EntityId,
    pub pos: V2,
    pub vel: V2,
    pub radius: f32,
    pub health: f32,
    pub max_health: f32,
    pub kind: BossKind,
    pub phase: u8,
    pub timer: f32,
    pub aim: f32,
    pub team: u8,
    pub mothership: bool,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Objective {
    pub id: EntityId,
    pub pos: V2,
    pub radius: f32,
    pub team: u8,
    pub progress: f32,
    pub contested: bool,
    pub carrier: Option<EntityId>,
    pub home: V2,
    pub return_timer: f32,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum ArenaEvent {
    #[default]
    None,
    Overdrive,
    DoublePoints,
    Regeneration,
    ShapeRush,
}
impl ArenaEvent {
    pub const ALL: [Self; 4] = [
        Self::Overdrive,
        Self::DoublePoints,
        Self::Regeneration,
        Self::ShapeRush,
    ];
    pub fn name(self) -> &'static str {
        match self {
            Self::None => "Next event",
            Self::Overdrive => "Overdrive",
            Self::DoublePoints => "Double points",
            Self::Regeneration => "Regeneration",
            Self::ShapeRush => "Shape rush",
        }
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct ArenaStatus {
    pub event: ArenaEvent,
    pub remaining: f32,
    pub wave: u8,
    pub enemies: u16,
    pub intermission: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MatchState {
    pub elapsed: f32,
    pub scores: [u32; 4],
    pub winner: Option<u8>,
    pub finished: bool,
    pub round: u32,
    pub boss_wave: u8,
    pub safe_radius: f32,
    pub announcement: String,
    #[serde(default)]
    pub arena: ArenaStatus,
}
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub enum EventKind {
    Shot,
    Impact,
    Destroy,
    LevelUp,
    Evolve,
    Ability,
    Capture,
    Victory,
    BossSpawn,
    Kill,
    Parry,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GameEvent {
    pub tick: u64,
    pub kind: EventKind,
    pub pos: V2,
    pub subject: EntityId,
    pub value: u32,
    pub team: u8,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Snapshot {
    pub tick: u64,
    pub config: WorldConfig,
    pub tanks: Vec<Tank>,
    pub projectiles: Vec<Projectile>,
    pub targets: Vec<Target>,
    pub bosses: Vec<Boss>,
    pub walls: Vec<Rect>,
    pub objectives: Vec<Objective>,
    pub match_state: MatchState,
    pub events: Vec<GameEvent>,
}
