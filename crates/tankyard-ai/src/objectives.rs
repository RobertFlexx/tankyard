//! Mode objectives and team roles.
use super::*;

pub(super) fn objective_goal(
    world: &World,
    tank: &Tank,
    observed: Option<TargetSample>,
) -> Option<(f32, V2, BotState)> {
    match world.config.mode {
        Mode::Infection if world.match_state.arena.intermission => None,
        Mode::Infection if tank.team == 1 => {
            if let Some(enemy) = observed {
                return Some((
                    3.5,
                    tank.pos + (tank.pos - enemy.pos).normalized() * 600.,
                    BotState::Repositioning,
                ));
            }
            world
                .tanks
                .iter()
                .filter(|ally| ally.id != tank.id && ally.alive && ally.team == 1)
                .min_by(|a, b| {
                    tank.pos
                        .distance(a.pos)
                        .total_cmp(&tank.pos.distance(b.pos))
                })
                .filter(|ally| tank.pos.distance(ally.pos) > 260.)
                .map(|ally| {
                    (
                        0.85,
                        ally.pos + V2::from_angle(noise(tank.id) * std::f32::consts::TAU) * 130.,
                        BotState::Escorting,
                    )
                })
        }
        Mode::Infection => {
            if observed.is_some() {
                return None;
            }
            // Patrol known terrain instead of reading hidden survivors' locations.
            let patrol = [[0.23, 0.26], [0.75, 0.72], [0.25, 0.74], [0.74, 0.25]];
            let point = patrol[((world.tick / 720 + tank.id) % 4) as usize];
            Some((
                0.9,
                world.config.arena_point(V2::new(
                    world.config.width * point[0],
                    world.config.height * point[1],
                )),
                BotState::Exploring,
            ))
        }
        Mode::BossRush => boss_hunt(world, tank, 1.6),
        Mode::CoopPve => {
            let enemy = world
                .tanks
                .iter()
                .filter(|other| {
                    other.tangible() && other.health > 0.0 && hostile(tank.team, other.team)
                })
                .map(|other| other.pos)
                .chain(
                    world
                        .bosses
                        .iter()
                        .filter(|boss| boss.health > 0.0 && hostile(tank.team, boss.team))
                        .map(|boss| boss.pos),
                )
                .filter(|&position| in_view(world, tank.pos, position))
                .min_by(|a, b| tank.pos.distance(*a).total_cmp(&tank.pos.distance(*b)));
            if let Some(enemy) = enemy {
                return (tank.pos.distance(enemy) > 350.0).then_some((
                    1.1,
                    enemy,
                    BotState::Attacking,
                ));
            }
            let humans = world
                .tanks
                .iter()
                .filter(|other| other.alive && !other.bot && other.team == tank.team)
                .count();
            // Stable assignments split wingmates between friends rather than
            // sending the whole squad after whichever human is nearest.
            world
                .tanks
                .iter()
                .filter(|other| other.alive && !other.bot && other.team == tank.team)
                .nth(tank.id as usize % humans.max(1))
                .filter(|human| tank.pos.distance(human.pos) > 240.)
                .map(|human| {
                    let offset = V2::from_angle(noise(tank.id) * std::f32::consts::TAU) * 130.;
                    (0.8, human.pos + offset, BotState::Escorting)
                })
        }
        Mode::Domination | Mode::KingOfTheHill => world
            .objectives
            .iter()
            .map(|objective| {
                let desired = objective.team != tank.team || objective.contested;
                let priority = if desired {
                    1.5
                } else if world.config.mode == Mode::KingOfTheHill {
                    1.1
                } else {
                    0.12
                };
                let coverage = world
                    .tanks
                    .iter()
                    .filter(|ally| {
                        ally.id != tank.id
                            && ally.alive
                            && ally.team == tank.team
                            && ally.pos.distance(objective.pos) < objective.radius + 240.
                    })
                    .count() as f32;
                let state = if objective.team == tank.team {
                    BotState::Defending
                } else {
                    BotState::Capturing
                };
                (
                    priority
                        / ((1. + tank.pos.distance(objective.pos) / 1800.)
                            * (1. + coverage * 0.45)),
                    objective.pos,
                    state,
                )
            })
            .max_by(|a, b| a.0.total_cmp(&b.0)),
        Mode::CaptureCore => {
            if world
                .objectives
                .iter()
                .any(|objective| objective.carrier == Some(tank.id))
            {
                return Some((4., world.team_home(tank.team), BotState::Escorting));
            }
            let mut best: Option<(f32, V2, BotState)> = None;
            for objective in &world.objectives {
                let candidate = if let Some(carrier) = objective.carrier {
                    if let Some(player) = world
                        .tanks
                        .iter()
                        .find(|player| player.id == carrier && player.alive)
                    {
                        if player.team == tank.team {
                            (
                                1.5,
                                player.pos + (tank.pos - player.pos).normalized() * 110.,
                                BotState::Escorting,
                            )
                        } else {
                            (2.2, player.pos, BotState::Defending)
                        }
                    } else {
                        continue;
                    }
                } else if objective.team != tank.team {
                    (1.75, objective.pos, BotState::Capturing)
                } else if objective.pos.distance(objective.home) > 30. {
                    (2.5, objective.pos, BotState::Defending)
                } else {
                    continue;
                };
                let utility = candidate.0 / (1. + tank.pos.distance(candidate.1) / 2200.);
                let candidate = (utility, candidate.1, candidate.2);
                if best
                    .as_ref()
                    .is_none_or(|previous| candidate.0 > previous.0)
                {
                    best = Some(candidate);
                }
            }
            best
        }
        Mode::Mothership => {
            let friendly = world
                .bosses
                .iter()
                .find(|boss| boss.mothership && boss.team == tank.team);
            if let Some(boss) = friendly {
                let damaged = boss.health / boss.max_health.max(1.) < 0.75;
                if damaged || noise(tank.id) < 0.3 {
                    let priority = if damaged { 1.8 } else { 0.65 };
                    let offset = V2::from_angle(noise(tank.id) * std::f32::consts::TAU)
                        * (boss.radius + 150.);
                    return Some((priority, boss.pos + offset, BotState::Defending));
                }
            }
            // Most healthy-team bots assault the other ship instead of camping
            // indefinitely outside perception range of the opposing command.
            world
                .bosses
                .iter()
                .filter(|boss| boss.mothership && hostile(tank.team, boss.team))
                .filter(|boss| in_view(world, tank.pos, boss.pos))
                .map(|boss| {
                    let distance = tank.pos.distance(boss.pos);
                    let approach =
                        boss.pos + (tank.pos - boss.pos).normalized() * (boss.radius + 300.);
                    (
                        1.35 / (1. + distance / 2500.),
                        approach,
                        BotState::BossEngagement,
                    )
                })
                .max_by(|a, b| a.0.total_cmp(&b.0))
        }
        // Elsewhere a boss draws the arena's veterans, while they are healthy
        // enough to fight it.
        _ if tank.level >= 25 && tank.health > tank.max_health * 0.5 => boss_hunt(world, tank, 1.),
        _ => None,
    }
}

/// Close on the nearest boss, to a spot of this bot's own beside it.
fn boss_hunt(world: &World, tank: &Tank, priority: f32) -> Option<(f32, V2, BotState)> {
    world
        .bosses
        .iter()
        .filter(|boss| boss.health > 0. && hostile(tank.team, boss.team))
        .map(|boss| {
            boss.nearest_part(tank.pos)
                .unwrap_or((boss.pos, boss.radius))
        })
        .filter(|&(position, _)| in_view(world, tank.pos, position))
        .min_by(|a, b| (tank.pos.distance(a.0) - a.1).total_cmp(&(tank.pos.distance(b.0) - b.1)))
        .map(|(position, radius)| {
            let direction = (tank.pos - position).normalized();
            let angle = (noise(tank.id) - 0.5) * 0.65;
            let offset = direction.rotate(angle) * (radius + 220.);
            (
                if tank.pos.distance(position) > radius + 250. {
                    priority
                } else {
                    0.15
                },
                position + offset,
                BotState::BossEngagement,
            )
        })
}

fn in_view(world: &World, from: V2, to: V2) -> bool {
    world
        .walls
        .iter()
        .all(|&wall| physics::sweep_rect(from, to, 5., wall).is_none())
}
