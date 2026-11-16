//! Groups that move by physics or along a path: bodies tanks can push, such
//! as crates and push doors, and keyframed paths a Play path trigger sends a
//! group along.
use crate::{Easing, MAP_GROUPS, MOVE_SPEED_LIMIT};
use serde::{Deserialize, Serialize};

/// Which ways a body slides.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BodyAxis {
    /// Any direction, like a crate.
    #[default]
    Free,
    /// Left and right only, like a sliding door.
    Across,
    /// Up and down only.
    Along,
}
impl BodyAxis {
    pub const ALL: [Self; 3] = [Self::Free, Self::Across, Self::Along];
    pub fn name(self) -> &'static str {
        match self {
            Self::Free => "Any way",
            Self::Across => "Left and right",
            Self::Along => "Up and down",
        }
    }
    /// Keeps the parts of `v` this axis allows.
    pub fn project(self, v: [f32; 2]) -> [f32; 2] {
        match self {
            Self::Free => v,
            Self::Across => [v[0], 0.],
            Self::Along => [0., v[1]],
        }
    }
}

/// Makes a group's blocks one solid body that tanks push by driving into
/// it. Its decorations, pads and trigger areas ride along.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MapBody {
    /// The group, 1 to 32, whose blocks make up the body.
    pub group: u8,
    #[serde(default)]
    pub axis: BodyAxis,
    /// How far it can slide from where the map places it, as fractions of
    /// the arena: left, up, right and down, each 0 to 1.
    #[serde(default = "default_reach")]
    pub reach: [f32; 4],
    /// How hard it is to push, 0.25 to 8. One tank pushes a body of weight
    /// 1 at its own speed; two tanks push one of weight 2 as fast.
    #[serde(default = "unit")]
    pub weight: f32,
    /// How fast it slides back to its place when no one pushes, in arena
    /// widths a second, 0 to 1. 0 leaves it where it was pushed.
    #[serde(default)]
    pub spring: f32,
    /// How quickly it stops sliding once let go, 0.5 to 20.
    #[serde(default = "default_friction")]
    pub friction: f32,
    /// How much of its speed it keeps when it hits the end of its reach or
    /// other cover, 0 to 0.9.
    #[serde(default)]
    pub bounce: f32,
    /// Shots shove it too, harder the more damage they carry.
    #[serde(default)]
    pub shots: bool,
}
fn default_reach() -> [f32; 4] {
    [0.1; 4]
}
fn unit() -> f32 {
    1.
}
fn default_friction() -> f32 {
    6.
}
impl Default for MapBody {
    fn default() -> Self {
        Self {
            group: 1,
            axis: BodyAxis::Free,
            reach: default_reach(),
            weight: 1.,
            spring: 0.,
            friction: default_friction(),
            bounce: 0.,
            shots: false,
        }
    }
}
impl MapBody {
    pub fn validate(&self) -> Result<(), String> {
        if !(1..=MAP_GROUPS).contains(&self.group) {
            return Err(format!("a body is a group from 1 to {MAP_GROUPS}"));
        }
        if !self.reach.iter().all(|&r| range(r, 0., 1.)) {
            return Err("a body slides 0 to 1 arena widths each way".into());
        }
        if !range(self.weight, 0.25, 8.) {
            return Err("a body weighs 0.25 to 8".into());
        }
        if !range(self.spring, 0., 1.) {
            return Err("a body springs back at 0 to 1 arena widths a second".into());
        }
        if !range(self.friction, 0.5, 20.) || !range(self.bounce, 0., 0.9) {
            return Err("a body's friction is 0.5 to 20 and its bounce 0 to 0.9".into());
        }
        Ok(())
    }
}

/// Where a body is and how it moves, as snapshots carry it: offset from
/// where the map places it and velocity, both in arena fractions.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct BodyPose {
    pub group: u8,
    pub offset: [f32; 2],
    pub velocity: [f32; 2],
}
impl BodyPose {
    /// Whether every field holds a value a map could produce.
    pub fn valid(&self) -> bool {
        (1..=MAP_GROUPS).contains(&self.group)
            && self
                .offset
                .iter()
                .chain(&self.velocity)
                .all(|v| v.is_finite() && v.abs() <= 4.)
    }
}

