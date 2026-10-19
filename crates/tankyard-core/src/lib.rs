//! Server authoritative fixed-step simulation, with no rendering dependencies.
mod arena;
mod arena_size;
mod bosses;
#[cfg(test)]
mod combat_regressions;
mod drones;
mod homing;
pub mod math;
mod minions;
pub mod modding;
pub mod model;
mod modes;
mod parry;
pub mod physics;
pub mod progression;
#[cfg(test)]
mod scenarios;
mod spectating;
mod stasis;
pub use stasis::{STASIS_RADIUS, STASIS_SECONDS};
mod weapons;
pub mod world;
pub use arena::PVE_WAVES;
pub use math::*;
pub use model::*;
pub use progression::*;
pub use spectating::*;
pub use tankyard_content::{MAX_LEVEL, UPGRADE_LEVELS};
pub use weapons::reload_progress;
pub use world::*;
