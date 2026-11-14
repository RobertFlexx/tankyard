//! Path following and local threat avoidance.
use super::*;

pub(super) fn navigate(
    world: &World,
    tank: &Tank,
    brain: &mut Brain,
    navigation: &mut Navigation,
) -> V2 {
    brain.goal = world
        .config
        .bounds()
        .clamp_circle(brain.goal, tank.radius + 40.);
    brain.path.retain(|point| {
        world
            .config
            .bounds()
            .contains_circle(*point, tank.radius + 12.)
    });
    let goal_distance = tank.pos.distance(brain.goal);
    if goal_distance < 20. {
        return V2::ZERO;
    }
    if clear_line(tank.pos, brain.goal, tank.radius + 4., &world.walls) {
        brain.path.clear();
        return (brain.goal - tank.pos).normalized() * (goal_distance / 90.).min(1.);
    }
    let cell = navigation.cell_of(brain.goal);
    if world.tick >= brain.next_path
        || (brain.path_cell != Some(cell) && world.tick.saturating_sub(brain.last_path) >= 24)
        || (brain.path.is_empty() && world.tick.saturating_sub(brain.last_path) >= 30)
    {
        brain.path = navigation
            .route(tank.pos, brain.goal, &world.walls, tank.radius + 4.)
            .into();
        brain.path_cell = Some(cell);
        brain.last_path = world.tick;
        brain.next_path = world.tick + 120 + tank.id % 45;
    }
    while brain
        .path
        .front()
        .is_some_and(|&waypoint| tank.pos.distance(waypoint) < 32.)
    {
        brain.path.pop_front();
    }
    while brain.path.len() >= 2
        && clear_line(tank.pos, brain.path[1], tank.radius + 4., &world.walls)
    {
        brain.path.pop_front();
    }
    brain
        .path
        .front()
        .map_or(V2::ZERO, |&next| (next - tank.pos).normalized())
}

pub(super) fn local_steering(world: &World, tank: &Tank, class: &ClassDef, seen: &[Seen]) -> V2 {
    let mut steer = V2::ZERO;
    let forward = tank.pos + tank.vel * 0.2;
    for &wall in &world.walls {
        if let Some((normal, depth)) = physics::circle_rect(forward, tank.radius + 20., wall) {
            steer += normal * (depth / 45.).clamp(0., 0.65);
        }
    }
    for &item in seen {
        if let Seen::Tank(index) = item {
            let ally = &world.tanks[index];
            if ally.id == tank.id || !ally.alive || tank.team == 0 || ally.team != tank.team {
                continue;
            }
            let delta = tank.pos - ally.pos;
            let distance = delta.length();
            if distance < tank.radius + ally.radius + 30. {
                steer += delta.normalized() * 0.25;
            }
        } else if let Seen::Target(index) = item {
            if class.multipliers[2] >= 2. {
                continue;
            }
            // Keep a farming approach out of accidental contact-damage duels.
            let target = &world.targets[index];
            let delta = tank.pos - target.pos;
            if delta.length() < target.radius + tank.radius + 12. {
                steer += delta.normalized() * 0.45;
            }
        }
    }
    let bounds = world.config.bounds();
    let margin = tank.radius + 95.;
    if forward.x < bounds.x + margin {
        steer.x += 0.4;
    }
    if forward.y < bounds.y + margin {
        steer.y += 0.4;
    }
    if forward.x > bounds.x + bounds.w - margin {
        steer.x -= 0.4;
    }
    if forward.y > bounds.y + bounds.h - margin {
        steer.y -= 0.4;
    }
    steer.clamp_length(0.9)
}

/// Threat estimate for a projectile relative to a moving tank. Only shots
/// with a plausible future impact influence steering. The tangent direction
/// costs less travel than reversing, which feels more like a skilled player.
pub(super) fn dodge_threat(relative_position: V2, relative_velocity: V2, safety: f32) -> (V2, f32) {
    let speed_sq = relative_velocity.length_squared();
    if speed_sq < 1.0 {
        return (V2::ZERO, 0.0);
    }
    // Minimum distance on the next 0.8 seconds, not merely current distance.
    let arrival = (-relative_position.dot(relative_velocity) / speed_sq).clamp(0.0, 0.80);
    let closest = relative_position + relative_velocity * arrival;
    let distance = closest.length();
    if distance >= safety || arrival <= 0.0 && distance >= safety * 0.6 {
        return (V2::ZERO, 0.0);
    }
    let travel = relative_velocity.normalized();
    let tangent = V2::new(-travel.y, travel.x);
    let side = if tangent.dot(relative_position) >= 0.0 {
        -1.0
    } else {
        1.0
    };
    let strength =
        (1.0 - distance / safety).clamp(0.0, 1.0) * (1.0 - arrival / 1.10).clamp(0.25, 1.0);
    (tangent * side, strength)
}

