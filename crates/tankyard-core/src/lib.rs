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
mod rifts;
pub use bosses::{BossRound, SIEGE_ROUND_RADIUS, SIEGE_ROUND_SPEED, special_volley};
pub use pocket::{
    POCKET_ECHO_LIMIT, POCKET_ECHO_WAKE, POCKET_HEAL_RATE, POCKET_SECONDS, walls as pocket_walls,
};
pub use pocket_layout::{
    LightState, PocketEdge, PocketLayout, PocketLight, bounds as pocket_bounds,
    with_layout as with_pocket_layout,
};
pub use rifts::{
    MAX_RIFTS, RIFT_ARMED, RIFT_CLOSING, RIFT_FORMING, RIFT_RADIUS, RIFT_RANGE, RIFT_SECONDS,
    RIFT_SHOT_SPEED, RIFT_SPACING,
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
    BodySpace, BodyState, BodyStep, HudPick, LAUNCH_COAST, LogicEffect, LogicInput,
    MAP_EVENT_LIMIT, MAP_HUD, MapEvent, MapLogic, MapView, Occupant, ShownUi, ZoomView,
    fill_map_widget, focus_event, focus_of, gate_state, launch_tank, map_point, map_rect,
    menu_number, place_walls, shake_event, shake_of, shelter_factor, snapshot_body_states,
    snapshot_poses, sound_event, sound_of, terrain_factor, terrain_grip, terrain_launch,
    terrain_push, ui_slot_id, zones_at, zoom_event, zoom_of,
};
pub use math::*;
pub use model::*;
pub use progression::*;
pub use special_modes::{HILL_SECONDS, INFECTION_SECONDS, INFECTION_WARMUP};
pub use spectating::*;
pub use tankyard_content::{MAX_LEVEL, UPGRADE_LEVELS};
pub use weapons::reload_progress;
pub use world::*;
