//! Gatewright's rift pairs. Activating the ability arms the rift gun for
//! two shots: the first opens an entry, the second its exit. While both
//! stand open, the owner, their team and flying rounds that reach the
//! entry's heart leave the exit at the same speed and heading. A pair stays
//! open for `RIFT_SECONDS`, then collapses.
use crate::physics::{circle_rect, clear_circle_position, segment_distance_squared, sweep_rect};
use crate::*;
use tankyard_content::{Ability, ClassDef, ProjectileKind};

/// How long a linked pair stays open.
pub const RIFT_SECONDS: f32 = 15.;
/// How long a rift takes to collapse. It carries nothing meanwhile.
pub const RIFT_CLOSING: f32 = 0.6;
/// A rift's rim radius in world units.
pub const RIFT_RADIUS: f32 = 54.;
/// Seconds after activating in which both rift shots must be fired.
pub const RIFT_ARMED: f32 = 8.;
/// A new rift carries nothing until its rim has formed.
pub const RIFT_FORMING: f32 = 0.3;
pub const RIFT_SHOT_SPEED: f32 = 1500.;
/// The farthest a rift shot flies before it opens.
pub const RIFT_RANGE: f32 = 1300.;
/// The two ends of a pair open at least this far apart.
pub const RIFT_SPACING: f32 = RIFT_RADIUS * 4.;
/// The pause between a gun's two rift shots.
const SHOT_INTERVAL: f32 = 0.35;
const SHOT_RADIUS: f32 = 8.;
/// How close to an entry's heart a tank's center must come: about where
/// most of its hull covers the rift.
const TANK_REACH: f32 = RIFT_RADIUS * 0.7;
/// How close to an entry's heart a round's path must pass.
const ROUND_REACH: f32 = RIFT_RADIUS * 0.6;
/// After going through, a tank waits this long before any portal takes it.
const TANK_REST: f32 = 0.5;
// Clients snap a tank that moves more than 150 units in one step rather
// than sliding it there, so every trip must be longer than that.
const _: () = assert!(RIFT_SPACING - TANK_REACH > 160.);
/// No more rifts than this stand in one arena.
pub const MAX_RIFTS: usize = 96;

/// Rounds in flight go through; drones, minions and deployables stay put.
fn carried(kind: ProjectileKind) -> bool {
    matches!(
        kind,
        ProjectileKind::Bullet
            | ProjectileKind::Shell
            | ProjectileKind::Piercing
            | ProjectileKind::Scatter
            | ProjectileKind::Homing
            | ProjectileKind::Explosive
            | ProjectileKind::Fragment
    )
}

/// Count down an armed rift gun. Shared with prediction.
pub(crate) fn tick_armed(tank: &mut Tank, dt: f32) {
    // The window outlasts the last shot, so one that fizzles comes back.
    tank.rift_armed = (tank.rift_armed - dt).max(0.);
    if tank.rift_armed <= 0. || !tank.alive {
        tank.rift_shots = 0;
        tank.rift_armed = 0.;
    }
}

/// Run an armed rift gun for one step: it fires nothing else, and a held
/// trigger sends the next rift shot once the barrel is ready. Shared with
/// prediction, which only animates the barrel; the host opens rifts.
pub(crate) fn pull_trigger(tank: &mut Tank, class: &ClassDef, dt: f32) -> bool {
    for cooldown in &mut tank.cooldowns {
        *cooldown = (*cooldown - dt).max(0.);
    }
    tank.charge = 0.;
    let Some(barrel) = class.barrels.first() else {
        return false;
    };
    if tank.rift_shots == 0
        || !tank.input.fire
        || tank
            .cooldowns
            .first()
            .is_some_and(|&cooldown| cooldown > 0.)
    {
        return false;
    }
    tank.rift_shots -= 1;
    let aim = tank.aim + barrel.angle;
    if let Some(cooldown) = tank.cooldowns.first_mut() {
        *cooldown = SHOT_INTERVAL;
    }
    if let Some(shots) = tank.barrel_shots.first_mut() {
        *shots = shots.wrapping_add(1);
    }
    if let Some(barrel_aim) = tank.barrel_aims.first_mut() {
        *barrel_aim = aim;
    }
    crate::weapons::kick(tank, class, barrel, aim, 1, 1.);
    tank.protection = 0.;
    tank.spawn_immunity = false;
    true
}

