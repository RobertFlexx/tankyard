//! Visible-threat decisions and tactical goals.
use super::*;

pub(super) fn decide(
    world: &World,
    tank: &Tank,
    class: &ClassDef,
    difficulty: Skill,
    seen: &[Seen],
    brain: &mut Brain,
) {
    let health = tank.health / tank.max_health.max(1.);
    let personality = Personality::of(tank.id);
    // Maxed tanks gain little from shapes, so players turn to hunting.
    let hunger = (f32::from(tank.level) / f32::from(tankyard_core::MAX_LEVEL)).min(1.);
    let free_for_all = tank.team == 0 || world.config.mode.teams() == 0;
    // Each bot keeps its own rhythm for changing its mind about where to go.
    let wander_epoch = world.tick / (200 + tank.id % 120);
    let teams = world.config.mode.teams();
    // In team modes, the home base and the enemy base this bot leans toward.
    let front = (teams > 1 && tank.team > 0).then(|| {
        let offset = wander_epoch % u64::from(teams - 1);
        let enemy = 1 + ((u64::from(tank.team) + offset) % u64::from(teams)) as u8;
        (world.team_home(tank.team), world.team_home(enemy))
    });
    let melee = is_melee(class);
    let mut best_target: Option<(f32, TargetSample, BotState)> = None;
    let mut nearest_enemy: Option<(f32, TargetSample)> = None;
    let mut enemy_pressure = 0.;
    let mut support = 0.;
    let mut companion: Option<(f32, V2)> = None;
    let mut threat_center = V2::ZERO;
    let mut nearby_enemies = 0;
    for &item in seen {
        let (sample, utility, state) = match item {
            Seen::Tank(index) => {
                let other = &world.tanks[index];
                if other.id == tank.id
                    || !other.tangible()
                    || other.protection > 0.
                    || world
                        .config
                        .base_protected(other.team, other.pos, other.radius)
                {
                    continue;
                }
                let distance = tank.pos.distance(other.pos);
                if distance > difficulty.perception_radius() {
                    continue;
                }
                let strength = (f32::from(other.level) + 12.) / (f32::from(tank.level) + 12.)
                    * (other.max_health / tank.max_health.max(1.)).sqrt();
                let weak = 1. - (other.health / other.max_health.max(1.)).clamp(0., 1.);
                let visible = clear_line(tank.pos, other.pos, tank.radius * 0.35, &world.walls);
                let mut pressure = strength.clamp(0.3, 2.)
                    * (1. - weak * 0.65)
                    * (1. - distance / 850.).clamp(0., 1.);
                if visible {
                    brain
                        .learning
                        .build
                        .observe_player(other, &world.content, world.tick);
                }
                let shared_threat = tank.last_attacker != 0
                    && tank.last_attacker == other.last_attacker
                    && world.tick.saturating_sub(tank.damaged_at) < 180
                    && world.tick.saturating_sub(other.damaged_at) < 180;
                if visible
                    && distance < 500.
                    && (shared_threat
                        || (tank.team != 0 && other.team == tank.team && brain.danger > 0.15))
                {
                    brain.learning.relationships.cooperate(
                        other.id,
                        if shared_threat { 1. } else { 0.55 },
                        world.tick,
                    );
                }
                if !hostile(tank.team, other.team) {
                    if visible {
                        let friendship = brain
                            .learning
                            .relationships
                            .get(other.id, world.tick)
                            .friendship;
                        support += pressure * (1. + friendship * 0.5);
                        let value = pressure * (0.3 + friendship);
                        if companion.is_none_or(|(best, _)| value > best) {
                            companion = Some((value, other.pos));
                        }
                    }
                    continue;
                }
                if !visible {
                    continue;
                }
                let peaceful = tank.last_attacker != other.id
                    || world.tick.saturating_sub(tank.damaged_at) > 600;
                if free_for_all
                    && distance < 600.
                    && peaceful
                    && brain.target.is_none_or(|target| target.id != other.id)
                {
                    brain.learning.relationships.tolerate(
                        other.id,
                        personality.sociability,
                        world.tick,
                    );
                }
                let motion = brain.learning.observe(other.id, other.pos, world.tick);
                let disposition = brain.learning.relationships.get(other.id, world.tick);
                pressure *= 1. + disposition.fear * 0.4;
                let sample = TargetSample {
                    id: other.id,
                    pos: other.pos,
                    velocity: motion.velocity,
                    radius: other.radius,
                };
                if nearest_enemy.is_none_or(|(previous, _)| distance < previous) {
                    nearest_enemy = Some((distance, sample));
                }
                if pressure > 0. {
                    nearby_enemies += 1;
                    enemy_pressure += pressure;
                    threat_center += other.pos * pressure;
                }
                let awareness = difficulty.awareness();
                let caution = 1. / (1. + (strength - 1.).max(0.) * awareness)
                    * if strength > 1.6 && health < 0.7 {
                        0.35
                    } else {
                        1.
                    };
                let persistence = if brain.target.is_some_and(|target| target.id == other.id) {
                    1.30
                } else {
                    1.0
                };
                // A grudge only encourages a fight the bot can reasonably take.
                // Fear of a stronger player outweighs that urge; neither can
                // reveal a hidden opponent or override recovery/objectives.
                let boldness = noise(tank.id.wrapping_mul(103));
                let manageable =
                    ((health - 0.4) / 0.6).clamp(0., 1.) * (1.8 - strength).clamp(0., 1.);
                let rivalry = (1. + disposition.animosity * manageable * (0.25 + boldness * 0.45))
                    * (1. - disposition.friendship * 0.65);
                let respect = 1.
                    / (1. + disposition.fear * strength.clamp(0.5, 2.) * (0.65 - boldness * 0.5));
                let temperament = (0.8 + personality.aggression * 0.4)
                    * (1. + hunger * personality.aggression * 0.6);
                let utility =
                    (1.15 + weak * 1.2) * caution * persistence * rivalry * respect * temperament
                        / (1. + distance / 420.);
                (sample, utility, BotState::Attacking)
            }
            Seen::Target(index) => {
                if world.config.mode == Mode::CoopPve && tank.team == 2 {
                    continue;
                }
                let target = &world.targets[index];
                if target.health <= 0. {
                    continue;
                }
                let distance = tank.pos.distance(target.pos);
                if distance > difficulty.perception_radius() {
                    continue;
                }
                let efficiency = (target.xp as f32 / target.health.max(20.)).clamp(0.3, 3.);
                let visible = clear_line(tank.pos, target.pos, 4.0, &world.walls);
                if !visible {
                    continue;
                }
                let persistence = if brain
                    .target
                    .is_some_and(|previous| previous.id == target.id)
                {
                    1.22
                } else {
                    1.0
                };
                let mut appetite = 1. - hunger * (0.35 + personality.aggression * 0.4);
                if let Some((home, enemy)) = front {
                    // Farm toward the fight, not in the safety of the base.
                    let across = enemy - home;
                    let progress = ((target.pos - home).dot(across)
                        / across.length_squared().max(1.))
                    .clamp(0., 1.);
                    appetite *= 0.7 + progress * (0.5 + personality.aggression * 0.5);
                }
                let utility =
                    (0.4 + efficiency * 0.18) * persistence * appetite / (1. + distance / 300.);
                (
                    TargetSample {
                        id: target.id,
                        pos: target.pos,
                        velocity: target.vel,
                        radius: target.radius,
                    },
                    utility,
                    BotState::Farming,
                )
            }
            Seen::Boss(index) => {
                let boss = &world.bosses[index];
                if boss.health <= 0. || !hostile(tank.team, boss.team) {
                    continue;
                }
                let distance = tank.pos.distance(boss.pos);
                if distance > difficulty.perception_radius()
                    || !clear_line(tank.pos, boss.pos, tank.radius * 0.35, &world.walls)
                {
                    continue;
                }
                let priority = if matches!(
                    world.config.mode,
                    Mode::BossRush | Mode::Mothership | Mode::CoopPve
                ) {
                    2.0
                } else {
                    0.75
                };
                (
                    TargetSample {
                        id: boss.id,
                        pos: boss.pos,
                        velocity: boss.vel,
                        radius: boss.radius,
                    },
                    priority / (1. + distance / 650.),
                    BotState::BossEngagement,
                )
            }
            Seen::Projectile(_) => continue,
        };
        if best_target
            .as_ref()
            .is_none_or(|(best, _, _)| utility > *best)
        {
            best_target = Some((utility, sample, state));
        }
    }
    if let Some((_, enemy)) = nearest_enemy {
        brain.last_threat = Some((enemy.pos, world.tick));
    }
    if let Some((_, sample, _)) = best_target {
        brain.interest = Some((sample.pos, world.tick));
    }
    let roam = |salt: u64| noise(tank.id.wrapping_mul(salt) ^ wander_epoch.wrapping_mul(salt));
    let mut goal = if let Some((spot, _)) = brain.interest.filter(|(spot, seen)| {
        world.tick.saturating_sub(*seen) < 1500
            && tank.pos.distance(*spot) > 160.
            && roam(59) < 0.35 + personality.curiosity * 0.5
    }) {
        // Go back to where the action was, as a player would.
        spot + V2::new(roam(61) - 0.5, roam(67) - 0.5) * 260.
    } else if let Some((home, enemy)) = front {
        // Team players drift toward the front line rather than idling at home;
        // bolder ones push further.
        let line = home.lerp(enemy, 0.3 + personality.aggression * 0.45);
        let across = (enemy - home).normalized();
        line + V2::new(-across.y, across.x) * (roam(71) - 0.5) * world.config.height * 0.6
    } else {
        V2::new(
            100. + roam(31) * (world.config.width - 200.),
            100. + roam(43) * (world.config.height - 200.),
        )
    };
    let mut state = BotState::Exploring;
    let mut priority = 0.06;
    let mut target = best_target.map(|(_, sample, _)| sample);
    if let Some((utility, sample, action)) = best_target {
        priority = utility;
        state = action;
        let distance = tank.pos.distance(sample.pos);
        let disposition = brain.learning.relationships.get(sample.id, world.tick);
        let preferred = if melee {
            0.
        } else {
            (class.ai_range.clamp(100., 600.)
                * 0.82
                * (0.9 + noise(tank.id.wrapping_mul(53)) * 0.2)
                * (1. + (brain.learning.caution - 0.35) * 0.3)
                * (1. + disposition.fear * 0.22)
                * (1.08 - personality.aggression * 0.16)
                + sample.radius)
                .min(difficulty.perception_radius() * 0.82)
        };
        let outward = (tank.pos - sample.pos).normalized();
        let epoch = world.tick / (180 + tank.id % 121);
        let side = if noise(tank.id ^ epoch.wrapping_mul(17)) > 0.5 {
            1.
        } else {
            -1.
        };
        let strafe = V2::new(-outward.y, outward.x) * side;
        goal = if melee {
            let lead = (distance / (tank.vel.length().max(180.) + 100.)).clamp(0.12, 0.4);
            let intercept = sample.pos + sample.velocity * lead;
            let dangerous_front = world
                .tanks
                .iter()
                .find(|other| other.id == sample.id)
                .is_some_and(|other| {
                    let facing = V2::from_angle(other.aim).dot(outward) > 0.75;
                    facing
                        && world.content.class(&other.class).is_some_and(|enemy| {
                            enemy
                                .barrels
                                .iter()
                                .any(|barrel| barrel.damage >= 25. || barrel.recoil >= 70.)
                        })
                });
            if distance > 155. && dangerous_front {
                intercept + strafe * (distance * 0.3).min(100.)
            } else {
                intercept
            }
        } else if distance > preferred * 1.25 {
            sample.pos + outward * preferred
        } else {
            sample.pos + outward * preferred + strafe * (90. + difficulty.prediction() * 90.)
        };
    }
    let personal_fear = nearest_enemy.map_or(0., |(_, enemy)| {
        brain.learning.relationships.get(enemy.id, world.tick).fear
    });
    // A bot that keeps losing with this class plays it more carefully.
    let doubt = brain
        .learning
        .build
        .verdict(class)
        .map_or(0., |verdict| (-verdict).max(0.) * 0.1);
    let recovering = health
        < difficulty.retreat_health()
            + (brain.learning.caution - 0.35) * 0.12
            + personal_fear * 0.12
            + (0.5 - personality.aggression) * 0.12
            + doubt
        || (brain.state == BotState::Retreating && health < 0.68 - personality.aggression * 0.12);
    // Retreat while a threat is in sight or was seen moments ago. With nobody
    // around, a hurt bot heals while farming and roaming instead of freezing.
    let threatened = nearest_enemy.is_some()
        || brain.last_threat.is_some_and(|(_, seen)| {
            world.tick.saturating_sub(seen) < 150 + (personality.patience * 90.) as u64
        });
    let pressure_limit = difficulty.pressure_limit();
    let outnumbered =
        nearby_enemies >= 2 && health < 0.85 && enemy_pressure > (1. + support) * pressure_limit;
    if (recovering && threatened) || outnumbered {
        priority = 5.;
        state = if recovering {
            BotState::Retreating
        } else {
            BotState::Repositioning
        };
        goal = if recovering && tank.team > 0 && world.config.mode.teams() > 1 {
            world.team_home(tank.team)
        } else if let Some((_, enemy)) = nearest_enemy {
            let threat = if enemy_pressure > 0. {
                threat_center / enemy_pressure
            } else {
                enemy.pos
            };
            tank.pos + (tank.pos - threat).normalized() * 650.
        } else if let Some((threat, _)) = brain.last_threat {
            tank.pos + (tank.pos - threat).normalized() * 500.
        } else {
            brain.goal
        };
        // Cover the escape against the pursuer rather than keeping an old
        // farming target while enemies are in contact range.
        target = nearest_enemy.map(|(_, enemy)| enemy);
    }
    if let Some((_, objective_goal, action)) =
        objective_goal(world, tank, nearest_enemy.map(|(_, enemy)| enemy))
            .filter(|(objective_priority, _, _)| priority < 5. && *objective_priority > priority)
    {
        state = action;
        goal = objective_goal;
    }
    if !melee
        && matches!(state, BotState::Attacking | BotState::BossEngagement)
        && let Some(sample) = target
    {
        let outward = (tank.pos - sample.pos).normalized();
        let tangent = V2::new(-outward.y, outward.x);
        let preferred = if melee {
            60.
        } else {
            class.ai_range.clamp(160., 600.) * 0.85 + sample.radius
        };
        let candidates = [
            goal,
            sample.pos + outward * preferred + tangent * 180.,
            sample.pos + outward * preferred - tangent * 180.,
            sample.pos + outward * preferred * 0.72,
            sample.pos + outward * preferred * 1.25,
            companion.map_or(goal, |(_, pos)| pos + (tank.pos - pos).normalized() * 140.),
        ];
        let legal = std::array::from_fn(|index| {
            (index != 5 || companion.is_some())
                && world.position_clear(candidates[index], tank.radius + 15.)
                && clear_line(tank.pos, candidates[index], tank.radius + 4., &world.walls)
        });
        let bounds = world.config.bounds();
        let border = (tank.pos.x - bounds.x)
            .min(tank.pos.y - bounds.y)
            .min(bounds.x + bounds.w - tank.pos.x)
            .min(bounds.y + bounds.h - tank.pos.y);
        let context = [
            health,
            (enemy_pressure / 3.).min(1.),
            (support / 2.).min(1.),
            (tank.pos.distance(sample.pos) / 900.).min(1.),
            (border / 500.).min(1.),
            (tank.vel.length() / 400.).min(1.),
            f32::from(melee),
            1.,
        ];
        if legal.iter().any(|legal| *legal) {
            let action = brain
                .learning
                .tactics
                .choose(tank, world.tick, context, legal);
            goal = candidates[action];
        }
    }
    if world.config.mode == Mode::Survival {
        let center = V2::new(world.config.width * 0.5, world.config.height * 0.5);
        if tank.pos.distance(center) > world.match_state.safe_radius - tank.radius - 95. {
            state = BotState::Repositioning;
            goal = center;
        }
    }
    if state == BotState::Exploring {
        target = None;
    }
    brain.state = state;
    brain.goal = world.config.bounds().clamp_circle(goal, tank.radius + 48.);
    brain.target = target;
}
