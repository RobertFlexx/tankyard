//! Arenas: independent matches of bounded size sharing one server process,
//! port and relay. Each arena owns its world, bots, mods and round cycle;
//! peers are pinned to one arena until a server-wide restart regroups them.
use crate::{LocalSession, NetError, UdpServer};
use std::time::Duration;
use tankyard_core::{GameEvent, Rect};

/// Spare arenas linger this long once empty before their memory is released.
const EMPTY_ARENA_SECONDS: u64 = 60;
/// Each arena's entity IDs start at `index << ID_SHIFT`.
const ID_SHIFT: u32 = 32;

pub struct Arena {
    pub session: LocalSession,
    pub(crate) pending_effects: Vec<GameEvent>,
    pub(crate) finished_for: f32,
    pub(crate) empty_since: Option<Duration>,
    pub(crate) walls: WallCache,
}

impl Arena {
    pub(crate) fn new(session: LocalSession) -> Self {
        Self {
            session,
            pending_effects: Vec::with_capacity(1024),
            finished_for: 0.,
            empty_since: None,
            walls: WallCache::default(),
        }
    }
}

/// Walls change on map changes and while borders animate. Snapshots carry
/// them only when they change, then repeat them briefly to survive loss.
#[derive(Default)]
pub(crate) struct WallCache {
    pub revision: u32,
    walls: Vec<Rect>,
    groups: Vec<u8>,
    pub changed_at: Duration,
}
impl WallCache {
    /// Snapshots carry walls where the map places them, so cover that map
    /// logic glides around never makes them resend.
    pub fn refresh(&mut self, walls: &[Rect], groups: &[u8], now: Duration) {
        if self.walls != walls || self.groups != groups {
            self.walls.clear();
            self.walls.extend_from_slice(walls);
            self.groups.clear();
            self.groups.extend_from_slice(groups);
            self.revision = self.revision.wrapping_add(1);
            self.changed_at = now;
        }
    }
}

impl UdpServer {
    pub fn arena_count(&self) -> usize {
        self.arenas.len()
    }
    /// Humans per arena, or zero for one arena of any size.
    pub(crate) fn arena_capacity(&self) -> usize {
        if self.settings.party.is_some() {
            0
        } else {
            self.settings.arena_capacity.min(self.settings.max_players)
        }
    }
    pub(crate) fn max_arenas(&self) -> usize {
        match self.arena_capacity() {
            0 => 1,
            capacity => self.settings.max_players.div_ceil(capacity).clamp(1, 16),
        }
    }
    pub(crate) fn arena_of(&self, client: u64) -> usize {
        self.peers
            .get(&client)
            .map_or(0, |peer| peer.arena)
            .min(self.arenas.len() - 1)
    }
    pub(crate) fn arena_of_entity(&self, entity: tankyard_core::EntityId) -> Option<usize> {
        self.arenas.iter().position(|arena| {
            arena
                .session
                .world
                .tanks
                .iter()
                .any(|tank| tank.id == entity)
        })
    }
    pub(crate) fn arena_humans(&self, arena: usize) -> usize {
        self.peers
            .values()
            .filter(|peer| {
                peer.arena == arena && peer.entity.is_some() && peer.disconnect_at.is_none()
            })
            .count()
    }
    /// Clients connected to one arena, ready for gameplay traffic.
    pub(crate) fn arena_clients(&self, arena: usize) -> Vec<u64> {
        self.peers
            .iter()
            .filter_map(|(&id, peer)| {
                (peer.arena == arena && peer.entity.is_some() && peer.disconnect_at.is_none())
                    .then_some(id)
            })
            .collect()
    }

    /// A lobby the player picked, while it has room; otherwise the fullest
    /// arena with room keeps matches lively, and a new arena opens only when
    /// every existing one is full.
    pub(crate) fn choose_arena(&mut self, preferred: Option<usize>) -> Result<usize, NetError> {
        let capacity = self.arena_capacity();
        if capacity == 0 {
            return Ok(0);
        }
        let counts: Vec<_> = (0..self.arenas.len())
            .map(|i| self.arena_humans(i))
            .collect();
        if let Some(index) = preferred.filter(|&i| i < counts.len() && counts[i] < capacity) {
            return Ok(index);
        }
        if let Some(index) = (0..self.arenas.len())
            .filter(|&i| counts[i] < capacity)
            .max_by_key(|&i| (counts[i], std::cmp::Reverse(i)))
        {
            return Ok(index);
        }
        if self.arenas.len() < self.max_arenas() {
            let session = self.arena_session(self.arenas.len(), 0)?;
            self.arenas.push(Arena::new(session));
            log::info!("opened arena {}", self.arenas.len());
            return Ok(self.arenas.len() - 1);
        }
        Ok((0..self.arenas.len())
            .min_by_key(|&i| counts[i])
            .unwrap_or(0))
    }