impl World {
    /// Send tank `index`'s next rift shot from its first barrel: an entry
    /// unless one is already open or on its way.
    pub(crate) fn fire_rift(&mut self, index: usize) {
        let tank = &self.tanks[index];
        let Some(barrel) = self
            .content
            .class(&tank.class)
            .and_then(|class| class.barrels.first())
        else {
            return;
        };
        let (owner, team) = (tank.id, tank.team);
        let aim = tank.aim + barrel.angle;
        let mount = tank.pos + V2::new(barrel.offset[0], barrel.offset[1]).rotate(tank.aim);
        let muzzle = mount + V2::from_angle(aim) * (barrel.length + tank.radius * 0.45);
        // As with bullets, a barrel against a wall can't fire through it.
        let mut contact: f32 = 1.;
        for &wall in &self.walls {
            if let Some((time, _)) = sweep_rect(mount, muzzle, SHOT_RADIUS, wall) {
                contact = contact.min(time);
            }
        }
        let origin = mount.lerp(muzzle, contact);
        // The rift opens under the cursor, within the gun's range.
        let range = (tank.input.aim_distance - tank.pos.distance(origin))
            .clamp(RIFT_RADIUS * 1.2, RIFT_RANGE);
        let entry_pending = self
            .rifts
            .iter()
            .any(|r| r.owner == owner && r.end == RiftEnd::Entry && r.life > 0.)
            || self
                .rift_shots
                .iter()
                .any(|s| s.owner == owner && s.end == RiftEnd::Entry);
        let end = if entry_pending {
            RiftEnd::Exit
        } else {
            RiftEnd::Entry
        };
        let shot = RiftShot {
            id: self.id(),
            owner,
            team,
            end,
            pos: origin,
            vel: V2::from_angle(aim) * RIFT_SHOT_SPEED,
            range,
        };
        self.rift_shots.push(shot);
        self.event(
            EventKind::Shot,
            origin,
            owner,
            ProjectileKind::Bullet as u32,
            team,
        );
    }

    /// Collapse every rift `owner` has open and drop their rift shots.
    pub(crate) fn collapse_rifts(&mut self, owner: EntityId) {
        self.rift_shots.retain(|shot| shot.owner != owner);
        for rift in self.rifts.iter_mut().filter(|rift| rift.owner == owner) {
            rift.life = rift.life.min(0.);
        }
    }

    /// Whether a whole rift fits at `at`: inside the arena, clear of cover
    /// and of other open rifts.
    fn rift_fits(&mut self, at: V2) -> bool {
        self.config.bounds().contains_circle(at, RIFT_RADIUS)
            && self
                .wall_index
                .near(at, RIFT_RADIUS)
                .iter()
                .all(|&wall| circle_rect(at, RIFT_RADIUS, wall).is_none())
            && self
                .rifts
                .iter()
                .all(|rift| rift.life <= 0. || rift.pos.distance(at) >= RIFT_RADIUS * 2.)
    }

