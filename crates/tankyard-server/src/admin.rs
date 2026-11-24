use crate::{AdminReply, NetError, UdpServer, difficulty_from_name, mode_from_name, unix_time};
use serde::{Deserialize, Serialize};
use std::{fs, io::Read, net::IpAddr, path::Path};
use tankyard_protocol::{BanRow, MOST_BAN_ROWS, ServerHealth};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct Ban {
    pub ip: IpAddr,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub client: Option<u64>,
    pub expires_at: u64,
    /// Who it was, for the list staff read. Bans saved before this field
    /// existed show their masked address alone.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    /// The verified account banned along with the address, so lifting the
    /// ban lifts both.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub account: Option<String>,
}
impl Ban {
    /// Names this ban to staff without saying where it connects from.
    fn id(&self) -> u64 {
        format!("{}|{:?}|{}", self.ip, self.client, self.expires_at)
            .bytes()
            .fold(0xcbf2_9ce4_8422_2325, |hash, byte| {
                (hash ^ u64::from(byte)).wrapping_mul(0x100_0000_01b3)
            })
    }
    fn row(&self, now: u64) -> BanRow {
        let plain = |text: &str, most: usize| -> String {
            text.chars()
                .filter(|c| !c.is_control())
                .take(most)
                .collect()
        };
        let address = masked_address(self.ip);
        let who = match self.name.as_deref().map(str::trim) {
            Some(name) if !name.is_empty() => {
                plain(&format!("{} ({address})", plain(name, 64)), 96)
            }
            _ => address,
        };
        BanRow {
            id: self.id(),
            who,
            reason: plain(self.reason.as_deref().unwrap_or(""), 160),
            minutes_left: self
                .expires_at
                .saturating_sub(now)
                .div_ceil(60)
                .min(u64::from(u32::MAX)) as u32,
        }
    }
    pub(crate) fn matches(&self, ip: IpAddr, client: u64, shared: bool, now: u64) -> bool {
        self.ip == ip
            && self.expires_at > now
            && match self.client {
                Some(id) => id == client,
                // Ignore legacy address-wide bans on shared relay addresses.
                None => !shared,
            }
    }
}

