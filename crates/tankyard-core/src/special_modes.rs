//! Asymmetric outbreak rounds and a moving control point share normal combat rules.
use crate::*;
pub const INFECTION_WARMUP: f32 = 8.;
pub const INFECTION_SECONDS: f32 = 180.;
pub const HILL_SECONDS: f32 = 45.;
impl World {
    pub(crate) fn initialize_infection(&mut self) {
        self.mode_timer = INFECTION_WARMUP;
        self.match_state.arena.intermission = true;
        self.match_state.arena.remaining = self.mode_timer;
        self.match_state.announcement = "Infection · waiting for two tanks".into();
    }
    pub(crate) fn update_infection(&mut self) {
        if self.match_state.arena.intermission {
            if self.tanks.len() < 2 {
                self.mode_timer = INFECTION_WARMUP;
                self.match_state.announcement = "Infection · waiting for two tanks".into();
            } else {
                self.mode_timer = (self.mode_timer - DT).max(0.);
                self.match_state.announcement =
                    format!("Outbreak in {}s", self.mode_timer.ceil() as u32);
                if self.mode_timer <= 0. {
                    self.projectiles.clear();
                    for tank in &mut self.tanks {
                        tank.team = 1;
                        tank.alive = true;
                        tank.health = tank.max_health;
                        tank.vel = V2::ZERO;
                        tank.input = PlayerInput::default();
                        tank.respawn_timer = 0.;
                        tank.protection = self.config.spawn_protection.min(3.);
                        tank.spawn_immunity = tank.protection > 0.;
                    }
                    self.match_state.arena.intermission = false;
                    self.match_state.arena.wave = 1;
                    self.mode_timer = if self.config.time_limit > 0. {
                        self.config.time_limit.clamp(30., INFECTION_SECONDS)
                    } else {
                        INFECTION_SECONDS
                    };
                    self.seed_infection();
                }
            }
        } else {
            let survivors = self.tanks.iter().filter(|tank| tank.team == 1).count();
            let infected = self.tanks.iter().filter(|tank| tank.team == 2).count();
            if survivors == 0 && infected > 0 {
                self.finish(Some(2), "The outbreak reached every survivor");
            } else if infected == 0 && survivors >= 2 {
                // Losing the infected connection cannot stall or reset the clock.
                self.seed_infection();
            } else if self.mode_timer <= DT || (infected == 0 && survivors == 1) {
                self.finish(Some(1), "The survivors contained the outbreak");
            }
            self.mode_timer = (self.mode_timer - DT).max(0.);
        }
        let survivors = self.tanks.iter().filter(|tank| tank.team == 1).count();
        let infected = self.tanks.iter().filter(|tank| tank.team == 2).count();
        self.match_state.scores = [survivors as u32, infected as u32, 0, 0];
        self.match_state.arena.remaining = self.mode_timer;
    }
    fn seed_infection(&mut self) {
        let candidates: Vec<_> = self
            .tanks
            .iter()
            .filter(|tank| tank.team == 1)
            .map(|tank| tank.id)
            .collect();
        if candidates.is_empty() {
            return;
        }
        let slot = (self.random() * candidates.len() as f32) as usize;
        let id = candidates[slot.min(candidates.len() - 1)];
        self.assign_team(id, 2);
        self.projectiles.retain(|shot| shot.owner != id);
        if let Some(tank) = self.tanks.iter().find(|tank| tank.id == id) {
            self.match_state.announcement = format!("{} is Patient Zero", tank.display_name());
        }
    }
    pub(crate) fn initialize_hill(&mut self) {
        self.mode_timer = HILL_SECONDS;
        let (pos, radius) = self.hill_location(0);
        let id = self.id();
        self.objectives.push(Objective {
            id,
            pos,
            radius,
            team: 0,
            progress: 0.,
            contested: false,
            carrier: None,
            home: pos,
            return_timer: 0.,
        });
        self.match_state.arena.remaining = self.mode_timer;
        self.match_state.announcement = "Capture the hill · it moves every 45s".into();
    }
    fn hill_location(&self, phase: u8) -> (V2, f32) {
        let positions = [[0.5, 0.5], [0.27, 0.35], [0.73, 0.65]];
        let point = positions[phase as usize % positions.len()];
        let preferred = self.config.arena_point(V2::new(
            self.config.width * point[0],
            self.config.height * point[1],
        ));
        for radius in [150., 110., 75., 42.] {
            let radius = radius * self.map_object_scale();
            if let Some(pos) =
                physics::clear_circle_position(preferred, radius, self.config.bounds(), &self.walls)
            {
                return (pos, radius);
            }
        }
        (self.config.bounds().clamp_circle(preferred, 22.), 22.)
    }
    pub(crate) fn update_hill(&mut self) {
        self.mode_timer = (self.mode_timer - DT).max(0.);
        if self.mode_timer <= 0. {
            self.mode_timer = HILL_SECONDS;
            self.match_state.arena.wave = (self.match_state.arena.wave + 1) % 3;
            let (pos, radius) = self.hill_location(self.match_state.arena.wave);
            for objective in &mut self.objectives {
                objective.pos = pos;
                objective.home = pos;
                objective.radius = radius;
                objective.team = 0;
                objective.progress = 0.;
                objective.contested = false;
            }
            self.match_state.announcement = "The hill has moved".into();
            self.event(EventKind::Capture, pos, 0, 0, 0);
        }
        self.match_state.arena.remaining = self.mode_timer;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn world(mode: Mode) -> World {
        World::new(
            WorldConfig {
                mode,
                target_count: 0,
                dynamic_arena: false,
                spawn_protection: 0.,
                ..Default::default()
            },
            tankyard_content::Content::builtin(),
        )
    }
    fn outbreak(world: &mut World) {
        for _ in 0..(INFECTION_WARMUP * TICK_RATE as f32) as usize + 2 {
            world.step();
        }
        assert!(!world.match_state.arena.intermission);
    }
    #[test]
    fn outbreak_waits_seeds_one_patient_converts_deaths_and_preserves_builds() {
        let mut world = world(Mode::Infection);
        world.add_player("Survivor", true);
        for _ in 0..600 {
            world.step();
        }
        assert!(world.match_state.arena.intermission);
        world.add_player("Teammate", true);
        world.add_player("Third", true);
        outbreak(&mut world);
        assert_eq!(world.tanks.iter().filter(|tank| tank.team == 2).count(), 1);
        let survivor = world.tanks.iter().position(|tank| tank.team == 1).unwrap();
        let level = world.tanks[survivor].level;
        world.tanks[survivor].health = -100.; // Even environmental deaths cannot evade conversion.
        world.step();
        assert_eq!(world.tanks[survivor].team, 2);
        assert_eq!(world.tanks[survivor].level, level);
        for _ in 0..300 {
            world.step();
        }
        assert!(world.tanks[survivor].alive);
        let late = world.add_player("Late arrival", false);
        assert_eq!(
            world
                .tanks
                .iter()
                .find(|tank| tank.id == late)
                .unwrap()
                .team,
            2
        );
    }
    #[test]
    fn infection_has_both_victories_and_recovers_after_patient_disconnect() {
        let mut survivors = world(Mode::Infection);
        for _ in 0..3 {
            survivors.add_player("Tank", true);
        }
        outbreak(&mut survivors);
        let patient = survivors
            .tanks
            .iter()
            .find(|tank| tank.team == 2)
            .unwrap()
            .id;
        survivors.remove_player(patient);
        survivors.step();
        assert_eq!(
            survivors.tanks.iter().filter(|tank| tank.team == 2).count(),
            1
        );
        survivors.mode_timer = DT;
        survivors.step();
        assert_eq!(survivors.match_state.winner, Some(1));
        let mut infected = world(Mode::Infection);
        for _ in 0..2 {
            infected.add_player("Tank", true);
        }
        outbreak(&mut infected);
        for tank in &mut infected.tanks {
            if tank.team == 1 {
                tank.health = -100.;
            }
        }
        infected.step();
        assert_eq!(infected.match_state.winner, Some(2));
    }
    #[test]
    fn hill_is_capturable_contestable_moves_safely_and_has_a_score_victory() {
        let mut world = world(Mode::KingOfTheHill);
        world.add_player("Blue", false);
        world.add_player("Red", false);
        let origin = world.objectives[0].pos;
        world.tanks[0].pos = origin;
        for _ in 0..540 {
            world.step();
        }
        assert_eq!(world.objectives[0].team, 1);
        assert!(world.match_state.scores[0] > 0);
        world.tanks[0].pos = origin + V2::new(400., 0.);
        let unattended = world.match_state.scores[0];
        for _ in 0..120 {
            world.step();
        }
        assert_eq!(world.match_state.scores[0], unattended);
        world.tanks[0].pos = origin;
        world.tanks[1].pos = origin + V2::new(80., 0.);
        let score = world.match_state.scores[0];
        for _ in 0..120 {
            world.step();
        }
        assert!(world.objectives[0].contested);
        assert_eq!(world.match_state.scores[0], score);
        world.mode_timer = DT;
        world.step();
        assert!(world.objectives[0].pos.distance(origin) > 100.);
        assert_eq!(world.objectives[0].team, 0);
        assert!(world.position_clear(world.objectives[0].pos, world.objectives[0].radius));
        world.match_state.scores[0] = world.config.score_limit;
        world.step();
        assert_eq!(world.match_state.winner, Some(1));
    }
    #[test]
    fn moving_hill_fits_every_builtin_map_at_small_and_large_arena_sizes() {
        for map in tankyard_content::builtin_maps() {
            for (width, height) in [(1800., 1200.), (3600., 2600.), (8000., 6000.)] {
                let world = World::new(
                    WorldConfig {
                        mode: Mode::KingOfTheHill,
                        map: map.id.clone(),
                        width,
                        height,
                        target_count: 0,
                        ..Default::default()
                    },
                    tankyard_content::Content::builtin(),
                );
                for phase in 0..3 {
                    let (pos, radius) = world.hill_location(phase);
                    assert!(
                        world.position_clear(pos, radius),
                        "{} {width}x{height} hill {phase}",
                        map.id
                    );
                }
            }
        }
    }
}
