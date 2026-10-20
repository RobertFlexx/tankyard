use crate::*;

pub const PVE_WAVES: u8 = 10;

impl World {
    pub(crate) fn update_arena_events(&mut self) {
        self.mode_timer = (self.mode_timer - DT).max(0.0);
        if self.mode_timer <= 0.0001 {
            if self.match_state.arena.event == ArenaEvent::None {
                let choices: Vec<_> = ArenaEvent::ALL
                    .into_iter()
                    .filter(|event| *event != self.last_arena_event)
                    .collect();
                let index = (self.random() * choices.len() as f32) as usize;
                let event = choices[index.min(choices.len() - 1)];
                self.last_arena_event = event;
                self.match_state.arena.event = event;
                self.mode_timer = 20.0;
                if event == ArenaEvent::ShapeRush {
                    for _ in 0..24 {
                        if self.targets.len() >= 5000 {
                            break;
                        }
                        let count = self.targets.len();
                        self.spawn_target();
                        if self.targets.len() > count {
                            let target = self.targets.last_mut().unwrap();
                            target.kind = TargetKind::Rare;
                            target.xp = target.xp.max(180);
                        }
                    }
                }
            } else {
                self.match_state.arena.event = ArenaEvent::None;
                self.mode_timer = 8.0;
            }
        }
        match self.match_state.arena.event {
            ArenaEvent::Overdrive => {
                for tank in self.tanks.iter_mut().filter(|tank| tank.alive) {
                    tank.boost = tank.boost.max(DT * 2.0);
                }
            }
            ArenaEvent::Regeneration => {
                for tank in self.tanks.iter_mut().filter(|tank| tank.alive) {
                    tank.health = (tank.health + tank.max_health * 0.035 * DT).min(tank.max_health);
                }
            }
            _ => {}
        }
        self.match_state.arena.remaining = self.mode_timer;
        if self.tick.is_multiple_of(15) || self.mode_timer >= 19.99 {
            self.match_state.announcement = format!(
                "{} · {}s",
                self.match_state.arena.event.name(),
                self.mode_timer.ceil() as u32
            );
        }
    }

    pub(crate) fn update_coop_pve(&mut self) {
        // An empty arena waits for players instead of spending waves on bots.
        if !self.tanks.iter().any(|tank| !tank.bot && tank.team == 1) {
            return;
        }
        if self.match_state.arena.intermission {
            self.mode_timer = (self.mode_timer - DT).max(0.0);
            self.match_state.arena.remaining = self.mode_timer;
            if self.mode_timer <= 0.0001 {
                self.start_pve_wave();
            } else if self.tick.is_multiple_of(15) {
                self.match_state.announcement = format!(
                    "Wave {} / {PVE_WAVES} in {}s",
                    self.match_state.arena.wave + 1,
                    self.mode_timer.ceil() as u32
                );
            }
            return;
        }
        let allies = self.tanks.iter().any(|tank| tank.team == 1 && tank.alive);
        if !allies {
            self.finish(Some(2), "Squad eliminated");
            return;
        }
        let enemies = self
            .tanks
            .iter()
            .filter(|tank| tank.team == 2 && tank.alive)
            .count()
            + self
                .bosses
                .iter()
                .filter(|boss| boss.team == 2 && boss.health > 0.0)
                .count();
        let changed = self.match_state.arena.enemies != enemies as u16;
        self.match_state.arena.enemies = enemies as u16;
        if enemies > 0 {
            if changed || self.tick.is_multiple_of(15) {
                self.match_state.announcement = format!(
                    "Wave {} / {PVE_WAVES} · {enemies} enemies",
                    self.match_state.arena.wave
                );
            }
            return;
        }
        // Clear spent enemies and their shots between waves, then revive the squad.
        self.tanks.retain(|tank| tank.team != 2);
        self.projectiles.retain(|projectile| projectile.team != 2);
        let squad: Vec<_> = self
            .tanks
            .iter()
            .filter(|tank| tank.team == 1)
            .map(|tank| tank.id)
            .collect();
        for id in squad {
            let pos = self.spawn_position(1);
            if let Some(tank) = self.tanks.iter_mut().find(|tank| tank.id == id) {
                if !tank.alive {
                    tank.pos = pos;
                    tank.vel = V2::ZERO;
                    tank.input = PlayerInput::default();
                    tank.protection = self.config.spawn_protection;
                    tank.spawn_immunity = tank.protection > 0.0;
                }
                tank.alive = true;
                tank.health = tank.max_health;
                tank.respawn_timer = 0.0;
            }
            let reward = 250 + u32::from(self.match_state.arena.wave) * 125;
            self.award(id, reward, reward);
        }
        if self.match_state.arena.wave >= PVE_WAVES {
            self.finish(Some(1), "All ten waves cleared");
        } else {
            self.match_state.arena.intermission = true;
            self.mode_timer = 8.0;
            self.match_state.arena.remaining = self.mode_timer;
            self.match_state.announcement = "Wave cleared · next wave in 8s".into();
        }
    }

