//! Server authoritative fixed-step simulation, with no rendering dependencies.
pub mod administration;
mod arena;
mod arena_size;
mod bosses;
#[cfg(test)]
mod combat_regressions;
pub mod creator_boss;
mod drones;
mod homing;
mod map_features;
mod map_layout;
pub mod math;
mod minions;
pub mod modding;
pub mod model;
mod modes;
mod parry;
mod pocket;
mod pocket_layout;
pub use pocket::{
    POCKET_ECHO_LIMIT, POCKET_ECHO_WAKE, POCKET_HEAL_RATE, POCKET_SECONDS, walls as pocket_walls,
};
pub use pocket_layout::{
    LightState, PocketEdge, PocketLayout, PocketLight, bounds as pocket_bounds,
    with_layout as with_pocket_layout,
};
pub mod physics;
pub mod progression;
#[cfg(test)]
mod scenarios;
mod special_modes;
mod spectating;
mod stasis;
mod team_bases;
pub use stasis::{STASIS_RADIUS, STASIS_SECONDS};
pub use team_bases::{BASE_ALERT_DISTANCE, BASE_GUARD_OWNER, base_guard_owner, base_guard_team};
mod weapons;
pub mod world;
pub use arena::PVE_WAVES;
pub use map_features::{
    LAUNCH_COAST, LogicEffect, LogicInput, MAP_EVENT_LIMIT, MapEvent, MapLogic, Occupant,
    gate_state, launch_tank, map_point, map_rect, place_walls, shake_event, shake_of,
    shelter_factor, snapshot_poses, sound_event, sound_of, terrain_factor, terrain_launch,
    terrain_push, zones_at,
};
pub use math::*;
pub use model::*;
pub use progression::*;
pub use special_modes::{HILL_SECONDS, INFECTION_SECONDS, INFECTION_WARMUP};
pub use spectating::*;
pub use tankyard_content::{MAX_LEVEL, UPGRADE_LEVELS};
pub use weapons::reload_progress;
pub use world::*;
