//! Shared geometry for team homes, spawning, protection and rendering.
use crate::{Mode, Rect, V2, WorldConfig};

impl WorldConfig {
    /// Team arenas have full-height side bases or four corner bases. Infection
    /// and cooperative modes deliberately have no permanent safe territory.
    pub fn team_base(&self, team: u8) -> Option<Rect> {
        if !matches!(
            self.mode,
            Mode::TwoTeams
                | Mode::FourTeams
                | Mode::Domination
                | Mode::Tag
                | Mode::Mothership
                | Mode::CaptureCore
                | Mode::Deathmatch
                | Mode::KingOfTheHill
        ) || team == 0
            || team > self.mode.teams()
        {
            return None;
        }
        let bounds = self.bounds();
        let (width, height) = if self.mode == Mode::FourTeams {
            (
                bounds.w * 0.18_f32.max(180. / self.width).min(0.3),
                bounds.h * 0.18_f32.max(180. / self.height).min(0.3),
            )
        } else {
            (
                bounds.w * 0.12_f32.max(180. / self.width).min(0.25),
                bounds.h,
            )
        };
        Some(Rect {
            x: bounds.x
                + if team == 2 || team == 3 {
                    bounds.w - width
                } else {
                    0.
                },
            y: bounds.y
                + if self.mode == Mode::FourTeams && (team == 2 || team == 4) {
                    bounds.h - height
                } else {
                    0.
                },
            w: width,
            h: height,
        })
    }

    /// The entire hull must be inside its own base; crossing the line removes
    /// protection immediately. This is independent of stationary spawn immunity.
    pub fn base_protected(&self, team: u8, position: V2, radius: f32) -> bool {
        radius.is_finite()
            && radius >= 0.
            && self
                .team_base(team)
                .is_some_and(|base| base.contains_circle(position, radius))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn bases_follow_animated_bounds_and_require_the_whole_hull() {
        for mode in [Mode::TwoTeams, Mode::FourTeams] {
            for scale in [0.5, 0.73, 1.] {
                let config = WorldConfig {
                    mode,
                    arena_scale: scale,
                    ..Default::default()
                };
                for team in 1..=mode.teams() {
                    let base = config.team_base(team).unwrap();
                    assert!(config.bounds().contains(base.center()));
                    assert!(config.base_protected(team, base.center(), 30.));
                    assert!(!config.base_protected(
                        team,
                        V2::new(base.x + 29., base.center().y),
                        30.
                    ));
                    assert!(!config.base_protected(0, base.center(), 30.));
                    for other in 1..=mode.teams() {
                        if other != team {
                            assert!(!config.base_protected(other, base.center(), 30.));
                        }
                    }
                }
            }
        }
        for mode in [Mode::Ffa, Mode::Infection, Mode::CoopPve, Mode::BossRush] {
            assert!(
                WorldConfig {
                    mode,
                    ..Default::default()
                }
                .team_base(1)
                .is_none()
            );
        }
    }

    #[test]
    fn team_spawn_and_protection_stay_in_clear_bases_on_every_map_and_size() {
        use crate::{V2, World};
        for mode in [Mode::TwoTeams, Mode::FourTeams, Mode::CaptureCore] {
            for map in tankyard_content::builtin_maps() {
                for (width, height) in [(900., 700.), (2400., 1800.), (6000., 4000.)] {
                    for scale in [0.5, 1.] {
                        let mut world = World::new(
                            WorldConfig {
                                mode,
                                width,
                                height,
                                target_count: 0,
                                dynamic_arena: false,
                                spawn_protection: 0.,
                                map: map.id.clone(),
                                ..Default::default()
                            },
                            tankyard_content::Content::builtin(),
                        );
                        world.config.arena_scale = scale;
                        world.scale_map_walls();
                        for team in 1..=mode.teams() {
                            let id = world.add_player("Defender", false);
                            world.assign_team(id, team);
                            let index = world.tanks.iter().position(|tank| tank.id == id).unwrap();
                            let tank = &world.tanks[index];
                            assert!(
                                world.config.base_protected(team, tank.pos, tank.radius),
                                "{} {mode:?} {width} {scale} {team}",
                                map.id
                            );
                            assert!(world.walls.iter().all(|wall| {
                                crate::physics::circle_rect(tank.pos, tank.radius, *wall).is_none()
                            }));
                            let health = tank.health;
                            world.damage_tank(index, 9999., id + 1, V2::new(100., 0.));
                            assert_eq!(world.tanks[index].health, health);
                            assert_eq!(world.tanks[index].vel, V2::ZERO);
                            world.tanks[index].pos = world.config.bounds().center();
                            world.damage_tank(index, 25., id + 1, V2::new(100., 0.));
                            assert_eq!(world.tanks[index].health, health - 25.);
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn live_projectiles_cannot_hurt_a_base_defender_but_do_after_they_leave() {
        use crate::{PlayerInput, V2, World};
        let mut world = World::new(
            WorldConfig {
                mode: Mode::TwoTeams,
                target_count: 0,
                dynamic_arena: false,
                spawn_protection: 0.,
                ..Default::default()
            },
            tankyard_content::Content::builtin(),
        );
        let defender = world.add_player("Defender", false);
        let attacker = world.add_player("Attacker", false);
        world.assign_team(defender, 1);
        world.assign_team(attacker, 2);
        world.tanks[0].pos = world.team_home(1);
        world.tanks[1].pos = world.tanks[0].pos + V2::new(450., 0.);
        let health = world.tanks[0].health;
        world.set_input(
            attacker,
            PlayerInput {
                aim: std::f32::consts::PI,
                fire: true,
                ..Default::default()
            },
        );
        for _ in 0..240 {
            world.step();
        }
        assert_eq!(world.tanks[0].health, health);
        let base = world.config.team_base(1).unwrap();
        world.tanks[0].pos.x = base.x + base.w + world.tanks[0].radius + 2.;
        for _ in 0..150 {
            world.step();
        }
        assert!(world.tanks[0].health < health);
    }
}
