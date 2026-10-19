//! Documented, bounded creator administration for explicitly consenting hosts.
//! The transport checks host policy and a signature on every command before
//! reaching this module. No client UI, credentials, or executable payloads live here.
use crate::{LocalSession, UdpServer};
use serde::{Deserialize, Serialize};
use tankyard_core::{BossKind, EntityId, MAX_LEVEL, V2};
use tankyard_protocol::ServerCommand;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
pub enum Command {
    Boss { kind: BossKind },
    Bots { count: usize },
    God { enabled: bool },
    Ability { unlimited: bool },
    Speed { value: f32 },
    Damage { value: f32 },
    Reload { value: f32 },
    Level { value: u8 },
    Class { id: String },
    Heal,
    FullBuild,
    ClearProjectiles,
    ClearBosses,
    FreezeBots { enabled: bool },
    Teleport { x: f32, y: f32 },
    EliminateEnemies,
    Reset,
    Moderate { command: ServerCommand },
}

pub(crate) fn handle(
    server: &mut UdpServer,
    player: EntityId,
    payload: &[u8],
) -> Result<String, String> {
    let command: Command =
        serde_json::from_slice(payload).map_err(|_| "Invalid creator command.")?;
    creator_index(&server.session, player)?;
    if let Command::Moderate { command } = command {
        if !command.valid() {
            return Err("Invalid moderation command.".into());
        }
        let target = match &command {
            ServerCommand::Kick { player, .. }
            | ServerCommand::Ban { player, .. }
            | ServerCommand::Mute { player, .. } => *player,
            _ => return Err("Creator moderation supports kick, ban, and mute only.".into()),
        };
        if target == player {
            return Err("Select another player.".into());
        }
        let client = server
            .peers
            .iter()
            .find_map(|(&id, peer)| {
                (peer.entity == Some(target) && peer.disconnect_at.is_none()).then_some(id)
            })
            .ok_or("Player is no longer connected.")?;
        match command {
            ServerCommand::Kick { reason, .. } => server.reject(
                client,
                if reason.is_empty() {
                    "Removed by creator moderation."
                } else {
                    &reason
                },
            ),
            ServerCommand::Ban {
                minutes, reason, ..
            } => {
                server
                    .ban_client(client, u64::from(minutes), &reason)
                    .map_err(|e| e.to_string())?;
            }
            ServerCommand::Mute { minutes, .. } => {
                let until = if minutes == 0 {
                    0
                } else {
                    crate::unix_time().as_secs() + u64::from(minutes) * 60
                };
                server.access.mute(client, until)?;
                server.send_voice_status(client);
            }
            _ => unreachable!(),
        }
        return Ok("Creator moderation applied.".into());
    }
    if let Command::Bots { count } = command {
        if count > 128 || count + server.settings.max_players > 256 {
            return Err("Bot limit exceeded.".into());
        }
        server.settings.adaptive_bots = false;
        server.settings.bot_count = count;
        server.sync_bots();
        server.broadcast_match_settings();
        server.session.world.config.modded = true;
        return Ok(format!("Bot population: {count}"));
    }
    let message = apply(&mut server.session, player, command)?;
    server.replicate_events();
    Ok(message)
}

fn creator_index(session: &LocalSession, player: EntityId) -> Result<usize, String> {
    session
        .world
        .tanks
        .iter()
        .position(|t| t.id == player && t.game_creator && !t.bot)
        .ok_or_else(|| "Verified creator player was not found.".into())
}

