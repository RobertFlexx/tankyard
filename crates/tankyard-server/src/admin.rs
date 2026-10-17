use crate::{AdminReply, NetError, UdpServer, difficulty_from_name, mode_from_name, unix_time};
use serde::{Deserialize, Serialize};
use std::{fs, io::Read, net::IpAddr, path::Path};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct Ban {
    pub ip: IpAddr,
    pub expires_at: u64,
}

pub(crate) fn load_bans(path: &str) -> Result<Vec<Ban>, NetError> {
    if path.is_empty() || !Path::new(path).exists() {
        return Ok(Vec::new());
    }
    let file = fs::File::open(path)?;
    if file.metadata()?.len() > 1024 * 1024 {
        return Err(NetError("ban file exceeds 1 MiB".into()));
    }
    let mut bytes = Vec::new();
    file.take(1024 * 1024 + 1).read_to_end(&mut bytes)?;
    if bytes.len() > 1024 * 1024 {
        return Err(NetError("ban file exceeds 1 MiB".into()));
    }
    let mut bans: Vec<Ban> =
        serde_json::from_slice(&bytes).map_err(|e| NetError(format!("invalid ban list: {e}")))?;
    if bans.len() > 4096 {
        return Err(NetError("ban list exceeds 4096 addresses".into()));
    }
    bans.retain(|b| b.expires_at > unix_time().as_secs());
    Ok(bans)
}

