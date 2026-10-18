//! Population changes move the border, never the coordinate system or map walls.
use crate::{ArenaEvent, DT, Mode, V2, World};

impl World {
    pub(crate) fn target_capacity(&self) -> usize {
        (self.config.target_count as f32 * self.config.arena_scale.powi(2)).round() as usize
    }
    pub(crate) fn update_arena_size(&mut self) {
        if !self.config.dynamic_arena
            || matches!(
                self.config.mode,
                Mode::Sandbox | Mode::Survival | Mode::BossRush | Mode::CoopPve | Mode::Maze
            )
        {
            return;
        }
        // Count participants through deaths/respawns to avoid border oscillation.
        let count = self.tanks.len();
        if count == 0 {
            return;
        }
        let minimum = 0.65_f32
            .max(900. / self.config.width)
            .max(700. / self.config.height)
            .min(1.);
        let density = ((count.clamp(2, 24) - 2) as f32 / 22.).sqrt();
        let target = minimum + (1. - minimum) * density;
        let old = self.config.arena_scale;
        // Critically damped motion, limited to 24 world units/second per edge.
        let acceleration = (target - old) * 0.36 - self.arena_velocity * 1.2;
        let limit = 48. / self.config.width.max(self.config.height);
        self.arena_velocity = (self.arena_velocity + acceleration * DT).clamp(-limit, limit);
        let next = (old + self.arena_velocity * DT).clamp(minimum, 1.);
        self.config.arena_scale =
            if (next - target).abs() < 0.00001 && self.arena_velocity.abs() < 0.00001 {
                self.arena_velocity = 0.;
                target
            } else {
                next
            };
        let ratio = self.config.arena_scale / old;
        let center = V2::new(self.config.width * 0.5, self.config.height * 0.5);
        let bounds = self.config.bounds();
        if self.tick.is_multiple_of(15) && self.match_state.arena.event != ArenaEvent::ShapeRush {
            let excess = self
                .targets
                .len()
                .saturating_sub(self.target_capacity())
                .min(4);
            for _ in 0..excess {
                if let Some((index, _)) =
                    self.targets.iter().enumerate().max_by(|(_, a), (_, b)| {
                        (a.pos - center)
                            .length_squared()
                            .total_cmp(&(b.pos - center).length_squared())
                    })
                {
                    self.targets.swap_remove(index);
                }
            }
        }
        for objective in &mut self.objectives {
            let at_home =
                objective.carrier.is_none() && objective.pos.distance(objective.home) < 1.;
            objective.home = center + (objective.home - center) * ratio;
            if let Some(clear) = crate::physics::clear_circle_position(
                objective.home,
                objective.radius,
                bounds,
                &self.walls,
            ) {
                objective.home = clear;
            }
            if at_home {
                objective.pos = objective.home;
            }
            objective.pos = bounds.clamp_circle(objective.pos, objective.radius);
        }
        self.targets
            .retain(|target| bounds.contains_circle(target.pos, target.radius + 1.));
        for target in &mut self.targets {
            if let Some(clear) = crate::physics::clear_circle_position(
                target.pos,
                target.radius + 1.,
                bounds,
                &self.walls,
            ) {
                target.pos = clear;
            } else {
                target.health = 0.;
            }
        }
        for boss in &mut self.bosses {
            if let Some(clear) = crate::physics::clear_circle_position(
                boss.pos,
                boss.radius + 1.,
                bounds,
                &self.walls,
            ) {
                boss.pos = clear;
            }
        }
        // A border overtaking a stationary trap must not leave an invisible
        // damaging object outside the playable arena.
        self.projectiles
            .retain(|p| p.kind.is_drone() || bounds.contains_circle(p.pos, p.radius));
    }
}

#[cfg(test)]
mod tests {
    use crate::*;
    use tankyard_content::Content;

    #[test]
    fn borders_ease_in_both_directions_without_respawn_pulsing() {
        let mut world = World::new(
            WorldConfig {
                target_count: 0,
                ..Default::default()
            },
            Content::builtin(),
        );
        let id = world.add_player("Pilot", false);
        let start = world.config.bounds();
        world.update_arena_size();
        assert!(world.config.bounds().w < start.w);
        assert!(start.w - world.config.bounds().w < 0.2);
        for _ in 0..3000 {
            world.update_arena_size();
        }
        let small = world.config.bounds();
        assert!((small.w / start.w - 0.65).abs() < 0.001);
        world.tanks[0].alive = false;
        world.update_arena_size();
        assert!((world.config.bounds().w - small.w).abs() < 0.01);
        for _ in 0..23 {
            world.add_player("Arrival", true);
        }
        for _ in 0..3000 {
            world.update_arena_size();
        }
        assert!(world.config.bounds().w > start.w * 0.999);
        assert!(world.tanks.iter().any(|t| t.id == id));
    }

    #[test]
    fn constraints_and_spawns_use_the_animated_border() {
        let mut world = World::new(
            WorldConfig {
                target_count: 0,
                ..Default::default()
            },
            Content::builtin(),
        );
        world.config.arena_scale = 0.65;
        world.add_player("Pilot", false);
        let bounds = world.config.bounds();
        assert!(bounds.contains_circle(world.tanks[0].pos, world.tanks[0].radius));
        world.tanks[0].pos = V2::new(1., 1.);
        crate::physics::constrain_tank(&mut world.tanks[0], &world.config, &world.walls);
        assert!(bounds.contains_circle(world.tanks[0].pos, world.tanks[0].radius));
    }

    #[test]
    fn a_shrinking_border_cannot_pin_a_hull_inside_a_wall() {
        let mut world = World::new(
            WorldConfig {
                target_count: 0,
                ..Default::default()
            },
            Content::builtin(),
        );
        world.add_player("Pilot", false);
        world.config.arena_scale = 0.65;
        let bounds = world.config.bounds();
        world.walls = vec![Rect {
            x: bounds.x + 10.,
            y: 800.,
            w: 100.,
            h: 600.,
        }];
        world.tanks[0].pos = V2::new(bounds.x + 1., 1000.);
        crate::physics::constrain_tank(&mut world.tanks[0], &world.config, &world.walls);
        assert!(world.position_clear(world.tanks[0].pos, world.tanks[0].radius));
    }
}
