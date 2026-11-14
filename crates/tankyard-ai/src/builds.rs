//! Legal stat spending and class evolution.
use super::*;

pub(super) fn progression_commands(
    world: &World,
    tank: &Tank,
    class: &ClassDef,
    learning: &policy::BuildLearning,
    commands: &mut Vec<Command>,
) {
    let melee = class.multipliers[2] >= 2.;
    let drones = class
        .barrels
        .iter()
        .any(|barrel| barrel.projectile.is_drone());
    let desired = if melee {
        [7, 5, 7, 2, 2, 2, 1, 7]
    } else if drones {
        [5, 3, 0, 2, 6, 7, 7, 3]
    } else if class.has_deployables() {
        [6, 3, 1, 1, 7, 6, 6, 3]
    } else {
        [4, 2, 0, 4, 5, 7, 7, 4]
    };
    let mut points = tank.points;
    let mut stats = tank.stats;
    while points > 0 {
        let next = (0..8)
            .filter(|&index| stats[index] < 7 && stats[index] < desired[index])
            .min_by(|&a, &b| {
                let ratio_a = f32::from(stats[a]) / f32::from(desired[a].max(1))
                    - learning.skill_value(class, stats, a);
                let ratio_b = f32::from(stats[b]) / f32::from(desired[b].max(1))
                    - learning.skill_value(class, stats, b);
                ratio_a.total_cmp(&ratio_b).then_with(|| b.cmp(&a))
            })
            .or_else(|| (0..8).find(|&index| stats[index] < 7));
        let Some(index) = next else {
            break;
        };
        commands.push(Command::Stat(tank.id, index));
        stats[index] += 1;
        points -= 1;
    }
    let eligible = || {
        class.upgrades.iter().filter(|id| {
            world
                .content
                .class(id)
                .is_some_and(|next| tank.level >= next.min_level)
        })
    };
    if let Some(next) = eligible()
        .filter_map(|id| world.content.class(id))
        .max_by(|a, b| {
            learning
                .class_score(tank, a)
                .total_cmp(&learning.class_score(tank, b))
        })
    {
        commands.push(Command::Evolve(tank.id, next.id.clone()));
    }
}