    /// A fresh match for an arena, using the server's current settings.
    pub(crate) fn arena_session(&self, index: usize, round: u32) -> Result<LocalSession, NetError> {
        let mut config = self.settings.world.clone();
        config.seed = config
            .seed
            .wrapping_add(u64::from(round))
            .wrapping_add((index as u64).wrapping_mul(0x9e37_79b9));
        config.id_base = (index as u64) << ID_SHIFT;
        let mut session = LocalSession::with_mods(
            config,
            self.settings.desired_bots(0),
            self.loaded_mods.clone(),
        )
        .map_err(NetError)?;
        session.bots.difficulty = self.settings.difficulty;
        if let Some(rules) = self.settings.rules_override {
            session.world.set_rules(rules).map_err(NetError)?;
        }
        for factory in &self.native_plugins {
            session.install_plugin(factory()).map_err(NetError)?;
        }
        Ok(session)
    }

    /// Release trailing arenas that stayed empty. Arena 0 always remains.
    pub(crate) fn trim_arenas(&mut self) {
        for index in 0..self.arenas.len() {
            let empty = self.arena_humans(index) == 0
                && !self
                    .peers
                    .values()
                    .any(|peer| peer.arena == index && peer.disconnect_at.is_none());
            let elapsed = self.elapsed;
            let arena = &mut self.arenas[index];
            arena.empty_since = if empty {
                arena.empty_since.or(Some(elapsed))
            } else {
                None
            };
        }
        while self.arenas.len() > 1
            && self
                .arenas
                .last()
                .and_then(|arena| arena.empty_since)
                .is_some_and(|since| {
                    self.elapsed.saturating_sub(since) >= Duration::from_secs(EMPTY_ARENA_SECONDS)
                })
        {
            self.arenas.pop();
            log::info!("closed empty arena {}", self.arenas.len() + 1);
        }
    }

    /// Server-wide restarts regroup players: each arena's group moves as a
    /// block into the first arena with room, so sparse arenas merge.
    pub(crate) fn regroup(&self, players: &[(u64, usize)]) -> Vec<(u64, usize)> {
        let capacity = self.arena_capacity();
        if capacity == 0 {
            return players.iter().map(|&(client, _)| (client, 0)).collect();
        }
        let mut groups: Vec<Vec<u64>> = vec![Vec::new(); self.arenas.len().max(1)];
        for &(client, arena) in players {
            let last = groups.len() - 1;
            groups[arena.min(last)].push(client);
        }
        let mut load: Vec<usize> = Vec::new();
        let mut placed = Vec::with_capacity(players.len());
        // An overflowed arena is split into full-size parts.
        for group in groups
            .into_iter()
            .flat_map(|group| {
                group
                    .chunks(capacity)
                    .map(<[u64]>::to_vec)
                    .collect::<Vec<_>>()
            })
            .filter(|group| !group.is_empty())
        {
            let target = match load.iter().position(|&used| used + group.len() <= capacity) {
                Some(target) => target,
                None => {
                    load.push(0);
                    load.len() - 1
                }
            };
            load[target] += group.len();
            placed.extend(group.into_iter().map(|client| (client, target)));
        }
        placed
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ServerSettings;

    fn server(capacity: usize) -> UdpServer {
        UdpServer::bind(ServerSettings {
            bind: "127.0.0.1:0".into(),
            bot_count: 0,
            ban_file: String::new(),
            arena_capacity: capacity,
            max_players: 64,
            ..Default::default()
        })
        .unwrap()
    }

    #[test]
    fn groups_merge_into_the_fewest_arenas_without_splitting() {
        let mut server = server(20);
        for _ in 0..3 {
            let session = server.arena_session(server.arenas.len(), 0).unwrap();
            server.arenas.push(Arena::new(session));
        }
        let mut players = Vec::new();
        for (arena, count) in [(0, 5), (1, 14), (2, 3), (3, 9)] {
            for n in 0..count {
                players.push((arena as u64 * 100 + n, arena));
            }
        }
        let placed = server.regroup(&players);
        let mut sizes = [0; 4];
        for (_, arena) in &placed {
            sizes[*arena] += 1;
        }
        assert_eq!(sizes, [19, 12, 0, 0]);
        for (client, arena) in &placed {
            let original = (*client / 100) as usize;
            let mates = placed
                .iter()
                .filter(|(other, _)| (*other / 100) as usize == original)
                .all(|(_, a)| a == arena);
            assert!(mates, "group {original} was split");
        }
    }

    #[test]
    fn arenas_use_disjoint_entity_ids_and_respect_the_player_limit() {
        let server = server(20);
        assert_eq!(server.max_arenas(), 4);
        let first = server.arena_session(0, 0).unwrap();
        let third = server.arena_session(2, 0).unwrap();
        assert!(first.world.targets.iter().all(|t| t.id < 1 << 32));
        assert!(
            third
                .world
                .targets
                .iter()
                .all(|t| t.id >= 2 << 32 && t.id < 3 << 32)
        );
        assert_eq!(
            super::super::UdpServer::bind(ServerSettings {
                bind: "127.0.0.1:0".into(),
                ban_file: String::new(),
                arena_capacity: 0,
                ..Default::default()
            })
            .unwrap()
            .max_arenas(),
            1
        );
    }
}