    fn start_pve_wave(&mut self) {
        let wave = self.match_state.arena.wave + 1;
        let allies = self.tanks.iter().filter(|tank| tank.team == 1).count();
        let count = (4 + usize::from(wave) * 2 + allies.min(8))
            .min(36)
            .min(512_usize.saturating_sub(self.tanks.len()));
        let classes: &[&str] = match wave {
            1..=2 => &["fork", "lance", "spray", "anchor"],
            3..=5 => &["rail", "gunner", "breach", "foundry"],
            _ => &["triplet", "booster", "hybrid", "sentryweb", "riposte"],
        };
        for index in 0..count {
            let id = self.add_player(&format!("Raider {}", index + 1), true);
            let pos = self.spawn_position(2);
            let class = classes[(index + usize::from(wave)) % classes.len()];
            let minimum = self.content.class(class).map_or(1, |class| class.min_level);
            self.set_level_unchecked(id, (10 + wave * 3).max(minimum).min(MAX_LEVEL));
            if self.content.class(class).is_some() {
                self.set_class(id, class);
            }
            let priority = if class == "booster" {
                [7, 2, 0, 1, 6, 5, 4, 3]
            } else if matches!(class, "sentryweb" | "anchor") {
                [6, 4, 5, 0, 3, 7, 1, 2]
            } else {
                [6, 5, 3, 4, 7, 0, 1, 2]
            };
            for stat in priority {
                for _ in 0..(2 + wave / 3).min(7) {
                    self.upgrade_stat(id, stat);
                }
            }
            if let Some(tank) = self.tanks.iter_mut().find(|tank| tank.id == id) {
                tank.team = 2;
                tank.pos = pos;
                tank.lives = 0;
            }
        }
        if wave.is_multiple_of(3) || wave == PVE_WAVES {
            let id = self.spawn_boss(match wave {
                3 => BossKind::Sentinel,
                6 => BossKind::Prism,
                9 => BossKind::Bastion,
                _ => BossKind::Eclipse,
            });
            let pos = self.spawn_position(2);
            if let Some(boss) = self.bosses.iter_mut().find(|boss| boss.id == id) {
                boss.team = 2;
                boss.pos = pos;
                boss.max_health = (800.0 + f32::from(wave) * 120.0)
                    * (1.0 + allies.saturating_sub(1).min(7) as f32 * 0.18);
                boss.health = boss.max_health;
            }
        }
        self.match_state.arena.wave = wave;
        self.match_state.arena.intermission = false;
        self.match_state.arena.remaining = 0.0;
        self.match_state.arena.enemies = (count
            + self
                .bosses
                .iter()
                .filter(|boss| boss.health > 0.0 && boss.team == 2)
                .count()) as u16;
        self.match_state.announcement = format!("Wave {wave} / {PVE_WAVES}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tankyard_content::Content;

    fn arena(mode: Mode) -> World {
        World::new(
            WorldConfig {
                mode,
                target_count: 0,
                spawn_protection: 0.0,
                ..Default::default()
            },
            Content::builtin(),
        )
    }

    #[test]
    fn events_rotate_deterministically_with_breaks_and_no_immediate_repeats() {
        let mut first = arena(Mode::RandomEvents);
        let mut second = arena(Mode::RandomEvents);
        let mut previous = ArenaEvent::None;
        for _ in 0..16 {
            first.mode_timer = DT;
            second.mode_timer = DT;
            first.step();
            second.step();
            let event = first.match_state.arena.event;
            assert_ne!(event, ArenaEvent::None);
            assert_ne!(event, previous);
            assert_eq!(event, second.match_state.arena.event);
            assert_eq!(first.match_state.arena.remaining, 20.0);
            first.mode_timer = DT;
            second.mode_timer = DT;
            first.step();
            second.step();
            assert_eq!(first.match_state.arena.event, ArenaEvent::None);
            assert_eq!(first.match_state.arena.remaining, 8.0);
            previous = event;
        }
    }

    #[test]
    fn event_bonuses_apply_only_during_their_event() {
        let mut world = arena(Mode::RandomEvents);
        let id = world.add_player("Pilot", false);
        world.match_state.arena.event = ArenaEvent::DoublePoints;
        world.award(id, 50, 80);
        assert_eq!((world.tanks[0].xp, world.tanks[0].score), (100, 160));
        world.match_state.arena.event = ArenaEvent::None;
        world.award(id, 50, 80);
        assert_eq!((world.tanks[0].xp, world.tanks[0].score), (150, 240));
        world.tanks[0].health = 30.0;
        world.match_state.arena.event = ArenaEvent::Regeneration;
        world.step();
        assert!(world.tanks[0].health > 30.0);
        world.match_state.arena.event = ArenaEvent::Overdrive;
        world.step();
        assert!(world.tanks[0].boost > DT);
        world.match_state.arena.event = ArenaEvent::None;
        world.step();
        world.step();
        assert_eq!(world.tanks[0].boost, 0.0);
    }

    #[test]
    fn shape_rush_adds_a_bounded_number_of_bonus_shapes() {
        let mut world = arena(Mode::RandomEvents);
        // Seed the previous event so the next selection can be checked normally.
        for _ in 0..16 {
            world.mode_timer = DT;
            world.update_arena_events();
            if world.match_state.arena.event == ArenaEvent::ShapeRush {
                break;
            }
            world.mode_timer = DT;
            world.update_arena_events();
        }
        assert_eq!(world.match_state.arena.event, ArenaEvent::ShapeRush);
        assert!(!world.targets.is_empty() && world.targets.len() <= 24);
        assert!(
            world
                .targets
                .iter()
                .all(|target| target.kind == TargetKind::Rare && target.xp >= 180)
        );
        let template = world.targets[0].clone();
        world.targets.resize(5000, template);
        world.match_state.arena.event = ArenaEvent::None;
        world.last_arena_event = ArenaEvent::None;
        for _ in 0..16 {
            world.mode_timer = DT;
            world.update_arena_events();
            assert_eq!(world.targets.len(), 5000);
            if world.match_state.arena.event == ArenaEvent::ShapeRush {
                return;
            }
            world.mode_timer = DT;
            world.update_arena_events();
        }
        panic!("seed did not produce shape rush");
    }

    #[test]
    fn coop_waits_for_players_and_runs_all_ten_waves_with_bosses() {
        let mut world = arena(Mode::CoopPve);
        let timer = world.mode_timer;
        for _ in 0..600 {
            world.step();
        }
        assert_eq!(world.mode_timer, timer);
        world.add_player("Pilot", false);
        world.add_player("Wingman", true);
        assert!(
            world
                .tanks
                .iter()
                .all(|tank| tank.team == 1 && tank.level >= 15)
        );
        for wave in 1..=PVE_WAVES {
            world.mode_timer = DT;
            world.step();
            assert_eq!(world.match_state.arena.wave, wave);
            assert!(!world.match_state.arena.intermission);
            assert!(!world.match_state.finished);
            let enemies = world.tanks.iter().filter(|tank| tank.team == 2).count();
            assert!((1..=36).contains(&enemies));
            let bosses = world.bosses.iter().filter(|boss| boss.health > 0.0).count();
            assert_eq!(
                bosses,
                usize::from(wave.is_multiple_of(3) || wave == PVE_WAVES)
            );
            assert_eq!(
                usize::from(world.match_state.arena.enemies),
                enemies + bosses
            );
            for tank in world.tanks.iter_mut().filter(|tank| tank.team == 2) {
                tank.health = 0.0;
                tank.damaged_at = world.tick;
            }
            for boss in &mut world.bosses {
                boss.health = 0.0;
            }
            world.step();
            assert!(world.tanks.iter().all(|tank| tank.team == 1));
            assert!(world.projectiles.iter().all(|shot| shot.team != 2));
            assert_eq!(world.match_state.finished, wave == PVE_WAVES);
            if wave < PVE_WAVES {
                assert!(world.match_state.arena.intermission);
            }
        }
        assert_eq!(world.match_state.winner, Some(1));
    }

    #[test]
    fn coop_revives_between_waves_preserving_builds_and_does_not_respawn_raiders() {
        let mut world = arena(Mode::CoopPve);
        let human = world.add_player("Pilot", false);
        world.add_player("Wingman", true);
        world.set_level_unchecked(human, 30);
        assert!(world.evolve(human, "hammer"));
        assert!(world.evolve(human, "riposte"));
        assert!(world.upgrade_stat(human, 5));
        let stats = world.tanks[0].stats;
        world.mode_timer = DT;
        world.step();
        world.tanks[0].health = 0.0;
        world.tanks[2].health = 0.0;
        world.step();
        assert!(!world.tanks[0].alive);
        assert!(!world.tanks[2].alive);
        world.tanks[0].respawn_timer = 0.0;
        world.tanks[0].input.ability = true;
        world.tanks[2].respawn_timer = 0.0;
        world.step();
        assert!(!world.tanks[0].alive && !world.tanks[2].alive);
        for tank in world.tanks.iter_mut().filter(|tank| tank.team == 2) {
            tank.health = 0.0;
        }
        world.step();
        assert!(world.tanks[0].alive);
        assert_eq!(world.tanks[0].class, "riposte");
        assert_eq!(world.tanks[0].stats, stats);
        assert!(world.tanks[0].level >= 30);
        assert_eq!(world.tanks[0].health, world.tanks[0].max_health);
        assert!(
            !world.tanks[0].input.ability,
            "a held respawn key must not spend the ability after revival"
        );
        let late = world.add_player("Late join", false);
        assert_eq!(
            world
                .tanks
                .iter()
                .find(|tank| tank.id == late)
                .unwrap()
                .team,
            1
        );
    }

    #[test]
    fn coop_ends_on_a_squad_wipe() {
        let mut world = arena(Mode::CoopPve);
        world.add_player("Pilot", false);
        world.mode_timer = DT;
        world.step();
        world.tanks[0].health = 0.0;
        world.step();
        assert!(world.match_state.finished);
        assert_eq!(world.match_state.winner, Some(2));
    }
}