    /// Open `shot`'s rift where it stopped, at `at` while flying along
    /// `dir`. Without room, the shot fizzles and the gun gets it back.
    fn open_rift(&mut self, shot: &RiftShot, at: V2, dir: V2) {
        // Back away from whatever stopped the shot until the rim fits.
        let spot = (0..=40)
            .map(|step| at - dir * (step as f32 * 6.))
            .find(|&p| self.rift_fits(p));
        let partner = self.rifts.iter().position(|rift| {
            rift.owner == shot.owner && rift.end != shot.end && rift.life > 0. && rift.partner == 0
        });
        let spot = spot.filter(|&p| {
            self.rifts.len() < MAX_RIFTS
                && partner.is_none_or(|index| self.rifts[index].pos.distance(p) >= RIFT_SPACING)
        });
        let Some(pos) = spot else {
            if let Some(tank) = self
                .tanks
                .iter_mut()
                .find(|tank| tank.id == shot.owner && tank.rift_armed > 0.)
            {
                tank.rift_shots = (tank.rift_shots + 1).min(2);
            }
            self.event(EventKind::RiftFizzle, at, shot.owner, 0, shot.team);
            return;
        };
        let id = self.id();
        let mut rift = Rift {
            id,
            owner: shot.owner,
            team: shot.team,
            end: shot.end,
            pos,
            angle: dir.angle(),
            age: 0.,
            life: RIFT_SECONDS,
            partner: 0,
        };
        if let Some(index) = partner {
            // The pair's time starts when both ends are open.
            rift.partner = self.rifts[index].id;
            self.rifts[index].partner = id;
            self.rifts[index].life = RIFT_SECONDS;
        }
        self.rifts.push(rift);
        // Events name the owner, so each player can tell their side's rifts
        // from an opponent's.
        self.event(
            EventKind::RiftOpen,
            pos,
            shot.owner,
            shot.end as u32,
            shot.team,
        );
    }

    /// Fly rift shots, age and collapse rifts, and carry what reaches an
    /// open entry to its exit.
    pub(crate) fn update_rifts(&mut self) {
        if self.rifts.is_empty() && self.rift_shots.is_empty() {
            return;
        }
        self.wall_index.sync(&self.walls);
        // A rift outlives neither its owner nor their rift gun.
        let mut gone: Vec<EntityId> = self
            .rifts
            .iter()
            .map(|rift| rift.owner)
            .chain(self.rift_shots.iter().map(|shot| shot.owner))
            .collect();
        gone.sort_unstable();
        gone.dedup();
        gone.retain(|&owner| {
            !self.tanks.iter().any(|tank| {
                tank.id == owner
                    && tank.tangible()
                    && tank.health > 0.
                    && self
                        .content
                        .class(&tank.class)
                        .is_some_and(|class| class.ability == Ability::Rift)
            })
        });
        for owner in gone {
            self.collapse_rifts(owner);
        }
        self.fly_rift_shots();
        self.age_rifts();
        self.carry_through_rifts();
    }

    fn fly_rift_shots(&mut self) {
        let shots = std::mem::take(&mut self.rift_shots);
        let mut flying = Vec::with_capacity(shots.len());
        for mut shot in shots {
            let speed = shot.vel.length();
            if !speed.is_finite() || speed < 1. || !shot.pos.finite() {
                continue;
            }
            let dir = shot.vel / speed;
            let travel = (speed * DT).min(shot.range);
            let start = shot.pos;
            let end = start + dir * travel;
            let mut stop: Option<f32> = None;
            let walls = &self.walls;
            for &wall in self.wall_index.query(start, end, SHOT_RADIUS) {
                if let Some((time, _)) = sweep_rect(start, end, SHOT_RADIUS, walls[wall]) {
                    stop = Some(stop.map_or(time, |s| s.min(time)));
                }
            }
            for (time, _) in boundary_hits(start, end, SHOT_RADIUS, &self.config)
                .into_iter()
                .flatten()
            {
                stop = Some(stop.map_or(time, |s| s.min(time)));
            }
            if let Some(time) = stop {
                self.open_rift(&shot, start + (end - start) * time, dir);
            } else if shot.range - travel <= 0.01 {
                self.open_rift(&shot, end, dir);
            } else {
                shot.pos = end;
                shot.range -= travel;
                flying.push(shot);
            }
        }
        self.rift_shots = flying;
    }