/// Trusted offline entry point; the network always uses the signed transport.
pub fn apply(
    session: &mut LocalSession,
    player: EntityId,
    command: Command,
) -> Result<String, String> {
    let index = creator_index(session, player)?;
    let world = &mut session.world;
    let valid = match &command {
        Command::Speed { value } | Command::Reload { value } => {
            value.is_finite() && (0.25..=6.).contains(value)
        }
        Command::Damage { value } => value.is_finite() && (0.25..=8.).contains(value),
        Command::Level { value } => (1..=MAX_LEVEL).contains(value),
        Command::Class { id } => world.content.class(id).is_some(),
        Command::Bots { count } => *count <= 128,
        Command::Teleport { x, y } => {
            x.is_finite() && y.is_finite() && world.config.bounds().contains(V2::new(*x, *y))
        }
        Command::Heal => world.tanks[index].alive,
        Command::Moderate { .. } => false,
        _ => true,
    };
    if !valid {
        return Err("Invalid creator command or value outside its allowed range.".into());
    }
    match command {
        Command::Boss { kind } => {
            if world.spawn_boss(kind) == 0 {
                return Err("No clear space for another boss.".into());
            }
        }
        Command::Bots { count } => {
            let current: Vec<_> = world.tanks.iter().filter(|t| t.bot).map(|t| t.id).collect();
            for id in current.iter().skip(count) {
                world.remove_player(*id);
            }
            for _ in current.len()..count {
                world.add_player("Creator bot", true);
            }
        }
        Command::God { enabled } => world.admin_modifiers.entry(player).or_default().god = enabled,
        Command::Ability { unlimited } => {
            world
                .admin_modifiers
                .entry(player)
                .or_default()
                .unlimited_ability = unlimited
        }
        Command::Speed { value } => world.tanks[index].speed_scale = value,
        Command::Damage { value } => world.tanks[index].damage_scale = value,
        Command::Reload { value } => world.tanks[index].reload_scale = value,
        Command::Level { value } => {
            world.config.modded = true;
            world.set_practice_level(player, value);
        }
        Command::Class { id } => {
            world.admin_class(player, &id);
        }
        Command::Heal => world.tanks[index].health = world.tanks[index].max_health,
        Command::FullBuild => world.admin_full_build(player),
        Command::ClearProjectiles => world.projectiles.clear(),
        Command::ClearBosses => {
            let bosses: std::collections::HashSet<_> = world.bosses.iter().map(|b| b.id).collect();
            world.projectiles.retain(|p| !bosses.contains(&p.owner));
            world.bosses.clear();
        }
        Command::FreezeBots { enabled } => world.bots_frozen = enabled,
        Command::Teleport { x, y } => {
            let pos = tankyard_core::physics::clear_circle_position(
                V2::new(x, y),
                world.tanks[index].radius,
                world.config.bounds(),
                &world.walls,
            )
            .ok_or("No clear destination.")?;
            world.tanks[index].pos = pos;
            world.tanks[index].vel = V2::ZERO;
        }
        Command::EliminateEnemies => {
            let team = world.tanks[index].team;
            for tank in &mut world.tanks {
                if tank.id != player
                    && tank.alive
                    && (team == 0 || tank.team == 0 || tank.team != team)
                {
                    tank.health = 0.;
                    tank.last_attacker = player;
                }
            }
            for boss in &mut world.bosses {
                if team == 0 || boss.team == 0 || boss.team != team {
                    boss.health = 0.;
                }
            }
        }
        Command::Reset => {
            world.admin_modifiers.remove(&player);
            world.bots_frozen = false;
            world.tanks[index].speed_scale = 1.;
            world.tanks[index].damage_scale = 1.;
            world.tanks[index].reload_scale = 1.;
        }
        Command::Moderate { .. } => unreachable!(),
    }
    world.config.modded = true;
    Ok("Creator command applied.".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn command_validation_precedes_world_mutation() {
        let mut local = LocalSession::new(
            tankyard_core::WorldConfig {
                target_count: 0,
                spawn_protection: 0.,
                ..Default::default()
            },
            0,
        );
        let player = local.world.add_player("Owner", false);
        assert!(apply(&mut local, player, Command::God { enabled: true }).is_err());
        local.world.tanks[0].game_creator = true;
        for command in [
            Command::Speed { value: f32::NAN },
            Command::Reload { value: 7. },
            Command::Damage { value: 9. },
            Command::Bots { count: 129 },
            Command::Level { value: 0 },
            Command::Class {
                id: "missing".into(),
            },
            Command::Teleport {
                x: f32::INFINITY,
                y: 0.,
            },
        ] {
            assert!(apply(&mut local, player, command).is_err());
            assert!(!local.world.config.modded);
        }
        apply(&mut local, player, Command::Speed { value: 3. }).unwrap();
        apply(&mut local, player, Command::God { enabled: true }).unwrap();
        apply(&mut local, player, Command::FullBuild).unwrap();
        apply(
            &mut local,
            player,
            Command::Class {
                id: "chronarch".into(),
            },
        )
        .unwrap();
        apply(&mut local, player, Command::Bots { count: 4 }).unwrap();
        assert_eq!(local.world.tanks[0].speed_scale, 3.);
        assert_eq!(local.world.tanks[0].stats, [7; 8]);
        assert_eq!(local.world.tanks[0].class, "chronarch");
        assert!(local.world.admin_modifiers[&player].god);
        assert_eq!(local.world.tanks.iter().filter(|t| t.bot).count(), 4);
        apply(&mut local, player, Command::Reset).unwrap();
        assert_eq!(local.world.tanks[0].speed_scale, 1.);
        assert!(local.world.admin_modifiers.is_empty());
    }
}
