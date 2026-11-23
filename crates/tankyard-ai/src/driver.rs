//! A small neural network that drives. Each tick it looks along sixteen
//! directions around the bot and scores each one from what a driver can see
//! that way: how far the walls are, whether incoming shots would cross that
//! path, where the target and other tanks are, and which way the bot already
//! planned to go. The bot steers toward the best-scoring direction.
//!
//! Its weights are trained by self-play: `tankyard-tools train-driver` plays
//! bots with slightly changed weights against bots without the network and
//! keeps the changes that win. The trained weights ship in
//! `driver_weights.rs`.
use super::*;
use crate::neural::Network;

/// Directions the driver considers, evenly spaced around the bot.
pub const DIRECTIONS: usize = 16;
/// What the driver sees along each direction.
pub const FEATURES: usize = 12;
pub type DriverNet = Network<FEATURES, 1>;
/// How far ahead the driver looks for walls.
const SIGHT: f32 = 360.;
/// How far it looks for shots and tanks.
const AWARENESS: f32 = 420.;

/// The weights bots drive with unless told otherwise.
pub fn trained() -> DriverNet {
    let mut net = DriverNet::default();
    if !net.set_weights(&crate::driver_weights::WEIGHTS) {
        net.set_weights(&initial_weights());
    }
    net
}

/// Weights that copy the hand-written steering, with a little wall and shot
/// sense on top: where training starts.
pub fn initial_weights() -> Vec<f32> {
    let mut hidden = [[0.; FEATURES]; 8];
    hidden[0][0] = 1.6;
    hidden[1][1] = 2.;
    hidden[1][11] = -1.;
    hidden[2][2] = 1.5;
    hidden[2][11] = -0.75;
    hidden[3][6] = -2.;
    let output = [1.5, 0.5, 0.3, 0.8, 0., 0., 0., 0., 0.];
    hidden
        .iter()
        .flatten()
        .chain(output.iter())
        .copied()
        .collect()
}

pub(super) struct Situation<'a> {
    pub world: &'a World,
    /// The world's cover, indexed for quick sight lines.
    pub cover: &'a Navigation,
    pub tank: &'a Tank,
    pub class: &'a ClassDef,
    /// Where the hand-written steering wants to go, up to length 1.
    pub plan: V2,
    pub target: Option<TargetSample>,
    pub seen: &'a [Seen],
    /// How readily this bot reacts to shots, from its difficulty.
    pub alertness: f32,
}

fn direction(index: usize) -> V2 {
    V2::from_angle(index as f32 / DIRECTIONS as f32 * std::f32::consts::TAU)
}

/// How far a tank can drive along `dir` before touching a wall or the arena
/// edge, as a fraction of [`SIGHT`].
fn clearance(world: &World, cover: &Navigation, pos: V2, radius: f32, dir: V2) -> f32 {
    let nearest = cover.first_contact(pos, pos + dir * SIGHT, radius);
    let bounds = world.config.bounds();
    let edge = |from: f32, low: f32, high: f32, step: f32| {
        if step > 1e-4 {
            (high - radius - from) / (step * SIGHT)
        } else if step < -1e-4 {
            (low + radius - from) / (step * SIGHT)
        } else {
            1.
        }
    };
    nearest
        .min(edge(pos.x, bounds.x, bounds.x + bounds.w, dir.x))
        .min(edge(pos.y, bounds.y, bounds.y + bounds.h, dir.y))
        .clamp(0., 1.)
}

thread_local! {
    /// The shots a decision weighs, kept between decisions for the space.
    static SHOTS: std::cell::RefCell<Vec<(V2, V2, f32, f32)>> =
        const { std::cell::RefCell::new(Vec::new()) };
}

/// What the driver sees along each direction.
pub(super) fn sense(s: &Situation) -> [[f32; FEATURES]; DIRECTIONS] {
    SHOTS.with_borrow_mut(|shots| {
        shots.clear();
        sense_with(s, shots)
    })
}

fn sense_with(s: &Situation, shots: &mut Vec<(V2, V2, f32, f32)>) -> [[f32; FEATURES]; DIRECTIONS] {
    let (world, tank) = (s.world, s.tank);
    let speed = physics::tank_speed(tank, &world.content).max(60.);
    let radius = tank.radius * 0.8;
    let clear: [f32; DIRECTIONS] =
        std::array::from_fn(|i| clearance(world, s.cover, tank.pos, radius, direction(i)));
    let mut threat = V2::ZERO;
    let mut allies = V2::ZERO;
    for &item in s.seen {
        match item {
            Seen::Tank(index) => {
                let other = &world.tanks[index];
                if other.id == tank.id || !other.tangible() {
                    continue;
                }
                let delta = other.pos - tank.pos;
                let distance = delta.length();
                if distance > AWARENESS || distance < 1. {
                    continue;
                }
                let near = 1. - distance / AWARENESS;
                if hostile(tank.team, other.team) {
                    let strength = (f32::from(other.level) + 12.) / (f32::from(tank.level) + 12.);
                    threat += delta / distance * near * strength.clamp(0.4, 2.);
                } else if tank.team != 0 {
                    allies += delta / distance * near;
                }
            }
            Seen::Projectile(index) => {
                let shot = &world.projectiles[index];
                if shot.owner == tank.id
                    || !hostile(tank.team, shot.team)
                    || shot.damage <= 0.
                    || shot.frozen.is_some()
                    || shot.pos.distance(tank.pos) > AWARENESS
                {
                    continue;
                }
                let severity = (shot.damage / (tank.health.max(1.) * 0.25))
                    .sqrt()
                    .clamp(0.6, 1.8);
                shots.push((shot.pos, shot.vel, shot.radius, severity));
            }
            Seen::Target(_) | Seen::Boss(_) => {}
        }
    }
    let threat_strength = threat.length().min(1.);
    let threat = threat.normalized();
    let ally_strength = allies.length().min(1.);
    let allies = allies.normalized();
    let health = tank.health / tank.max_health.max(1.);
    let (to_target, range_error) = s.target.map_or((V2::ZERO, 0.), |target| {
        let delta = target.pos - tank.pos;
        let preferred = s.class.ai_range.max(120.);
        (
            delta.normalized(),
            ((delta.length() - preferred) / preferred).clamp(-1., 1.),
        )
    });
    let velocity = tank.vel / speed;
    std::array::from_fn(|i| {
        let dir = direction(i);
        let mut risk = 0.;
        for &(pos, vel, shot_radius, severity) in shots.iter() {
            let relative = pos - tank.pos;
            let closing = vel - dir * speed;
            let speed_sq = closing.length_squared().max(1.);
            let time = (-relative.dot(closing) / speed_sq).clamp(0., 0.6);
            let miss = (relative + closing * time).length();
            let safety = tank.radius + shot_radius + 14.;
            if miss < safety {
                risk += (1. - miss / safety) * (1. - time / 0.9) * severity;
            }
        }
        let toward = to_target.dot(dir);
        [
            s.plan.dot(dir),
            clear[i],
            clear[(i + 1) % DIRECTIONS].min(clear[(i + DIRECTIONS - 1) % DIRECTIONS]),
            toward,
            toward * range_error,
            (to_target.x * dir.y - to_target.y * dir.x).abs(),
            (risk * s.alertness).min(1.),
            threat.dot(dir) * threat_strength,
            velocity.dot(dir).clamp(-1., 1.),
            health * toward,
            allies.dot(dir) * ally_strength,
            1.,
        ]
    })
}