/// How a path carries on after its last point.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PathRepeat {
    /// Stops at the last point.
    #[default]
    Once,
    /// Goes on from the last point to the first, round and round.
    Loop,
    /// Turns back at each end, there and back again.
    Bounce,
}
impl PathRepeat {
    pub const ALL: [Self; 3] = [Self::Once, Self::Loop, Self::Bounce];
    pub fn name(self) -> &'static str {
        match self {
            Self::Once => "Once",
            Self::Loop => "Loop",
            Self::Bounce => "There and back",
        }
    }
}

/// One stop on a path.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PathPoint {
    /// Where the group goes, as an offset from where the map places it in
    /// fractions of the arena's width and height.
    pub at: [f32; 2],
    /// Seconds to get here from the point before.
    #[serde(default = "unit")]
    pub seconds: f32,
    #[serde(default)]
    pub easing: Easing,
    /// How far the group's decorations are turned here, in degrees.
    #[serde(default)]
    pub turn: f32,
    /// Seconds to wait here before going on.
    #[serde(default)]
    pub pause: f32,
}
impl Default for PathPoint {
    fn default() -> Self {
        Self {
            at: [0., 0.],
            seconds: 1.,
            easing: Easing::default(),
            turn: 0.,
            pause: 0.,
        }
    }
}

/// A keyframed route for a group: a custom animation that moves cover,
/// pads and decorations through each point in turn.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MapPath {
    #[serde(default)]
    pub name: String,
    pub points: Vec<PathPoint>,
    #[serde(default)]
    pub repeat: PathRepeat,
}

/// Most paths a map holds; Play path triggers name them from 1.
pub const MAP_PATHS: usize = 32;
/// Most points on one path.
pub const PATH_POINTS: usize = 64;

impl MapPath {
    pub fn validate(&self) -> Result<(), String> {
        if self.points.is_empty() || self.points.len() > PATH_POINTS {
            return Err(format!("a path has 1 to {PATH_POINTS} points"));
        }
        if self.name.chars().count() > 32 || self.name.chars().any(char::is_control) {
            return Err("a path's name is at most 32 characters".into());
        }
        let mut previous = [0.; 2];
        for (index, point) in self.points.iter().enumerate() {
            let n = index + 1;
            if !point
                .at
                .iter()
                .all(|&v| range(v, -crate::TRAVEL_LIMIT, crate::TRAVEL_LIMIT))
            {
                return Err(format!(
                    "point {n} is more than {} arena widths away",
                    crate::TRAVEL_LIMIT
                ));
            }
            if !range(point.seconds, 0., crate::TWEEN_SECONDS)
                || !range(point.pause, 0., crate::TWEEN_SECONDS)
                || !range(point.turn, -36_000., 36_000.)
            {
                return Err(format!("point {n}'s times or turn are out of range"));
            }
            // A path moves cover like a Move trigger, so the same speed
            // limit keeps tanks from being flung through walls.
            let far = (point.at[0] - previous[0])
                .abs()
                .max((point.at[1] - previous[1]).abs());
            if far > 0. && far > point.seconds * MOVE_SPEED_LIMIT + 1e-4 {
                return Err(format!(
                    "point {n} is reached too fast: at most {MOVE_SPEED_LIMIT} arena widths a second"
                ));
            }
            previous = point.at;
        }
        Ok(())
    }
}

fn range(value: f32, min: f32, max: f32) -> bool {
    value.is_finite() && value >= min && value <= max
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bodies_and_paths_check_their_ranges() {
        assert!(MapBody::default().validate().is_ok());
        for bad in [
            MapBody {
                group: 0,
                ..MapBody::default()
            },
            MapBody {
                weight: 0.,
                ..MapBody::default()
            },
            MapBody {
                reach: [0., f32::NAN, 0., 0.],
                ..MapBody::default()
            },
        ] {
            assert!(bad.validate().is_err(), "{bad:?}");
        }
        let path = |points: Vec<PathPoint>| MapPath {
            points,
            ..MapPath::default()
        };
        assert!(path(vec![]).validate().is_err());
        let near = PathPoint {
            at: [0.2, 0.],
            ..PathPoint::default()
        };
        assert!(path(vec![near]).validate().is_ok());
        // A fifth of the arena in a tenth of a second is too fast.
        let rushed = PathPoint {
            seconds: 0.1,
            ..near
        };
        assert!(path(vec![rushed]).validate().is_err());
        assert_eq!(BodyAxis::Across.project([3., 4.]), [3., 0.]);
    }
}