pub(super) fn projectile_avoidance(world: &World, tank: &Tank, seen: &[Seen]) -> (V2, f32, V2) {
    let mut dodge = V2::ZERO;
    let mut danger: f32 = 0.;
    let mut escape = V2::ZERO;
    for &item in seen {
        let Seen::Projectile(index) = item else {
            continue;
        };
        let projectile = &world.projectiles[index];
        if projectile.frozen.is_some() {
            continue;
        }
        if projectile.owner == tank.id
            || !hostile(tank.team, projectile.team)
            || projectile.damage <= 0.
            || projectile.lifetime <= 0.
            || projectile.penetration <= 0.
        {
            continue;
        }
        let delta = projectile.pos - tank.pos;
        if delta.length_squared() > 340.0 * 340.0
            || !clear_line(tank.pos, projectile.pos, projectile.radius, &world.walls)
        {
            continue;
        }
        let (direction, risk) = dodge_threat(
            delta,
            projectile.vel - tank.vel,
            tank.radius + projectile.radius + 22.0,
        );
        // Collision risk determines whether to dodge even a modest bullet.
        // Damage only increases urgency; it must not suppress lane avoidance.
        let severity = (projectile.damage / (tank.health.max(1.0) * 0.25))
            .sqrt()
            .clamp(1.0, 1.8);
        dodge += direction * risk * severity;
        danger = danger.max((risk * severity).min(1.0));
        let distance = delta.length();
        let drone = projectile.kind.is_drone();
        let approaching_homing = projectile.kind == tankyard_content::ProjectileKind::Homing
            && projectile.vel.dot(delta) < 0.
            && distance
                < projectile.vel.length() * projectile.lifetime + tank.radius + projectile.radius;
        if drone || approaching_homing {
            // A drone can change direction even if its current lane misses.
            // Homing shots already flying away or about to expire are ignored.
            let pursuit = ((340. - distance) / 260.).clamp(0., 1.);
            let away = (tank.pos - projectile.pos).normalized();
            dodge += away * pursuit * severity;
            escape += away * pursuit * 1.4;
            danger = danger.max((pursuit * severity).min(1.));
        }
    }
    (dodge.clamp_length(2.), danger, escape.clamp_length(1.))
}

/// Movement habits that make a bot read as a person: circle-strafing instead
/// of parking at range, never standing perfectly still, and noticing when it
/// is stuck against something and stepping out of it.
pub(super) fn human_motion(
    world: &World,
    tank: &Tank,
    class: &ClassDef,
    brain: &mut Brain,
    movement: V2,
) -> V2 {
    let personality = Personality::of(tank.id);
    let mut movement = movement;
    if brain.orbit == 0. {
        brain.orbit = if noise(tank.id ^ 0x0b17) < 0.5 {
            -1.
        } else {
            1.
        };
    }
    if let Some(target) = brain.target
        && !is_melee(class)
        && matches!(
            brain.state,
            BotState::Attacking | BotState::BossEngagement | BotState::Farming
        )
    {
        // Reverse now and then on a personal rhythm, so strafes can't be read.
        let period = 150 + (personality.patience * 210.) as u64 + tank.id % 60;
        if world
            .tick
            .wrapping_add(tank.id.wrapping_mul(13))
            .is_multiple_of(period)
            && noise(tank.id ^ world.tick) < 0.6
        {
            brain.orbit = -brain.orbit;
        }
        let outward = (tank.pos - target.pos).normalized();
        let tangent = V2::new(-outward.y, outward.x) * brain.orbit;
        let strength = if brain.state == BotState::Farming {
            0.3
        } else {
            0.45 + personality.aggression * 0.2
        };
        movement += tangent * strength * (1. - movement.length() * 0.5).max(0.2);
    }
    if movement.length() < 0.12 {
        // People fidget: a slow, personal drift instead of freezing in place.
        let phase = world.tick as f32 / 95. + noise(tank.id ^ 0xf1d6) * std::f32::consts::TAU;
        movement += V2::from_angle(phase) * 0.32;
    }
    if world.tick.saturating_sub(brain.progress.1) >= 45 {
        let moved = tank.pos.distance(brain.progress.0);
        if tank.input.movement.length() > 0.5 && moved < 22. {
            brain.stuck_checks = brain.stuck_checks.saturating_add(1);
            let away = tank.input.movement.normalized() * -1.;
            let side = V2::new(-away.y, away.x) * brain.orbit;
            brain.unstick = Some(((away * 0.6 + side).normalized(), world.tick + 40));
            brain.orbit = -brain.orbit;
            brain.path.clear();
            brain.next_path = world.tick;
            if brain.stuck_checks >= 3 {
                // This plan isn't working; rethink it instead of grinding.
                brain.initialized = false;
                brain.interest = None;
                brain.stuck_checks = 0;
            }
        } else {
            brain.stuck_checks = 0;
        }
        brain.progress = (tank.pos, world.tick);
    }
    if let Some((direction, until)) = brain.unstick {
        if world.tick < until {
            movement = direction * 0.85 + movement * 0.3;
        } else {
            brain.unstick = None;
        }
    }
    movement
}

pub(super) fn guarded_base_repulsion(world: &World, tank: &Tank) -> V2 {
    let mut push = V2::ZERO;
    for team in 1..=world.config.mode.teams() {
        if team == tank.team || !world.config.base_guarded(team) {
            continue;
        }
        let Some(base) = world.config.team_base(team) else {
            continue;
        };
        let margin = world.config.base_alert_distance() + tank.radius + 140.;
        let distance = base.distance(tank.pos);
        if distance >= margin {
            continue;
        }
        let nearest = V2::new(
            tank.pos.x.clamp(base.x, base.x + base.w),
            tank.pos.y.clamp(base.y, base.y + base.h),
        );
        let away = if distance > 0.5 {
            (tank.pos - nearest) / distance
        } else {
            (tank.pos - base.center()).normalized()
        };
        push += away * (1. - distance / margin) * 2.;
    }
    push
}