/// Where the driver steers, as a movement no longer than the plan.
pub(super) fn drive(net: &DriverNet, s: &Situation) -> V2 {
    let length = s.plan.length();
    if length < 0.05 {
        return s.plan;
    }
    let scores = sense(s).map(|features| net.predict(features)[0]);
    let best = (0..DIRECTIONS)
        .max_by(|&a, &b| scores[a].total_cmp(&scores[b]))
        .unwrap_or(0);
    // Fit a parabola through the best score and its neighbours, so the bot
    // can steer between the sixteen directions instead of snapping to them.
    let (left, mid, right) = (
        scores[(best + DIRECTIONS - 1) % DIRECTIONS],
        scores[best],
        scores[(best + 1) % DIRECTIONS],
    );
    let curve = left - 2. * mid + right;
    let shift = if curve < -1e-6 {
        (0.5 * (left - right) / curve).clamp(-0.5, 0.5)
    } else {
        0.
    };
    let angle = (best as f32 + shift) / DIRECTIONS as f32 * std::f32::consts::TAU;
    V2::from_angle(angle) * length.min(1.)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tankyard_content::Content;
    use tankyard_core::{Rect, WorldConfig};

    fn situation_world() -> World {
        let mut world = World::new(WorldConfig::default(), Content::builtin());
        world.add_player("Driver", true);
        world.walls.clear();
        world.tanks[0].pos = V2::new(1000., 1000.);
        world
    }

    #[test]
    fn starting_weights_follow_the_plan_in_the_open() {
        let world = situation_world();
        let tank = &world.tanks[0];
        let class = world.content.class(&tank.class).unwrap();
        let net = {
            let mut net = DriverNet::default();
            assert!(net.set_weights(&initial_weights()));
            net
        };
        for angle in [0.3_f32, 1.9, -2.4] {
            let plan = V2::from_angle(angle) * 0.8;
            let steer = drive(
                &net,
                &Situation {
                    world: &world,
                    cover: &Navigation::of(&world),
                    tank,
                    class,
                    plan,
                    target: None,
                    seen: &[],
                    alertness: 1.,
                },
            );
            assert!(steer.normalized().dot(plan.normalized()) > 0.98, "{angle}");
            assert!((steer.length() - 0.8).abs() < 1e-4);
        }
    }

    #[test]
    fn the_driver_sees_walls_and_the_arena_edge() {
        let mut world = situation_world();
        world.walls.push(Rect {
            x: 1100.,
            y: 900.,
            w: 40.,
            h: 200.,
        });
        let tank = &world.tanks[0];
        let cover = Navigation::of(&world);
        let east = clearance(&world, &cover, tank.pos, tank.radius, V2::new(1., 0.));
        let west = clearance(&world, &cover, tank.pos, tank.radius, V2::new(-1., 0.));
        assert!(east < 0.3, "east {east}");
        assert_eq!(west, 1.);
        let bounds = world.config.bounds();
        let edge = V2::new(bounds.x + tank.radius + 1., bounds.y + 400.);
        assert!(clearance(&world, &cover, edge, tank.radius, V2::new(-1., 0.)) < 0.01);
        assert!(clearance(&world, &cover, edge, tank.radius, V2::new(0., 1.)) > 0.9);
        // With walls straight ahead, the start weights turn aside.
        let class = world.content.class(&tank.class).unwrap();
        let mut net = DriverNet::default();
        net.set_weights(&initial_weights());
        let steer = drive(
            &net,
            &Situation {
                world: &world,
                cover: &cover,
                tank,
                class,
                plan: V2::new(1., 0.),
                target: None,
                seen: &[],
                alertness: 1.,
            },
        );
        assert!(steer.x > 0. && steer.y.abs() > 0.15, "{steer:?}");
    }

    #[test]
    fn shipped_weights_load() {
        let net = trained();
        assert!(net.weights().iter().all(|w| w.is_finite()));
        assert_eq!(crate::driver_weights::WEIGHTS.len(), DriverNet::WEIGHTS);
    }
}