/// Enough of an address to tell two bans apart, and not enough to find anyone.
fn masked_address(ip: IpAddr) -> String {
    match ip {
        IpAddr::V4(ip) => {
            let [a, b, ..] = ip.octets();
            format!("{a}.{b}.x.x")
        }
        IpAddr::V6(ip) => {
            let [a, b, ..] = ip.segments();
            format!("{a:x}:{b:x}:…")
        }
    }
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
            "central-owner" => match args.next().unwrap_or("status") {
                "on" => self.set_central_owner_admin(true),
                "off" => self.set_central_owner_admin(false),
                "status" => Ok(format!("Central owner administration: {}", self.settings.central_owner_enabled())),
                _ => Err(NetError("expected on, off, or status".into())),
            },
            "creator-admin" => match args.next().unwrap_or("status") {
                "on" => self.set_creator_admin(true),
                "off" => self.set_creator_admin(false),
                "status" => Ok(format!("Creator administration: {}", if self.creator_admin_available() { "enabled by operator" } else { "disabled" })),
                _ => Err(NetError("expected on, off, or status".into())),
            },
            "owner-client"=>{
                let client:u64=args.next().ok_or_else(||NetError("expected client ID".into()))?.parse().map_err(|_|NetError("invalid client ID".into()))?;
                if let Some(public) = args.next() {
                    let expected = self.peers.get(&client).and_then(|peer| peer.admin_key).ok_or_else(|| NetError("Administration handshake required.".into()))?;
                    let encoded: String = expected.iter().map(|byte| format!("{byte:02x}")).collect();
                    if public != encoded { return Err(NetError("Host client public key does not match this connection.".into())); }
                }
                self.local_role(client,tankyard_protocol::ServerRole::Owner)?;Ok("Server owner assigned.".into())
            },
            "role"=>{
                let entity:u64=args.next().ok_or_else(||NetError("expected player ID".into()))?.parse().map_err(|_|NetError("invalid player ID".into()))?;
                let role=match args.next().unwrap_or("") {"player"=>tankyard_protocol::ServerRole::Player,"moderator"=>tankyard_protocol::ServerRole::Moderator,"admin"=>tankyard_protocol::ServerRole::Admin,"owner"=>tankyard_protocol::ServerRole::Owner,_=>return Err(NetError("expected player, moderator, admin, or owner".into()))};
                let client=self.peers.iter().find_map(|(id,p)|(p.entity==Some(entity)).then_some(*id)).ok_or_else(||NetError("player was not found".into()))?;
                self.local_role(client,role)?;Ok("Server role updated.".into())
            },
            "reload-mods"=>{self.reload_mods()?;Ok("Mods reloaded; a new round has started.".into())},
            "help" => Ok("Commands: status, players, maps, owner-client <client> [public-key], role <entity> <player|moderator|admin|owner>, kick <entity>, ban <entity> [minutes], unban <ip>, bans, bots <count>, difficulty <name>, mode <name>, map <id>, boss-rush [classic|all|<id,id,...>], restart, reload-mods, creator-admin [on|off|status], central-owner [on|off|status], save [path], shutdown".into()),
            "status" => {
                let timing = self.metrics.tick_timing();
                let world = &self.arenas[0].session.world;
                let mut status = format!("{} | {} on {} | round {} | {} humans in {} arena{} | {} AI | tick {} | tick average/p95/p99/max {:.1} / {} / {} / {} us | snapshot last/max {} / {} us | {} snapshots, {} KiB payload | {} skipped snapshots | {} deferred notices | {} dropped catch-up ticks",
                self.settings.name, world.config.mode.name(), world.map.name, world.match_state.round, self.player_count(), self.arenas.len(), if self.arenas.len() == 1 { "" } else { "s" }, self.settings.difficulty.name(), world.tick,
                timing.average_micros, timing.p95_micros, timing.p99_micros, self.metrics.max_tick_micros,
                self.metrics.last_snapshot_micros, self.metrics.max_snapshot_micros,
                self.metrics.snapshots_sent, self.metrics.snapshot_bytes / 1024, self.metrics.snapshots_skipped, self.metrics.events_deferred, self.metrics.dropped_catchup_ticks);
                for (index, arena) in self.arenas.iter().enumerate() {
                    let world = &arena.session.world;
                    status.push_str(&format!("\narena {}: {} on {} | round {} | {} humans + {} bots | {} projectiles, {} targets{}",
                        index + 1, world.config.mode.name(), world.map.name, world.match_state.round, self.arena_humans(index),
                        world.tanks.iter().filter(|t| t.bot).count(), world.projectiles.len(), world.targets.len(),
                        if arena.empty_since.is_some() { " | paused" } else { "" }));
                }
                Ok(status)
            }
            "players" => {
                let mut rows: Vec<_> = self.peers.iter().filter_map(|(&client, peer)| {
                    let tank = self.arenas.get(peer.arena)?.session.world.tanks.iter().find(|t| Some(t.id) == peer.entity)?;
                    Some(format!("entity={} client={} arena={} name={:?} level={} team={} score={} ip={}", tank.id, client, peer.arena + 1, tank.name, tank.level, tank.team, tank.score,
                        self.transport.client_addr(client).map(|a| a.ip().to_string()).unwrap_or_default()))
                }).collect(); rows.sort(); Ok(if rows.is_empty() { "No connected human players.".into() } else { rows.join("\n") })
            }
            "maps" => Ok(self.arenas[0].session.bundle.maps.iter().map(|map| format!("{}: {}", map.id, map.name)).collect::<Vec<_>>().join("\n")),
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
            "bans" => Ok(self.bans.iter().filter(|b| b.expires_at > unix_time().as_secs()).map(|b| format!("{}{} expires at Unix time {}", b.ip, b.client.map_or_else(String::new, |id| format!(" client {id}")), b.expires_at)).collect::<Vec<_>>().join("\n")),
            "bots" => {
                let count: usize = args.next().ok_or_else(|| NetError("expected a bot count".into()))?.parse().map_err(|_| NetError("invalid bot count".into()))?;
                if count > 128 || count + self.settings.max_players > 256 { return Err(NetError("bot count exceeds configured resource limits".into())); }
                if let Some(party) = &mut self.settings.party {
                    if !(2..=8).contains(&count) { return Err(NetError("Party team size must be between two and eight.".into())); }
                    party.ally_slots = count;
                }
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
            "rules" => {
                let current = self.arenas[0].session.world.config.rules;
                let Some(field) = args.next() else {
                    return serde_json::to_string_pretty(&current).map_err(|e| NetError(e.to_string()));
                };
                let rules = if field == "reset" && args.clone().count() == 0 {
                    tankyard_mods::GameRules::default()
                } else {
                    let value: f32 = args.next().ok_or_else(|| NetError("Use rules <field> <number>, rules reset, or rules.".into()))?
                        .parse().map_err(|_| NetError("Rule value must be a finite number.".into()))?;
                    if !value.is_finite() { return Err(NetError("Rule value must be finite.".into())); }
                    let mut document = serde_json::to_value(current).map_err(|e| NetError(e.to_string()))?;
                    if document.get(field).is_none() { return Err(NetError("Unknown rule. Enter rules to list field names.".into())); }
                    document[field] = serde_json::json!(value);
                    serde_json::from_value(document).map_err(|e| NetError(e.to_string()))?
                };
                self.configure_rules(rules)?;
                Ok("Rules applied live. Use save to retain them in the server configuration.".into())
            }
            "restart" => { self.restart_round(); Ok("A new round has started.".into()) }
            "boss-rush" => {
                if let Some(lineup) = args.next() {
                    self.settings.world.boss_rush = crate::boss_rush_from_text(lineup).map_err(NetError)?;
                }
                let lineup = self.settings.world.rush_lineup();
                let names: Vec<_> = lineup.iter().map(|kind| kind.name()).collect();
                Ok(format!("Boss Rush lineup ({}): {}. It applies from the next round.", names.len(), names.join(", ")))
            }
            "difficulty" => {
                let difficulty = difficulty_from_name(args.next().ok_or_else(|| NetError("expected a bot difficulty".into()))?).map_err(NetError)?;
                self.settings.difficulty = difficulty;
                for arena in &mut self.arenas {
                    arena.session.bots.difficulty = difficulty;
                }
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
        let scope = self.settings.shared_relay(address.ip()).then_some(client);
        let previous = self.bans.clone();
        self.bans
            .retain(|ban| (ban.ip != address.ip() || ban.client != scope) && ban.expires_at > now);
        if self.bans.len() >= 4096 {
            return Err(NetError("ban list is full".into()));
        }
        let name = self.peers.get(&client).and_then(|peer| {
            let world = &self.arenas.get(peer.arena)?.session.world;
            let tank = world.tanks.iter().find(|t| Some(t.id) == peer.entity)?;
            Some(tank.name.clone())
        });
        self.bans.push(Ban {
            ip: address.ip(),
            client: scope,
            expires_at: now.saturating_add(minutes * 60),
            name,
            reason: (!reason.is_empty()).then(|| reason.to_owned()),
            account: self.access.account_of(client),
        });
        if let Err(error) = self.save_bans() {
            self.bans = previous;
            return Err(error);
        }
        if let Err(error) = self
            .access
            .ban_account(client, now.saturating_add(minutes * 60))
        {
            self.bans = previous;
            if let Err(restore) = self.save_bans() {
                log::error!("Could not restore previous ban file: {restore}");
            }
            return Err(NetError(error));
        }
        self.reject(
            client,
            if reason.is_empty() {
                "Temporarily banned by a server moderator."
            } else {
                reason
            },
        );
        Ok(if scope.is_some() {
            format!(
                "Banned client {client} and its verified account for {minutes} minutes; shared relay address preserved."
            )
        } else {
            format!("Banned {} for {minutes} minutes.", address.ip())
        })
    }
    /// The bans in force, longest first.
    pub(crate) fn ban_rows(&self) -> Vec<BanRow> {
        let now = unix_time().as_secs();
        let mut live: Vec<_> = self.bans.iter().filter(|b| b.expires_at > now).collect();
        live.sort_by_key(|ban| std::cmp::Reverse(ban.expires_at));
        live.into_iter()
            .take(MOST_BAN_ROWS)
            .map(|ban| ban.row(now))
            .collect()
    }
    /// End one ban early, for its address and any account it covers.
    pub(crate) fn lift_ban(&mut self, id: u64) -> Result<(), NetError> {
        let index = self
            .bans
            .iter()
            .position(|ban| ban.id() == id)
            .ok_or_else(|| NetError("That ban has already ended.".into()))?;
        let lifted = self.bans.remove(index);
        if let Err(error) = self.save_bans() {
            self.bans.insert(index, lifted);
            return Err(error);
        }
        if let Some(account) = &lifted.account
            && let Err(error) = self.access.lift_account_ban(account)
        {
            // The account save rolls its own state back. Restore the
            // address ban too, so staff can retry the same operation.
            self.bans.insert(index, lifted);
            if let Err(restore) = self.save_bans() {
                return Err(NetError(format!(
                    "Account ban was not lifted: {error}. The address ban remains active, \
                         but its file could not be restored: {restore}"
                )));
            }
            return Err(NetError(error));
        }
        Ok(())
    }
    /// How this server is running, for the staff panel.
    pub(crate) fn health(&self) -> ServerHealth {
        let short = |n: usize| n.min(usize::from(u16::MAX)) as u16;
        let timing = self.metrics.tick_timing();
        let now = unix_time().as_secs();
        ServerHealth {
            uptime_seconds: self.elapsed.as_secs(),
            humans: short(self.player_count()),
            bots: short(
                self.arenas
                    .iter()
                    .map(|a| a.session.world.tanks.iter().filter(|t| t.bot).count())
                    .sum(),
            ),
            arenas: short(self.arenas.len()),
            bosses: short(
                self.arenas
                    .iter()
                    .map(|a| a.session.world.bosses.len())
                    .sum(),
            ),
            tick_average_micros: timing.average_micros as u32,
            tick_p95_micros: timing.p95_micros.min(u64::from(u32::MAX)) as u32,
            tick_budget_micros: (tankyard_core::DT * 1_000_000.) as u32,
            snapshot_kib: self.metrics.snapshot_bytes / 1024,
            rejected_packets: self.metrics.rejected_packets,
            bans: short(self.bans.iter().filter(|b| b.expires_at > now).count()),
        }
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
        "kick" | "unban" | "bots" | "mode" | "map" | "difficulty" => count == 1,
        "owner-client" => (1..=2).contains(&count),
        "role" => count == 2,
        "ban" => (1..=2).contains(&count),
        "save" | "creator-admin" | "central-owner" | "boss-rush" => count <= 1,
        "rules" => count <= 2,
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
    fn lifting_a_ban_preserves_both_records_when_either_save_fails() {
        let root = std::env::temp_dir().join(format!(
            "tankyard-lift-ban-{}",
            u64::from_le_bytes(renet_netcode::generate_random_bytes())
        ));
        let access_dir = root.join("accounts");
        fs::create_dir_all(&access_dir).unwrap();
        let mut settings = crate::ServerSettings {
            bind: "127.0.0.1:0".into(),
            server_id: "ban-rollback".into(),
            access_file: access_dir.join("access.json").to_string_lossy().into(),
            ban_file: root.join("bans.json").to_string_lossy().into(),
            audit_file: String::new(),
            bot_count: 0,
            adaptive_bots: false,
            bot_memory_file: Some(String::new()),
            ..Default::default()
        };
        settings.world.target_count = 0;
        let mut server = UdpServer::bind(settings).unwrap();
        let now = unix_time().as_secs();
        let claim = || {
            Some(crate::access::VerifiedAccount {
                id: "banned_account".into(),
                expires_at: now + 3600,
            })
        };
        server.access.add(1, claim(), now).unwrap();
        server.access.ban_account(1, now + 600).unwrap();
        server.access.remove(1);
        server.bans.push(Ban {
            ip: "203.0.113.7".parse().unwrap(),
            client: None,
            expires_at: now + 600,
            name: None,
            reason: None,
            account: Some("banned_account".into()),
        });
        server.save_bans().unwrap();
        let id = server.bans[0].id();
        let ban_path = root.join("bans.json");
        let saved_bans = root.join("saved-bans.json");
        fs::rename(&ban_path, &saved_bans).unwrap();
        fs::create_dir(&ban_path).unwrap();
        assert!(server.lift_ban(id).is_err());
        assert_eq!(server.bans.len(), 1);
        assert!(server.access.add(2, claim(), now).is_err());
        fs::remove_dir(&ban_path).unwrap();
        fs::rename(&saved_bans, &ban_path).unwrap();

        let saved_accounts = root.join("saved-accounts");
        fs::rename(&access_dir, &saved_accounts).unwrap();
        fs::write(&access_dir, b"not a directory").unwrap();
        assert!(server.lift_ban(id).is_err());
        assert_eq!(server.bans.len(), 1);
        assert_eq!(load_bans(&server.settings.ban_file).unwrap().len(), 1);
        assert!(server.access.add(2, claim(), now).is_err());
        fs::remove_file(&access_dir).unwrap();
        fs::rename(&saved_accounts, &access_dir).unwrap();

        server.lift_ban(id).unwrap();
        assert!(server.bans.is_empty());
        assert!(load_bans(&server.settings.ban_file).unwrap().is_empty());
        server.access.add(2, claim(), now).unwrap();
        drop(server);
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn shared_relay_bans_cannot_block_another_client_or_revive_legacy_blanket_bans() {
        let ip = "127.0.0.1".parse().unwrap();
        let ban = Ban {
            ip,
            client: Some(42),
            expires_at: 200,
            name: None,
            reason: None,
            account: None,
        };
        assert!(ban.matches(ip, 42, true, 100));
        assert!(!ban.matches(ip, 43, true, 100));
        assert!(!ban.matches(ip, 42, true, 201));
        let legacy = Ban {
            client: None,
            ..ban
        };
        assert!(!legacy.matches(ip, 42, true, 100));
        assert!(legacy.matches(ip, 42, false, 100));
        let decoded: Ban = serde_json::from_str(r#"{"ip":"127.0.0.1","expires_at":200}"#).unwrap();
        assert!(!decoded.matches(ip, 43, true, 100));
    }
    #[test]
    fn a_ban_row_masks_the_address_and_keeps_old_ban_files_readable() {
        let old: Ban = serde_json::from_str(r#"{"ip":"203.0.113.77","expires_at":700}"#).unwrap();
        let row = old.row(100);
        assert_eq!(row.who, "203.0.x.x");
        assert_eq!(row.minutes_left, 10);
        assert!(row.valid());

        let named = Ban {
            name: Some("Mira\u{202e}".repeat(40)),
            reason: Some("Griefing\nteammates".into()),
            ip: "2001:db8::5".parse().unwrap(),
            ..old
        };
        let row = named.row(699);
        assert!(!row.who.contains("db8::5"));
        assert!(row.who.contains("2001:db8"));
        assert_eq!(row.minutes_left, 1);
        assert_eq!(row.reason, "Griefingteammates");
        assert!(row.valid(), "{row:?}");
        assert_ne!(
            named.id(),
            Ban {
                expires_at: 701,
                ..named.clone()
            }
            .id()
        );
    }
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