impl UdpServer {
    pub fn admin_command(&mut self, command: &str) -> AdminReply {
        let result = self.execute_admin(command);
        match result {
            Ok(message) => AdminReply {
                message,
                shutdown: self.shutting_down,
            },
            Err(e) => AdminReply {
                message: format!("Error: {e}"),
                shutdown: false,
            },
        }
    }
    fn execute_admin(&mut self, command: &str) -> Result<String, NetError> {
        if command.len() > 512 {
            return Err(NetError("command is too long".into()));
        }
        let mut args = command.split_whitespace();
        let Some(verb) = args.next() else {
            return Ok(String::new());
        };
        validate_arguments(verb, args.clone().count())?;
        match verb {
            "owner-client"=>{
                let client:u64=args.next().ok_or_else(||NetError("expected client ID".into()))?.parse().map_err(|_|NetError("invalid client ID".into()))?;
                self.local_role(client,tankyard_protocol::ServerRole::Owner)?;Ok("Server owner assigned.".into())
            },
            "role"=>{
                let entity:u64=args.next().ok_or_else(||NetError("expected player ID".into()))?.parse().map_err(|_|NetError("invalid player ID".into()))?;
                let role=match args.next().unwrap_or("") {"player"=>tankyard_protocol::ServerRole::Player,"moderator"=>tankyard_protocol::ServerRole::Moderator,"admin"=>tankyard_protocol::ServerRole::Admin,"owner"=>tankyard_protocol::ServerRole::Owner,_=>return Err(NetError("expected player, moderator, admin, or owner".into()))};
                let client=self.peers.iter().find_map(|(id,p)|(p.entity==Some(entity)).then_some(*id)).ok_or_else(||NetError("player was not found".into()))?;
                self.local_role(client,role)?;Ok("Server role updated.".into())
            },
            "reload-mods"=>{self.reload_mods()?;Ok("Mods reloaded; a new round has started.".into())},
            "help" => Ok("Commands: status, players, maps, owner-client <client>, role <entity> <player|moderator|admin|owner>, kick <entity>, ban <entity> [minutes], unban <ip>, bans, bots <count>, difficulty <name>, mode <name>, map <id>, restart, reload-mods, save [path], shutdown".into()),
            "status" => {
                let timing = self.metrics.tick_timing();
                Ok(format!("{} | {} on {} | round {} | {} humans + {} bots | {} AI | tick {} | tick average/p95/p99/max {:.1} / {} / {} / {} us | snapshot last/max {} / {} us | {} projectiles, {} targets | {} snapshots, {} KiB payload | {} skipped snapshots | {} dropped catch-up ticks",
                self.settings.name, self.session.world.config.mode.name(), self.session.world.map.name, self.session.world.match_state.round, self.player_count(), self.session.world.tanks.iter().filter(|t| t.bot).count(), self.settings.difficulty.name(), self.session.world.tick,
                timing.average_micros, timing.p95_micros, timing.p99_micros, self.metrics.max_tick_micros,
                self.metrics.last_snapshot_micros, self.metrics.max_snapshot_micros, self.session.world.projectiles.len(), self.session.world.targets.len(),
                self.metrics.snapshots_sent, self.metrics.snapshot_bytes / 1024, self.metrics.snapshots_skipped, self.metrics.dropped_catchup_ticks))
            }
            "players" => {
                let mut rows: Vec<_> = self.peers.iter().filter_map(|(&client, peer)| {
                    let tank = self.session.world.tanks.iter().find(|t| Some(t.id) == peer.entity)?;
                    Some(format!("entity={} client={} name={:?} level={} team={} score={} ip={}", tank.id, client, tank.name, tank.level, tank.team, tank.score,
                        self.transport.client_addr(client).map(|a| a.ip().to_string()).unwrap_or_default()))
                }).collect(); rows.sort(); Ok(if rows.is_empty() { "No connected human players.".into() } else { rows.join("\n") })
            }
            "maps" => Ok(self.session.bundle.maps.iter().map(|map| format!("{}: {}", map.id, map.name)).collect::<Vec<_>>().join("\n")),
            "kick" | "ban" => {
                let entity: u64 = args.next().ok_or_else(|| NetError("expected player entity ID".into()))?.parse().map_err(|_| NetError("invalid entity ID".into()))?;
                let client = self.peers.iter().find_map(|(&client, peer)| (peer.entity == Some(entity)).then_some(client)).ok_or_else(|| NetError("player was not found".into()))?;
                if verb == "ban" {
                    let minutes: u64 = args.next().unwrap_or("60").parse().map_err(|_| NetError("invalid duration".into()))?;
                    self.ban_client(client, minutes, "Temporarily banned by the server administrator.")
                } else { self.reject(client, "Kicked by the server administrator."); Ok(format!("Kicked entity {entity}.")) }
            }
            "unban" => {
                let ip: IpAddr = args.next().ok_or_else(|| NetError("expected an IP address".into()))?.parse().map_err(|_| NetError("invalid IP address".into()))?;
                self.bans.retain(|b| b.ip != ip); self.save_bans()?; Ok(format!("Removed ban for {ip}."))
            }
            "bans" => Ok(self.bans.iter().filter(|b| b.expires_at > unix_time().as_secs()).map(|b| format!("{} expires at Unix time {}", b.ip, b.expires_at)).collect::<Vec<_>>().join("\n")),
            "bots" => {
                let count: usize = args.next().ok_or_else(|| NetError("expected a bot count".into()))?.parse().map_err(|_| NetError("invalid bot count".into()))?;
                if count > 128 || count + self.settings.max_players > 256 { return Err(NetError("bot count exceeds configured resource limits".into())); }
                self.settings.bot_count = count;
                self.sync_bots();
                self.broadcast_match_settings();
                Ok(if self.settings.adaptive_bots {
                    format!("Bot limit is now {count}; population scales down as players join.")
                } else { format!("Bot population is now {count}.") })
            }
            "mode" => {
                let mode = mode_from_name(args.next().ok_or_else(|| NetError("expected a game mode".into()))?).map_err(NetError)?;
                self.configure_match(mode, &self.settings.world.map.clone(), self.settings.difficulty)?;
                Ok(format!("Started {}.", mode.name()))
            }
            "map" => {
                let map = args.next().unwrap();
                self.configure_match(self.settings.world.mode, map, self.settings.difficulty)?;
                Ok(format!("Started map {}.", self.settings.world.map))
            }
            "restart" => { self.restart_round(); Ok("A new round has started.".into()) }
            "difficulty" => {
                let difficulty = difficulty_from_name(args.next().ok_or_else(|| NetError("expected a bot difficulty".into()))?).map_err(NetError)?;
                self.settings.difficulty = difficulty;
                self.session.bots.difficulty = difficulty;
                self.broadcast_match_settings();
                Ok(format!("Bot difficulty is now {}.", difficulty.name()))
            }
            "save" => { let path = args.next().unwrap_or("server.toml"); self.settings.save(path)?; self.save_bans()?; Ok(format!("Saved configuration to {path}.")) }
            "shutdown" | "quit" => { self.shutdown(); Ok("Server stopped gracefully.".into()) }
            _ => Err(NetError("unknown command; enter help".into())),
        }
    }
    pub(crate) fn ban_client(
        &mut self,
        client: u64,
        minutes: u64,
        reason: &str,
    ) -> Result<String, NetError> {
        if !(1..=525600).contains(&minutes) {
            return Err(NetError("ban duration must be 1 to 525600 minutes".into()));
        }
        let address = self
            .transport
            .client_addr(client)
            .ok_or_else(|| NetError("player address was not found".into()))?;
        let now = unix_time().as_secs();
        self.bans
            .retain(|ban| ban.ip != address.ip() && ban.expires_at > now);
        if self.bans.len() >= 4096 {
            return Err(NetError("ban list is full".into()));
        }
        self.bans.push(Ban {
            ip: address.ip(),
            expires_at: now.saturating_add(minutes * 60),
        });
        self.save_bans()?;
        self.access
            .ban_account(client, now.saturating_add(minutes * 60))
            .map_err(NetError)?;
        self.reject(
            client,
            if reason.is_empty() {
                "Temporarily banned by a server moderator."
            } else {
                reason
            },
        );
        Ok(format!("Banned {} for {minutes} minutes.", address.ip()))
    }
    fn save_bans(&self) -> Result<(), NetError> {
        if self.settings.ban_file.is_empty() {
            return Ok(());
        }
        let path = Path::new(&self.settings.ban_file);
        if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
            fs::create_dir_all(parent)?;
        }
        crate::storage::atomic_write(
            path,
            &serde_json::to_vec_pretty(&self.bans).map_err(|e| NetError(e.to_string()))?,
        )?;
        Ok(())
    }
}

fn validate_arguments(verb: &str, count: usize) -> Result<(), NetError> {
    let valid = match verb {
        "owner-client" | "kick" | "unban" | "bots" | "mode" | "map" | "difficulty" => count == 1,
        "role" => count == 2,
        "ban" => (1..=2).contains(&count),
        "save" => count <= 1,
        "help" | "status" | "players" | "maps" | "bans" | "reload-mods" | "restart"
        | "shutdown" | "quit" => count == 0,
        _ => true,
    };
    if valid {
        Ok(())
    } else {
        Err(NetError(format!(
            "Incorrect arguments for {verb}; enter help."
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn admin_commands_reject_missing_and_trailing_arguments_before_mutation() {
        for (verb, count) in [
            ("mode", 0),
            ("map", 2),
            ("bots", 2),
            ("ban", 3),
            ("role", 1),
            ("shutdown", 1),
            ("restart", 1),
        ] {
            assert!(validate_arguments(verb, count).is_err(), "{verb}");
        }
        for (verb, count) in [
            ("mode", 1),
            ("map", 1),
            ("ban", 1),
            ("ban", 2),
            ("save", 0),
            ("restart", 0),
        ] {
            assert!(validate_arguments(verb, count).is_ok(), "{verb}");
        }
    }
}
