//! Interception, parry timing and ability selection.
use super::*;

#[cfg(test)]
pub(super) fn parry_opportunity(world: &World, tank: &Tank, seen: &[Seen]) -> Option<f32> {
    parry_threat(world, tank, seen).map(|(_, eta)| eta)
}
pub(super) fn parry_threat(world: &World, tank: &Tank, seen: &[Seen]) -> Option<(EntityId, f32)> {
    seen.iter()
        .filter_map(|item| {
            let (id, pos, vel, radius, lifetime) = match *item {
                Seen::Projectile(index) => {
                    let shot = &world.projectiles[index];
                    if shot.owner == tank.id
                        || !hostile(tank.team, shot.team)
                        || shot.damage <= 0.
                        || shot.lifetime <= 0.
                        || shot.penetration <= 0.
                        || shot.frozen.is_some()
                        || matches!(
                            shot.kind,
                            ProjectileKind::Trap
                                | ProjectileKind::Mine
                                | ProjectileKind::Barrier
                                | ProjectileKind::Turret
                        )
                    {
                        return None;
                    }
                    (shot.id, shot.pos, shot.vel, shot.radius, shot.lifetime)
                }
                Seen::Tank(index) => {
                    let attacker = &world.tanks[index];
                    if attacker.id == tank.id
                        || !attacker.alive
                        || attacker.health <= 0.
                        || attacker.protection > 0.
                        || world
                            .config
                            .base_protected(attacker.team, attacker.pos, attacker.radius)
                        || attacker.knockback > 0.
                        || !hostile(tank.team, attacker.team)
                        || !world
                            .content
                            .class(&attacker.class)
                            .is_some_and(ClassDef::is_smasher)
                    {
                        return None;
                    }
                    (attacker.id, attacker.pos, attacker.vel, attacker.radius, 1.)
                }
                _ => return None,
            };
            let delta = pos - tank.pos;
            if delta.length_squared() > 340. * 340.
                || !clear_line(tank.pos, pos, radius, &world.walls)
            {
                return None;
            }
            let velocity = vel - tank.vel;
            let safety = tank.radius + radius + 2.;
            let a = velocity.length_squared();
            let b = 2. * delta.dot(velocity);
            let c = delta.length_squared() - safety * safety;
            if c <= 0. {
                return Some((id, 0.));
            }
            let discriminant = b * b - 4. * a * c;
            if a < 0.01 || b >= 0. || discriminant < 0. {
                return None;
            }
            let eta = (-b - discriminant.sqrt()) / (2. * a);
            ((0. ..=0.3).contains(&eta) && eta < lifetime).then_some((id, eta))
        })
        .min_by(|a, b| a.1.total_cmp(&b.1))
}

pub(super) fn use_ability(class: &ClassDef, tank: &Tank, brain: &Brain, distance: f32) -> bool {
    let health = tank.health / tank.max_health.max(1.);
    let ability = class.ability;
    match ability {
        Ability::None => false,
        Ability::Dash => {
            brain.state == BotState::Retreating
                || if class.multipliers[2] >= 2. {
                    health > 0.5
                        && (80. ..240.).contains(&distance)
                        && brain.danger < 0.55
                        && brain.state == BotState::Attacking
                } else {
                    distance > 100. && distance < 420.
                }
        }
        Ability::Shield => brain.danger > 0.3 || (distance < 350. && health < 0.75),
        Ability::Parry | Ability::CounterParry | Ability::VolleyParry => {
            !brain.parry_miss
                && brain.parry_eta.is_some_and(|eta| {
                    eta <= ability.parry_window().unwrap_or(0.)
                        * if brain.parry_timing > 0. {
                            brain.parry_timing
                        } else {
                            0.85
                        }
                })
        }
        Ability::Heal => health < 0.72 || (brain.state == BotState::Escorting && distance < 500.),
        Ability::Nova => distance < 190. || brain.danger > 0.6,
        Ability::Recall => brain.state == BotState::Retreating || (health < 0.5 && distance < 200.),
        Ability::Cloak => {
            brain.state == BotState::Retreating || (distance > 250. && distance < 650.)
        }
        Ability::Overdrive => brain.target.is_some() && distance < 650.,
        Ability::Blink => {
            brain.state == BotState::Retreating || (distance > 170. && distance < 650.)
        }
        Ability::Fortify => {
            brain.danger > 0.3
                || distance < 230.
                || (matches!(brain.state, BotState::Capturing | BotState::Defending)
                    && tank.pos.distance(brain.goal) < 90.)
        }
        Ability::Magnet => distance < 280. || brain.danger > 0.4,
        Ability::Stasis => brain.danger > 0.45 && distance < 650.,
        Ability::Pocket => health < 0.5 && !tank.carrying,
    }
}

/// Solve the intercept quadratic, including the nearly linear case. No client
/// prediction or hidden bonuses are involved; bots estimate ordinary shot travel.
pub(super) fn intercept(origin: V2, position: V2, velocity: V2, speed: f32) -> V2 {
    let delta = position - origin;
    let a = velocity.length_squared() - speed * speed;
    let b = 2. * delta.dot(velocity);
    let c = delta.length_squared();
    let time = if a.abs() < 0.001 {
        if b.abs() > 0.001 {
            (-c / b).max(0.)
        } else {
            0.
        }
    } else {
        let discriminant = b * b - 4. * a * c;
        if discriminant < 0. {
            0.
        } else {
            let root = discriminant.sqrt();
            let first = (-b - root) / (2. * a);
            let second = (-b + root) / (2. * a);
            [first, second]
                .into_iter()
                .filter(|&t| t >= 0. && t.is_finite())
                .min_by(f32::total_cmp)
                .unwrap_or(0.)
        }
    };
    position + velocity * time.clamp(0., 2.)
}
