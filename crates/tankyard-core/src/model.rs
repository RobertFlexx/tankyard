use crate::railgunner::{RAIL_RADIUS, RailWork};
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
    Infection,
    KingOfTheHill,
}
impl Mode {
    pub const ALL: [Self; 16] = [
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
        Self::Infection,
        Self::KingOfTheHill,
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
            Self::Infection => "Infection",
            Self::KingOfTheHill => "King of the Hill",
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
            | Self::Deathmatch
            | Self::Infection
            | Self::KingOfTheHill => 2,
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
    #[serde(default = "enabled")]
    pub dynamic_arena: bool,
    /// Authoritative animated playable bounds inside the configured map.
    #[serde(default = "unit_scale")]
    pub arena_scale: f32,
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
    /// First entity ID of this world. Arenas sharing one server use disjoint
    /// ranges so player IDs stay unambiguous. Host-only; never serialized.
    #[serde(skip)]
    pub id_base: u64,
    /// The bosses a Boss Rush fights, in order; empty plays [`CLASSIC_RUSH`].
    /// Host-only: clients follow the wave from the match state.
    #[serde(skip)]
    pub boss_rush: Vec<BossKind>,
}
/// The bosses a Boss Rush fights unless the host picks its own.
pub const CLASSIC_RUSH: [BossKind; 6] = [
    BossKind::Sentinel,
    BossKind::Prism,
    BossKind::Foundry,
    BossKind::Tempest,
    BossKind::Bastion,
    BossKind::Eclipse,
];
/// The most bosses one Boss Rush can line up.
pub const MOST_RUSH_BOSSES: usize = 32;
/// The highest wave number a snapshot may carry. A longer lineup keeps
/// counting on the host and tells players the real wave in its announcement.
pub const SHOWN_RUSH_WAVES: u8 = 6;
impl Default for WorldConfig {
    fn default() -> Self {
        Self {
            rules: tankyard_mods::GameRules::default(),
            modded: false,
            mode_name: None,
            width: 3600.,
            height: 2600.,
            dynamic_arena: true,
            arena_scale: 1.,
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
            id_base: 0,
            boss_rush: Vec::new(),
        }
    }
}
fn enabled() -> bool {
    true
}
fn unit_scale() -> f32 {
    1.
}
impl WorldConfig {
    /// The bosses this Boss Rush fights, in order.
    pub fn rush_lineup(&self) -> &[BossKind] {
        if self.boss_rush.is_empty() {
            &CLASSIC_RUSH
        } else {
            &self.boss_rush[..self.boss_rush.len().min(MOST_RUSH_BOSSES)]
        }
    }
    pub fn bounds(&self) -> Rect {
        let scale = if self.arena_scale.is_finite() {
            self.arena_scale.clamp(0.5, 1.)
        } else {
            1.
        };
        Rect {
            x: self.width * (1. - scale) * 0.5,
            y: self.height * (1. - scale) * 0.5,
            w: self.width * scale,
            h: self.height * scale,
        }
    }
    pub fn arena_point(&self, point: V2) -> V2 {
        let bounds = self.bounds();
        V2::new(
            bounds.x + point.x / self.width * bounds.w,
            bounds.y + point.y / self.height * bounds.h,
        )
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct PlayerInput {
    pub sequence: u32,
    pub movement: V2,
    pub aim: f32,
    pub aim_distance: f32,
    pub fire: bool,
    /// Manual cursor control for a captured drone, independent of auto-fire.
    #[serde(default)]
    pub steer_return: bool,
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
            steer_return: false,
            alt_fire: false,
            ability: false,
        }
    }
}
impl PlayerInput {
    /// Shared send/receive limits. Widening them requires a protocol change
    /// so older hosts can distinguish new input from an invalid frame.
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
    /// Trap, mine, barrier and turret counts, including units outside interest.
    #[serde(default)]
    pub deployed_defenses: [u8; 4],
    #[serde(default)]
    pub spawn_immunity: bool,
    /// Remaining forced recoil, shared with movement prediction.
    #[serde(default)]
    pub knockback: f32,
    /// Authoritative movement effects; client input cannot change this value.
    #[serde(default = "unit_scale")]
    pub speed_scale: f32,
    #[serde(default = "unit_scale")]
    pub reload_scale: f32,
    #[serde(default = "unit_scale")]
    pub damage_scale: f32,
    #[serde(default)]
    pub stasis: f32,
    /// Seconds left in the isolated pocket layer. Always controlled by the host.
    #[serde(default)]
    pub pocket: f32,
    /// Private, non-shooting doubles. Only the host advances their pursuit.
    #[serde(default)]
    pub pocket_echoes: Vec<PocketEcho>,
    /// Rift shots left after activating a rift ability: 2 opens the entry
    /// next, 1 the exit.
    #[serde(default)]
    pub rift_shots: u8,
    /// Seconds left to fire `rift_shots`.
    #[serde(default)]
    pub rift_armed: f32,
    /// Set only after the server verifies the creator's signing key.
    #[serde(default)]
    pub game_creator: bool,
    #[serde(default)]
    pub creator_boss: bool,
    #[serde(default)]
    pub creator_attacks: [f32; crate::creator_boss::ATTACK_COUNT],
    /// Seconds until Axiom's cannons can fire again, and whose turn it is.
    /// The server runs them alone, so neither travels in snapshots.
    #[serde(skip)]
    pub creator_reload: f32,
    #[serde(skip)]
    pub creator_turn: u8,
    #[serde(default)]
    pub server_badge: ServerBadge,
    /// Movement factor from the map pad under the tank; 1 off pads. Each
    /// side computes pads itself (clients only for their own prediction),
    /// so neither pad value travels in snapshots.
    #[serde(skip, default = "unit_scale")]
    pub terrain: f32,
    /// Velocity added by conveyor pads, in world units per second.
    #[serde(skip)]
    pub push: V2,
    /// How firmly the tank grips the floor, 1 off pads; ice lowers it.
    #[serde(skip, default = "unit_scale")]
    pub grip: f32,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum ServerBadge {
    #[default]
    Player,
    Moderator,
    Admin,
    Host,
    CentralOwner,
}
impl ServerBadge {
    pub fn label(self) -> &'static str {
        match self {
            Self::Player => "",
            Self::Moderator => "Moderator",
            Self::Admin => "Admin",
            Self::Host => "Server Host",
            Self::CentralOwner => "Central Server Owner",
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PocketEcho {
    pub pos: V2,
    pub previous: V2,
    pub vel: V2,
    pub aim: f32,
    pub radius: f32,
    pub class: String,
    /// A short still pose before the double starts pursuing.
    pub wake: f32,
}

impl Tank {
    /// Present for combat, pickups and objectives in the shared arena.
    pub fn tangible(&self) -> bool {
        self.alive && self.pocket <= 0.
    }
    pub fn badge(&self) -> &'static str {
        if self.game_creator {
            "Game Creator"
        } else {
            self.server_badge.label()
        }
    }
    pub fn display_name(&self) -> String {
        display_player_name(&self.name, self.game_creator, self.server_badge)
    }
}
pub fn display_player_name(name: &str, game_creator: bool, server_badge: ServerBadge) -> String {
    let badge = if game_creator {
        "Game Creator"
    } else {
        server_badge.label()
    };
    if !badge.is_empty() {
        format!("{name} [{badge}]")
    } else {
        name.to_owned()
    }
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
            deployed_defenses: self.deployed_defenses,
            spawn_immunity: self.spawn_immunity,
            knockback: self.knockback,
            speed_scale: self.speed_scale,
            reload_scale: self.reload_scale,
            damage_scale: self.damage_scale,
            stasis: self.stasis,
            pocket: self.pocket,
            pocket_echoes: self.pocket_echoes.clone(),
            rift_shots: self.rift_shots,
            rift_armed: self.rift_armed,
            game_creator: self.game_creator,
            creator_boss: self.creator_boss,
            creator_attacks: self.creator_attacks,
            creator_reload: self.creator_reload,
            creator_turn: self.creator_turn,
            server_badge: self.server_badge,
            terrain: self.terrain,
            push: self.push,
            grip: self.grip,
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
        self.deployed_defenses.clone_from(&source.deployed_defenses);
        self.spawn_immunity.clone_from(&source.spawn_immunity);
        self.knockback = source.knockback;
        self.speed_scale = source.speed_scale;
        self.reload_scale = source.reload_scale;
        self.damage_scale = source.damage_scale;
        self.stasis = source.stasis;
        self.pocket = source.pocket;
        self.pocket_echoes.clone_from(&source.pocket_echoes);
        self.rift_shots = source.rift_shots;
        self.rift_armed = source.rift_armed;
        self.game_creator = source.game_creator;
        self.creator_boss = source.creator_boss;
        self.creator_attacks = source.creator_attacks;
        self.creator_reload = source.creator_reload;
        self.creator_turn = source.creator_turn;
        self.server_badge = source.server_badge;
        self.terrain = source.terrain;
        self.push = source.push;
        self.grip = source.grip;
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
    #[serde(default)]
    pub frozen: Option<FrozenShot>,
    #[serde(default)]
    pub stasis_used: bool,
}
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct FrozenShot {
    pub anchor: EntityId,
    pub offset: V2,
    pub velocity: V2,
}
/// Which end of a linked pair a rift is: tanks and shots go in at the
/// entry and come out at the exit.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum RiftEnd {
    Entry,
    Exit,
}
/// One open end of a rift pair.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Rift {
    pub id: EntityId,
    pub owner: EntityId,
    pub team: u8,
    pub end: RiftEnd,
    pub pos: V2,
    /// The direction its shot was flying when it opened.
    pub angle: f32,
    /// Seconds since it opened.
    pub age: f32,
    /// Seconds left open. Below zero it is collapsing, until
    /// `-RIFT_CLOSING`, and carries nothing.
    pub life: f32,
    /// The other end of its pair once both are open; 0 before.
    pub partner: EntityId,
}
/// The bolt a rift gun fires; it opens a rift where it stops.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RiftShot {
    pub id: EntityId,
    pub owner: EntityId,
    pub team: u8,
    pub end: RiftEnd,
    pub pos: V2,
    pub vel: V2,
    /// Distance left before it opens where it is.
    pub range: f32,
}
impl Projectile {
    /// Captured swarms use a drone body and remember the attacking owner.
    pub fn is_return_drone(&self) -> bool {
        self.kind == ProjectileKind::Drone && self.homing_target != 0
    }
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
            frozen: self.frozen,
            stasis_used: self.stasis_used,
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
        self.frozen = source.frozen;
        self.stasis_used = source.stasis_used;
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
    Guardian,
    Summoner,
    Defender,
    FallenOverlord,
    FallenBooster,
    SiegeEngine,
    Skirmisher,
    Trapper,
    /// New in 2.0.5. It stays last so every kind before it keeps its number,
    /// and it never goes out on the wire under its own: see [`Self::from_wire`].
    Railgunner,
    /// The secret encounter, sent under its own kind in protocol 35.
    /// See [`Self::SECRET`] for the separate encounter pool.
    Sunderworm,
    Jester,
    Carillon,
}
impl BossKind {
    pub const ALL: [Self; 17] = [
        Self::Sentinel,
        Self::Prism,
        Self::Foundry,
        Self::Tempest,
        Self::Bastion,
        Self::Eclipse,
        Self::Guardian,
        Self::Summoner,
        Self::Defender,
        Self::FallenOverlord,
        Self::FallenBooster,
        Self::SiegeEngine,
        Self::Skirmisher,
        Self::Trapper,
        Self::Railgunner,
        Self::Jester,
        Self::Carillon,
    ];
    /// The bosses random events bring. The Railgunner comes on its own,
    /// very rarely.
    pub const COMMON: [Self; 14] = [
        Self::Sentinel,
        Self::Prism,
        Self::Foundry,
        Self::Tempest,
        Self::Bastion,
        Self::Eclipse,
        Self::Guardian,
        Self::Summoner,
        Self::Defender,
        Self::FallenOverlord,
        Self::FallenBooster,
        Self::SiegeEngine,
        Self::Skirmisher,
        Self::Trapper,
    ];
    /// Encounters outside the ordinary roster. Rare menu rolls, saved
    /// lineups, maps and mods can still select them by id.
    pub const SECRET: [Self; 1] = [Self::Sunderworm];
    /// Stable numeric IDs used by reliable encounter events. Append new kinds.
    pub const EVERY: [Self; 18] = [
        Self::Sentinel,
        Self::Prism,
        Self::Foundry,
        Self::Tempest,
        Self::Bastion,
        Self::Eclipse,
        Self::Guardian,
        Self::Summoner,
        Self::Defender,
        Self::FallenOverlord,
        Self::FallenBooster,
        Self::SiegeEngine,
        Self::Skirmisher,
        Self::Trapper,
        Self::Railgunner,
        Self::Sunderworm,
        Self::Jester,
        Self::Carillon,
    ];
    /// The kind numbered `index`, as boss events carry it.
    pub fn from_index(index: u32) -> Option<Self> {
        Self::EVERY.get(index as usize).copied()
    }
    pub fn name(self) -> &'static str {
        match self {
            Self::Sentinel => "Sentinel",
            Self::Prism => "Prism",
            Self::Foundry => "Foundry",
            Self::Tempest => "Tempest",
            Self::Bastion => "Bastion",
            Self::Eclipse => "Eclipse",
            Self::Guardian => "Guardian of the Pentagons",
            Self::Summoner => "Summoner",
            Self::Defender => "Defender",
            Self::FallenOverlord => "Fallen Overlord",
            Self::FallenBooster => "Fallen Booster",
            Self::SiegeEngine => "Siege Engine",
            Self::Skirmisher => "Skirmisher",
            Self::Trapper => "Trapper",
            Self::Railgunner => "Railgunner",
            Self::Sunderworm => "Sunderworm",
            Self::Jester => "Jester",
            Self::Carillon => "Carillon",
        }
    }
    /// The lowercase id maps and commands use, such as `fallen_overlord`.
    pub fn id(self) -> &'static str {
        match self {
            Self::Sentinel => "sentinel",
            Self::Prism => "prism",
            Self::Foundry => "foundry",
            Self::Tempest => "tempest",
            Self::Bastion => "bastion",
            Self::Eclipse => "eclipse",
            Self::Guardian => "guardian",
            Self::Summoner => "summoner",
            Self::Defender => "defender",
            Self::FallenOverlord => "fallen_overlord",
            Self::FallenBooster => "fallen_booster",
            Self::SiegeEngine => "siege_engine",
            Self::Skirmisher => "skirmisher",
            Self::Trapper => "trapper",
            Self::Railgunner => "railgunner",
            Self::Sunderworm => "sunderworm",
            Self::Jester => "jester",
            Self::Carillon => "carillon",
        }
    }
    pub fn from_id(id: &str) -> Option<Self> {
        Self::EVERY.into_iter().find(|kind| kind.id() == id)
    }
    pub fn radius(self) -> f32 {
        match self {
            Self::Guardian => 102.,
            Self::Summoner => 96.,
            Self::Defender => 106.,
            Self::FallenOverlord => 88.,
            Self::FallenBooster => 82.,
            Self::Foundry | Self::Bastion => 100.,
            Self::Tempest => 78.,
            Self::SiegeEngine => 104.,
            Self::Skirmisher => 72.,
            Self::Trapper => 88.,
            Self::Railgunner => RAIL_RADIUS,
            Self::Jester => crate::encounters::JESTER_RADIUS,
            Self::Carillon => crate::encounters::CARILLON_RADIUS,
            Self::Sunderworm => crate::sunderworm::WORM_RADIUS,
            _ => 90.,
        }
    }
    pub fn drone_limit(self) -> usize {
        match self {
            Self::Guardian => 24,
            Self::Summoner => 28,
            Self::FallenOverlord => 16,
            Self::Foundry | Self::Jester => 12,
            Self::Carillon => 16,
            Self::Sunderworm => crate::sunderworm::WORM_BROOD,
            _ => 0,
        }
    }
    /// Its health against other bosses. Those whose swarms soak up shots
    /// need less of it to last as long.
    pub fn toughness(self) -> f32 {
        match self {
            Self::Sunderworm => 7.5,
            Self::Railgunner => 3.,
            Self::Carillon => 5.6,
            Self::Jester => 2.7,
            Self::Defender => 1.29,
            Self::Bastion | Self::SiegeEngine => 1.2,
            Self::Sentinel | Self::Eclipse => 1.1,
            Self::Prism => 1.16,
            Self::Trapper => 1.55,
            Self::FallenBooster => 1.,
            Self::Tempest | Self::Skirmisher => 0.9,
            Self::Foundry | Self::Guardian => 0.95,
            Self::Summoner => 0.9,
            Self::FallenOverlord => 0.85,
        }
    }
    /// How fast its turning launchers spin, in radians a second.
    pub fn spin(self) -> f32 {
        match self {
            Self::Prism => 0.6,
            Self::Defender => 0.35,
            Self::Bastion => 0.3,
            _ => 0.,
        }
    }
    /// The angle its turning launchers are at on `tick`. Its rounds leave
    /// from them, so the game draws them there too.
    pub fn turn(self, tick: u64) -> f32 {
        (tick as f64 * f64::from(DT) * f64::from(self.spin())).rem_euclid(std::f64::consts::TAU)
            as f32
    }
    /// Seconds of warning before its special attack, with its aim locked.
    pub fn windup(self) -> f32 {
        match self {
            // Its cannon destroys whatever it hits, so it gives longer. The
            // Railgunner's must not pass this: older clients read it as this
            // boss and check the warning against it.
            Self::SiegeEngine | Self::Railgunner => 1.5,
            Self::Carillon => 1.35,
            // Its lighter cannon still gets a proper look down the barrel.
            Self::Bastion => 1.15,
            _ => 0.9,
        }
    }
    /// How many usual boss rewards defeating it is worth.
    pub fn bounty(self) -> u32 {
        match self {
            Self::Railgunner | Self::Jester => 3,
            Self::Carillon => 5,
            // Paid out twice, once for each half.
            Self::Sunderworm => 4,
            _ => 1,
        }
    }
}
/// Damage that destroys any tank outright, as the Siege Engine's cannon
/// deals. Other bosses and the staff's own boss take [`LETHAL_CAP`].
pub const LETHAL_DAMAGE: f32 = 1.0e6;
/// What a defeated boss pays out. Its `Destroy` event carries this as its
/// value, which is how a boss's death differs from one of its explosions: the
/// same event with the boss as subject, carrying the blast radius instead.
pub const BOSS_REWARD: u32 = 12_000;
/// The shares of its health at which a boss enters its second and third phase.
pub const BOSS_PHASES: [f32; 2] = [0.65, 0.3];
/// The longest a ready special waits for the gun to face its target, in
/// seconds. Its attack timer counts below zero during this hold.
pub const BOSS_AIM_HOLD: f32 = 1.2;
/// What a lethal round deals to a boss.
pub const LETHAL_CAP: f32 = 1500.;
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum BossTactic {
    #[default]
    Patrol,
    Advance,
    Orbit,
    Retreat,
    Windup,
    Charge,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Boss {
    pub id: EntityId,
    pub pos: V2,
    pub vel: V2,
    pub radius: f32,
    pub health: f32,
    pub max_health: f32,
    #[serde(serialize_with = "boss_kind_on_the_wire")]
    pub kind: BossKind,
    pub phase: u8,
    pub timer: f32,
    pub aim: f32,
    pub team: u8,
    pub mothership: bool,
    pub target: EntityId,
    pub goal: V2,
    pub tactic: BossTactic,
    pub tactic_time: f32,
    pub last_attacker: EntityId,
    pub arrival: f32,
    /// Time until a special is ready; down to `-BOSS_AIM_HOLD` while aiming.
    #[serde(default)]
    pub attack_timer: f32,
    #[serde(default)]
    pub attack: u8,
    #[serde(default)]
    pub attack_windup: f32,
    /// Its sidestep from incoming rounds, eased. Only the host steers with
    /// it, so it never travels in a snapshot.
    #[serde(skip)]
    pub dodge: V2,
    /// How it reads its target's motion, which lags a real change of course.
    #[serde(skip)]
    pub read: V2,
    /// Where its current straight leg ends, and how long it holds that leg.
    #[serde(skip)]
    pub leg: V2,
    #[serde(skip)]
    pub leg_time: f32,
    /// What a Railgunner is in the middle of; nothing for any other boss.
    #[serde(skip)]
    pub rail: RailWork,
    /// A parried strike on its way back to it. Host only, like the above.
    #[serde(skip)]
    pub clash: crate::clash::ClashWork,
    /// Who it is after and why, and what it has used lately. Host only.
    #[serde(skip)]
    pub mind: crate::boss_brain::Mind,
    /// A Sunderworm's body and what it is doing; none for any other boss.
    /// Host only; snapshots carry its compact collision rings separately.
    #[serde(skip)]
    pub worm: Option<Box<crate::sunderworm::WormWork>>,
}

impl Boss {
    /// A viewer never needs the host's brain, path history or attack work.
    /// Avoid cloning those buffers for every player receiving a snapshot.
    pub(crate) fn for_viewer(&self) -> Self {
        Self {
            id: self.id,
            pos: self.pos,
            vel: self.vel,
            radius: self.radius,
            health: self.health,
            max_health: self.max_health,
            kind: self.kind,
            phase: self.phase,
            timer: self.timer,
            aim: self.aim,
            team: self.team,
            mothership: self.mothership,
            target: self.target,
            goal: self.goal,
            tactic: self.tactic,
            tactic_time: self.tactic_time,
            last_attacker: self.last_attacker,
            arrival: self.arrival,
            attack_timer: self.attack_timer,
            attack: self.attack,
            attack_windup: self.attack_windup,
            dodge: V2::ZERO,
            read: V2::ZERO,
            leg: V2::ZERO,
            leg_time: 0.,
            rail: Default::default(),
            clash: Default::default(),
            mind: Default::default(),
            worm: None,
        }
    }
}
/// Keep the Railgunner's published wire representation. The Sunderworm
/// arrived with protocol 35 and uses its own kind, with no size-based alias.
fn boss_kind_on_the_wire<S: serde::Serializer>(kind: &BossKind, out: S) -> Result<S::Ok, S::Error> {
    match kind {
        BossKind::Railgunner => BossKind::SiegeEngine,
        other => *other,
    }
    .serialize(out)
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
    /// The Boss Rush wave as snapshots carry it, at most [`SHOWN_RUSH_WAVES`].
    pub boss_wave: u8,
    /// The real Boss Rush wave, on the host.
    #[serde(skip)]
    pub rush_wave: u8,
    /// How many times running the next boss has found nowhere safe to land.
    #[serde(skip)]
    pub rush_stalls: u8,
    pub safe_radius: f32,
    pub announcement: String,
    #[serde(default)]
    pub arena: ArenaStatus,
    /// Map groups hidden right now, one bit per group (group 1 is bit 0).
    #[serde(default)]
    pub hidden_groups: u32,
    /// Every map counter. Snapshots leave these out and carry
    /// `shown_counters` instead.
    #[serde(skip)]
    pub counters: tankyard_content::MapCounters,
    /// The counters the map's labels show, as (counter, value) pairs.
    #[serde(default)]
    pub shown_counters: Vec<(u8, i32)>,
    /// Pushed bodies away from where the map places them, or sliding.
    #[serde(default)]
    pub bodies: Vec<tankyard_content::BodyPose>,
    /// How map logic is moving, turning and fading groups. Clients replay
    /// these against the round clock instead of receiving moved geometry.
    #[serde(default)]
    pub tweens: Vec<tankyard_content::Tween>,
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
    BodyParry,
    StasisRelease,
    BossAttack,
    PocketEject,
    /// A Shake screen trigger reached tank `subject`; see
    /// [`crate::shake_of`] for `value`.
    MapShake,
    /// A Play sound trigger at `pos`; see [`crate::sound_of`] for `value`.
    MapSound,
    /// A launch pad flung tank `subject`.
    Launch,
    /// A Zoom view trigger reached tank `subject`; see [`crate::zoom_of`]
    /// for `value`.
    MapZoom,
    /// Rift `subject` opened; `value` is 0 for an entry, 1 for an exit.
    RiftOpen,
    /// Rift `subject` finished collapsing; `value` as for `RiftOpen`.
    RiftClose,
    /// Something went through a rift pair owned by `subject`: `value` is
    /// 0 for a tank, 1 for a shot. One event marks each end.
    RiftTravel,
    /// A rift shot found no room to open; `subject` is its owner.
    RiftFizzle,
    /// Where a Zoom view on tank `subject` turns; it follows that view's
    /// `MapZoom`. See [`crate::focus_of`] for `value`.
    MapFocus,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Elimination {
    pub victim: EntityId,
    pub name: String,
    pub game_creator: bool,
    #[serde(default)]
    pub server_badge: ServerBadge,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GameEvent {
    pub tick: u64,
    pub kind: EventKind,
    pub pos: V2,
    pub subject: EntityId,
    pub value: u32,
    pub team: u8,
    #[serde(default)]
    pub elimination: Option<Elimination>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Snapshot {
    pub tick: u64,
    pub config: WorldConfig,
    pub tanks: Vec<Tank>,
    pub projectiles: Vec<Projectile>,
    pub targets: Vec<Target>,
    #[serde(serialize_with = "crate::encounters::serialize_bosses")]
    pub bosses: Vec<Boss>,
    /// Cover where the map places it; map groups move it by `match_state.tweens`.
    pub walls: Vec<Rect>,
    /// The map group of each wall, sent along with `walls`.
    #[serde(default)]
    pub wall_groups: Vec<u8>,
    pub objectives: Vec<Objective>,
    pub match_state: MatchState,
    pub events: Vec<GameEvent>,
    /// Revision of the arena's wall geometry. Online snapshots repeat walls
    /// only after they change; clients keep the last copy they received.
    pub wall_revision: u32,
    /// Whether `walls` holds this revision's geometry in this snapshot.
    pub walls_included: bool,
    /// Open and collapsing rifts, and rift shots in flight.
    #[serde(default)]
    pub rifts: Vec<Rift>,
    #[serde(default)]
    pub rift_shots: Vec<RiftShot>,
    /// Authoritative worm segments, bounded by the arena's boss count.
    pub worm_bodies: Vec<crate::sunderworm::WormBody>,
}

/// Strip a distant opponent down to what the leaderboard and minimap need.
/// Aim, cursor, motion and weapon timers stay with the server; the position
/// is snapped to a coarse grid that is invisible at minimap scale.
pub fn conceal_distant_tank(tank: &mut Tank) {
    const GRID: f32 = 96.;
    tank.pos = V2::new(
        (tank.pos.x / GRID).round() * GRID,
        (tank.pos.y / GRID).round() * GRID,
    );
    tank.vel = V2::ZERO;
    tank.aim = 0.;
    tank.input = PlayerInput {
        sequence: tank.input.sequence,
        ..Default::default()
    };
    tank.cooldowns.clear();
    tank.barrel_shots.clear();
    tank.barrel_aims.clear();
    tank.charge = 0.;
    tank.knockback = 0.;
}