    fn age_rifts(&mut self) {
        let armed: Vec<EntityId> = self
            .tanks
            .iter()
            .filter(|tank| tank.rift_shots > 0)
            .map(|tank| tank.id)
            .collect();
        for index in 0..self.rifts.len() {
            let (pos, owner, partner, life) = {
                let rift = &mut self.rifts[index];
                rift.age += DT;
                (rift.pos, rift.owner, rift.partner, rift.life)
            };
            if life <= 0. {
                self.rifts[index].life -= DT;
                continue;
            }
            // Cover moving onto a rift's heart crushes it.
            let crushed = self
                .wall_index
                .near(pos, RIFT_RADIUS * 0.5)
                .iter()
                .any(|&wall| circle_rect(pos, RIFT_RADIUS * 0.5, wall).is_some());
            // A lone end whose gun can no longer open its partner.
            let stranded = partner == 0
                && !armed.contains(&owner)
                && !self.rift_shots.iter().any(|shot| shot.owner == owner);
            let rift = &mut self.rifts[index];
            if partner != 0 {
                rift.life -= DT;
            }
            if crushed || stranded {
                rift.life = rift.life.min(0.);
            }
        }
        // A pair collapses together.
        for index in 0..self.rifts.len() {
            let (life, partner) = (self.rifts[index].life, self.rifts[index].partner);
            if life <= 0.
                && partner != 0
                && let Some(other) = self.rifts.iter_mut().find(|rift| rift.id == partner)
            {
                other.life = other.life.min(0.);
            }
        }
        let mut popped = Vec::new();
        self.rifts.retain(|rift| {
            let open = rift.life > -RIFT_CLOSING;
            if !open {
                popped.push((rift.pos, rift.owner, rift.end as u32, rift.team));
            }
            open
        });
        for (pos, owner, end, team) in popped {
            self.event(EventKind::RiftClose, pos, owner, end, team);
        }
    }

