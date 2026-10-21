use crate::UdpServer;
use tankyard_protocol::ServerControl;
impl UdpServer {
    pub(crate) fn send_rotation(&mut self, client: u64) {
        let remaining = if self.settings.map_rotation_seconds == 0 {
            0
        } else {
            self.next_rotation
                .saturating_sub(self.elapsed.as_secs())
                .max(1) as u32
        };
        self.send_control(client, &ServerControl::MapRotation { remaining });
    }
    pub(crate) fn update_map_rotation(&mut self) {
        let interval = u64::from(self.settings.map_rotation_seconds);
        let now = self.elapsed.as_secs();
        if interval == 0 || self.rotation_checked_at == Some(now) {
            return;
        }
        self.rotation_checked_at = Some(now);
        if now >= self.next_rotation {
            let maps = &self.session.bundle.maps;
            if !maps.is_empty() {
                let current = maps
                    .iter()
                    .position(|m| m.id == self.settings.world.map)
                    .unwrap_or(0);
                use tankyard_core::Mode;
                let key = match self.settings.world.mode {
                    Mode::Ffa => "ffa",
                    Mode::TwoTeams => "two_teams",
                    Mode::FourTeams => "four_teams",
                    Mode::CaptureCore => "capture_core",
                    Mode::CoopPve => "coop_pve",
                    Mode::BossRush => "boss_rush",
                    Mode::RandomEvents => "random_events",
                    Mode::Mothership => "mothership",
                    Mode::Deathmatch => "deathmatch",
                    Mode::Domination => "domination",
                    Mode::Survival => "survival",
                    Mode::Maze => "maze",
                    Mode::Tag => "tag",
                    Mode::Infection => "infection",
                    Mode::KingOfTheHill => "king_of_the_hill",
                    Mode::Sandbox => "sandbox",
                };
                let next = (1..=maps.len())
                    .map(|step| (current + step) % maps.len())
                    .find(|&i| maps[i].recommended_modes.iter().any(|m| m == key))
                    .unwrap_or((current + 1) % maps.len());
                self.settings.world.map = maps[next].id.clone();
                self.settings.world.seed = self.settings.world.seed.wrapping_add(1);
                self.restart_round();
                self.announce(format!("Map changed to {}", self.settings.world.map));
            }
            // Preserve the process uptime schedule through rounds, empty arenas,
            // and delayed updates. Never execute a burst of missed rotations.
            self.next_rotation = (now / interval + 1).saturating_mul(interval);
        }
        let clients: Vec<_> = self
            .peers
            .iter()
            .filter_map(|(&id, p)| p.entity.map(|_| id))
            .collect();
        for client in clients {
            self.send_rotation(client);
        }
    }
    pub(crate) fn sync_party_bots(&mut self, humans: usize) {
        let party = self.settings.party.as_ref().expect("party settings");
        let teams = self.session.world.config.mode.teams();
        let slots = party.ally_slots.max(humans.min(8));
        if self.session.world.config.mode == tankyard_core::Mode::Infection {
            // Conversion must not replenish the survivor team indefinitely.
            let desired = (slots * 2).saturating_sub(humans);
            let current = self
                .session
                .world
                .tanks
                .iter()
                .filter(|tank| tank.bot)
                .count();
            if current == desired {
                return;
            }
            let mut bots: Vec<_> = self
                .session
                .world
                .tanks
                .iter()
                .filter(|tank| tank.bot)
                .map(|tank| (tank.alive, tank.score, tank.id))
                .collect();
            bots.sort_unstable();
            for (_, _, id) in bots.into_iter().take(current.saturating_sub(desired)) {
                self.session.world.remove_player(id);
            }
            for i in current..desired {
                self.session
                    .world
                    .add_player(&format!("Outbreak {:02}", i + 1), true);
            }
            return;
        }
        // Co-op wave enemies (team 2) remain entirely mode-owned. Stable
        // populations allocate nothing; only an actual replacement builds IDs.
        let first = if teams == 0 { 0 } else { 1 };
        let last = if teams == 0 { 0 } else { teams };
        for team in first..=last {
            let humans_on_team = self
                .session
                .world
                .tanks
                .iter()
                .filter(|t| !t.bot && t.team == team)
                .count();
            let desired = slots.saturating_sub(humans_on_team);
            let current = self
                .session
                .world
                .tanks
                .iter()
                .filter(|t| t.bot && t.team == team)
                .count();
            if current == desired {
                continue;
            }
            let mut ids: Vec<_> = self
                .session
                .world
                .tanks
                .iter()
                .filter(|t| t.bot && t.team == team)
                .map(|t| t.id)
                .collect();
            ids.sort_by_key(|id| {
                let tank = self
                    .session
                    .world
                    .tanks
                    .iter()
                    .find(|t| t.id == *id)
                    .expect("existing bot");
                (tank.alive, tank.score, std::cmp::Reverse(tank.id))
            });
            for id in ids.iter().take(current.saturating_sub(desired)) {
                self.session.world.remove_player(*id);
            }
            for i in ids.len()..desired {
                let id = self.session.world.add_player(
                    &format!(
                        "{} {:02}",
                        if team == 1 { "Wingmate" } else { "Rival" },
                        i + 1
                    ),
                    true,
                );
                if team > 0 {
                    self.session.world.assign_team(id, team);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ServerSettings;
    use std::time::Duration;
    #[test]
    fn rotation_follows_empty_process_uptime_and_survives_round_restarts() {
        let mut server = UdpServer::bind(ServerSettings {
            bind: "127.0.0.1:0".into(),
            bot_count: 0,
            ban_file: String::new(),
            ..Default::default()
        })
        .unwrap();
        let first = server.settings.world.map.clone();
        server.elapsed = Duration::from_secs(7199);
        server.update_map_rotation();
        assert_eq!(server.settings.world.map, first);
        assert_eq!(server.session.world.tick, 0);
        server.elapsed = Duration::from_secs(7200);
        server.update_map_rotation();
        assert_ne!(server.settings.world.map, first);
        assert_eq!(server.next_rotation, 14400);
        server.restart_round();
        assert_eq!(server.next_rotation, 14400);
        let round = server.session.world.match_state.round;
        server.elapsed = Duration::from_secs(36001);
        server.update_map_rotation();
        assert_eq!(server.session.world.match_state.round, round + 1);
        assert_eq!(server.next_rotation, 43200);
        server.update_map_rotation();
        assert_eq!(server.session.world.match_state.round, round + 1);
    }
    #[test]
    fn private_matches_can_disable_rotation() {
        let mut server = UdpServer::bind(ServerSettings {
            bind: "127.0.0.1:0".into(),
            map_rotation_seconds: 0,
            bot_count: 0,
            ban_file: String::new(),
            ..Default::default()
        })
        .unwrap();
        server.elapsed = Duration::from_secs(100000);
        server.update_map_rotation();
        assert_eq!(server.session.world.match_state.round, 1);
    }
}