    fn carry_through_rifts(&mut self) {
        let bounds = self.config.bounds();
        for index in 0..self.rifts.len() {
            let entry = &self.rifts[index];
            if entry.end != RiftEnd::Entry
                || entry.life <= 0.
                || entry.age < RIFT_FORMING
                || entry.partner == 0
            {
                continue;
            }
            let Some(exit) = self.rifts.iter().find(|rift| {
                rift.id == entry.partner && rift.life > 0. && rift.age >= RIFT_FORMING
            }) else {
                continue;
            };
            let (from, to, owner, team) = (entry.pos, exit.pos, entry.owner, entry.team);
            for t in 0..self.tanks.len() {
                let tank = &self.tanks[t];
                // The owner and their team go through. A flag carrier can't
                // shortcut a capture, and opponents can't follow.
                if !tank.tangible()
                    || tank.health <= 0.
                    || tank.creator_boss
                    || tank.carrying
                    || !(tank.id == owner || (team > 0 && tank.team == team))
                    || (tank.pos - from).length_squared() > TANK_REACH * TANK_REACH
                    || self.portal_rest.contains_key(&tank.id)
                {
                    continue;
                }
                let (id, radius) = (tank.id, tank.radius);
                let Some(clear) = clear_circle_position(to, radius, bounds, &self.walls) else {
                    continue;
                };
                self.tanks[t].pos = clear;
                self.portal_rest.insert(id, TANK_REST);
                self.event(EventKind::RiftTravel, from, owner, 0, team);
                self.event(EventKind::RiftTravel, clear, owner, 0, team);
            }
            let mut carried_any = false;
            for shot in &mut self.projectiles {
                if shot.lifetime <= 0.
                    || shot.penetration <= 0.
                    || shot.frozen.is_some()
                    || !carried(shot.kind)
                    || segment_distance_squared(from, shot.previous, shot.pos)
                        > ROUND_REACH * ROUND_REACH
                {
                    continue;
                }
                let speed = shot.vel.length();
                if !speed.is_finite() || speed < 1. {
                    continue;
                }
                // It leaves the exit as far past the heart as it had flown
                // past the entry's, heading the same way. Its last position
                // moves with it, a whole step back inside the exit, so it is
                // drawn coming out at its speed rather than standing still.
                let dir = shot.vel / speed;
                let step = shot.pos.distance(shot.previous);
                let beyond = (shot.pos - from).dot(dir).clamp(0., RIFT_RADIUS * 0.5);
                shot.pos = to + dir * beyond;
                shot.previous = shot.pos - dir * step.min(RIFT_RADIUS);
                carried_any = true;
            }
            if carried_any {
                self.event(EventKind::RiftTravel, to, owner, 1, team);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn world() -> (World, EntityId) {
        let mut w = World::new(
            WorldConfig {
                mode: Mode::Sandbox,
                target_count: 0,
                spawn_protection: 0.,
                dynamic_arena: false,
                ..Default::default()
            },
            tankyard_content::Content::builtin(),
        );
        let id = w.add_player("Gate", false);
        assert!(w.sandbox_class(id, "gatewright"));
        w.tanks[0].pos = V2::new(800., 1300.);
        w.tanks[0].aim = 0.;
        w.walls.clear();
        (w, id)
    }

    /// Activate, then fire at `distance` along `aim` until a shot leaves.
    fn fire(w: &mut World, aim: f32, distance: f32) {
        w.tanks[0].aim = aim;
        w.tanks[0].input.aim = aim;
        w.tanks[0].input.aim_distance = distance;
        w.tanks[0].input.fire = true;
        for _ in 0..40 {
            let before = w.rift_shots.len() + w.rifts.len();
            w.step();
            if w.rift_shots.len() + w.rifts.len() > before {
                break;
            }
        }
        w.tanks[0].input.fire = false;
    }

    fn open_pair(w: &mut World) {
        w.tanks[0].input.ability = true;
        w.step();
        w.tanks[0].input.ability = false;
        assert_eq!(w.tanks[0].rift_shots, 2);
        fire(w, 0., 700.);
        fire(w, std::f32::consts::FRAC_PI_2, 700.);
        for _ in 0..60 {
            w.step();
        }
        assert_eq!(w.rifts.len(), 2, "both ends open");
        assert!(w.rifts.iter().all(|r| r.partner != 0 && r.life > 0.));
    }

    #[test]
    fn two_shots_open_a_linked_pair_under_the_cursor() {
        let (mut w, id) = world();
        open_pair(&mut w);
        let entry = w.rifts.iter().find(|r| r.end == RiftEnd::Entry).unwrap();
        let exit = w.rifts.iter().find(|r| r.end == RiftEnd::Exit).unwrap();
        assert_eq!(entry.owner, id);
        assert!((entry.pos.y - 1300.).abs() < 40. && entry.pos.x > 1300.);
        assert!(exit.pos.y > 1700.);
        assert_eq!(entry.partner, exit.id);
        assert_eq!(w.tanks[0].rift_shots, 0);
    }

    #[test]
    fn the_owner_and_their_rounds_come_out_of_the_exit_moving_the_same_way() {
        let (mut w, _) = world();
        open_pair(&mut w);
        let entry = w
            .rifts
            .iter()
            .find(|r| r.end == RiftEnd::Entry)
            .unwrap()
            .pos;
        let exit = w.rifts.iter().find(|r| r.end == RiftEnd::Exit).unwrap().pos;
        w.tanks[0].pos = entry - V2::new(30., 0.);
        w.tanks[0].vel = V2::new(200., 0.);
        w.step();
        assert!(w.tanks[0].pos.distance(exit) < 40., "the tank went through");
        assert!(w.tanks[0].vel.x > 0.);
        // Coming back through the exit does nothing: it only lets things out.
        for _ in 0..60 {
            w.step();
        }
        let near_exit = w.tanks[0].pos.distance(exit) < 200.;
        assert!(near_exit);
        w.tanks[0].pos = V2::new(300., 300.);
        let other = w.add_player("Shooter", false);
        let p = w.simple_projectile(
            other,
            0,
            entry - V2::new(60., 0.),
            V2::new(900., 0.),
            ProjectileKind::Bullet,
            10.,
            6.,
            2.,
            1.,
        );
        let shot = p.id;
        w.projectiles.push(p);
        for _ in 0..6 {
            w.step();
        }
        let p = w.projectiles.iter().find(|p| p.id == shot).unwrap();
        assert!(
            p.pos.distance(exit) < 200.,
            "an opponent's round goes through"
        );
        assert!(p.vel.x > 800.);
    }

    #[test]
    fn a_round_comes_out_of_the_exit_at_its_pace() {
        let (mut w, _) = world();
        open_pair(&mut w);
        let entry = w
            .rifts
            .iter()
            .find(|r| r.end == RiftEnd::Entry)
            .unwrap()
            .pos;
        let exit = w.rifts.iter().find(|r| r.end == RiftEnd::Exit).unwrap().pos;
        let other = w.add_player("Shooter", false);
        let p = w.simple_projectile(
            other,
            0,
            entry - V2::new(60., 0.),
            V2::new(900., 0.),
            ProjectileKind::Bullet,
            10.,
            6.,
            2.,
            1.,
        );
        let shot = p.id;
        w.projectiles.push(p);
        for _ in 0..12 {
            w.step();
            let p = w.projectiles.iter().find(|p| p.id == shot).unwrap();
            if p.pos.distance(exit) < RIFT_RADIUS {
                // Drawn from a step back inside the exit, it keeps its pace.
                assert!((p.pos.distance(p.previous) - 900. * DT).abs() < 0.5);
                assert!((p.pos - p.previous).x > 0.);
                assert!(p.previous.distance(exit) < RIFT_RADIUS);
                return;
            }
        }
        panic!("the round never came out of the exit");
    }

    #[test]
    fn the_owner_goes_through_again_and_again_on_a_map_without_pads() {
        let (mut w, _) = world();
        assert!(w.map.zones.is_empty());
        open_pair(&mut w);
        let entry = w
            .rifts
            .iter()
            .find(|r| r.end == RiftEnd::Entry)
            .unwrap()
            .pos;
        let exit = w.rifts.iter().find(|r| r.end == RiftEnd::Exit).unwrap().pos;
        for trip in 0..4 {
            w.tanks[0].pos = entry - V2::new(20., 0.);
            w.tanks[0].vel = V2::ZERO;
            w.step();
            assert!(w.tanks[0].pos.distance(exit) < 60., "trip {trip}");
            // Off the exit, then rested.
            w.tanks[0].pos = V2::new(300., 300.);
            for _ in 0..((TANK_REST + 0.1) / DT) as usize {
                w.step();
            }
        }
    }

    #[test]
    fn opponents_drones_and_flag_carriers_stay_on_their_side() {
        let (mut w, _) = world();
        open_pair(&mut w);
        let entry = w
            .rifts
            .iter()
            .find(|r| r.end == RiftEnd::Entry)
            .unwrap()
            .pos;
        let rival = w.add_player("Rival", false);
        let rival_index = w.tanks.iter().position(|t| t.id == rival).unwrap();
        w.tanks[rival_index].pos = entry;
        w.tanks[rival_index].protection = 0.;
        w.tanks[rival_index].spawn_immunity = false;
        w.tanks[0].pos = V2::new(300., 300.);
        w.tanks[0].carrying = true;
        let drone = w.simple_projectile(
            rival,
            0,
            entry - V2::new(20., 0.),
            V2::new(300., 0.),
            ProjectileKind::Drone,
            10.,
            6.,
            10.,
            1.,
        );
        let drone_id = drone.id;
        w.projectiles.push(drone);
        w.step();
        assert!(w.tanks[rival_index].pos.distance(entry) < 60.);
        let drone = w.projectiles.iter().find(|p| p.id == drone_id).unwrap();
        assert!(drone.pos.distance(entry) < 60.);
        w.tanks[0].pos = entry;
        w.step();
        assert!(
            w.tanks[0].pos.distance(entry) < 60.,
            "flag carriers can't use rifts"
        );
    }

    #[test]
    fn a_pair_collapses_after_fifteen_seconds_and_pops() {
        let (mut w, id) = world();
        open_pair(&mut w);
        let mut popped = 0;
        for _ in 0..((RIFT_SECONDS + RIFT_CLOSING + 0.2) / DT) as usize {
            w.step();
            // Each pop names the owner, so players can tell whose it was.
            popped += w
                .events
                .iter()
                .filter(|e| matches!(e.kind, EventKind::RiftClose) && e.subject == id)
                .count();
        }
        assert!(w.rifts.is_empty());
        assert_eq!(popped, 2);
    }

    #[test]
    fn a_lone_entry_closes_when_the_window_ends_and_death_or_a_new_tank_closes_rifts() {
        let (mut w, id) = world();
        w.tanks[0].input.ability = true;
        w.step();
        w.tanks[0].input.ability = false;
        fire(&mut w, 0., 600.);
        for _ in 0..30 {
            w.step();
        }
        assert_eq!(w.rifts.len(), 1);
        for _ in 0..((RIFT_ARMED + RIFT_CLOSING + 0.5) / DT) as usize {
            w.step();
        }
        assert!(w.rifts.is_empty(), "the stranded entry collapsed");
        assert_eq!(w.tanks[0].rift_shots, 0);
        w.tanks[0].ability_cooldown = 0.;
        open_pair(&mut w);
        assert!(w.sandbox_class(id, "chronarch"));
        w.step();
        assert!(w.rifts.iter().all(|r| r.life <= 0.));
    }

    #[test]
    fn no_room_gives_the_shot_back_and_rifts_never_open_in_cover() {
        let (mut w, _) = world();
        // A corridor too narrow for a rift.
        w.walls.push(Rect {
            x: 900.,
            y: 1200.,
            w: 1200.,
            h: 60.,
        });
        w.walls.push(Rect {
            x: 900.,
            y: 1340.,
            w: 1200.,
            h: 60.,
        });
        w.tanks[0].input.ability = true;
        w.step();
        w.tanks[0].input.ability = false;
        w.tanks[0].pos = V2::new(1000., 1300.);
        fire(&mut w, 0., 600.);
        for _ in 0..30 {
            w.step();
        }
        assert!(w.rifts.is_empty());
        assert_eq!(w.tanks[0].rift_shots, 2, "the fizzled shot came back");
        // A wall ahead: the rift opens before it, whole.
        w.walls.clear();
        w.walls.push(Rect {
            x: 1500.,
            y: 1000.,
            w: 60.,
            h: 600.,
        });
        w.tanks[0].pos = V2::new(800., 1300.);
        fire(&mut w, 0., 1200.);
        for _ in 0..30 {
            w.step();
        }
        let rift = &w.rifts[0];
        assert!(rift.pos.x + RIFT_RADIUS <= 1500.);
        assert!(
            w.walls
                .iter()
                .all(|&wall| circle_rect(rift.pos, RIFT_RADIUS, wall).is_none())
        );
    }

    #[test]
    fn exits_too_close_to_the_entry_fizzle() {
        let (mut w, _) = world();
        w.tanks[0].input.ability = true;
        w.step();
        w.tanks[0].input.ability = false;
        fire(&mut w, 0., 600.);
        fire(&mut w, 0.05, 620.);
        for _ in 0..30 {
            w.step();
        }
        assert_eq!(w.rifts.len(), 1);
        assert_eq!(w.tanks[0].rift_shots, 1);
    }
}
