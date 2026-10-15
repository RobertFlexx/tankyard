use std::collections::{BTreeSet, HashMap, VecDeque};
use tankyard_protocol::{ChatMessage, RoomInfo, ServerKind, ServerRole};
struct Room {
    info: RoomInfo,
    minimum_role: ServerRole,
    members: BTreeSet<u64>,
    history: VecDeque<ChatMessage>,
}
pub struct Chat {
    enabled: bool,
    rooms: HashMap<String, Room>,
    limits: HashMap<u64, (f32, u64)>,
    next_message: u64,
}
impl Chat {
    pub fn new(kind: ServerKind, enabled: bool) -> Self {
        let enabled = kind == ServerKind::Community && enabled;
        let mut chat = Self {
            enabled,
            rooms: HashMap::new(),
            limits: HashMap::new(),
            next_message: 1,
        };
        if enabled {
            chat.create("lobby".into(), "Lobby".into(), ServerRole::Player)
                .unwrap();
        }
        chat
    }
    pub fn enabled(&self) -> bool {
        self.enabled
    }
    pub fn create(
        &mut self,
        id: String,
        name: String,
        minimum_role: ServerRole,
    ) -> Result<(), String> {
        if !self.enabled {
            return Err("Chat is disabled on this server.".into());
        }
        if self.rooms.len() >= 8
            || self.rooms.contains_key(&id)
            || !tankyard_mods::identifier(&id)
            || name.is_empty()
            || name.len() > 48
            || name.chars().any(char::is_control)
        {
            return Err("Invalid room or room limit reached.".into());
        }
        self.rooms.insert(
            id.clone(),
            Room {
                info: RoomInfo { id, name },
                minimum_role,
                members: BTreeSet::new(),
                history: VecDeque::with_capacity(32),
            },
        );
        Ok(())
    }
    pub fn delete(&mut self, id: &str) -> Result<(), String> {
        if id == "lobby" {
            return Err("The lobby cannot be deleted.".into());
        }
        let room = self.rooms.remove(id).ok_or("Room not found.")?;
        if let Some(lobby) = self.rooms.get_mut("lobby") {
            lobby.members.extend(room.members);
        }
        Ok(())
    }
    pub fn list(&self, role: ServerRole) -> Vec<RoomInfo> {
        let mut rooms: Vec<_> = self
            .rooms
            .values()
            .filter(|r| r.minimum_role <= role)
            .map(|r| r.info.clone())
            .collect();
        rooms.sort_by(|a, b| a.id.cmp(&b.id));
        rooms
    }
    pub fn join(
        &mut self,
        client: u64,
        id: &str,
        role: ServerRole,
    ) -> Result<Vec<ChatMessage>, String> {
        let room = self.rooms.get(id).ok_or("Room not found.")?;
        if role < room.minimum_role {
            return Err("Room permission denied.".into());
        }
        if room.members.len() >= 128 && !room.members.contains(&client) {
            return Err("Room is full.".into());
        }
        let history = room.history.iter().cloned().collect();
        for room in self.rooms.values_mut() {
            room.members.remove(&client);
        }
        self.rooms.get_mut(id).unwrap().members.insert(client);
        Ok(history)
    }
    pub fn refresh_role(&mut self, client: u64, role: ServerRole) {
        let lost_access = self
            .rooms
            .values()
            .any(|r| r.members.contains(&client) && r.minimum_role > role);
        if lost_access {
            let _ = self.join(client, "lobby", role);
        }
    }
    pub fn remove(&mut self, client: u64) {
        for r in self.rooms.values_mut() {
            r.members.remove(&client);
        }
        self.limits.remove(&client);
    }
    pub fn can_read(&self, client: u64, id: &str, role: ServerRole) -> bool {
        self.rooms
            .get(id)
            .is_some_and(|room| room.members.contains(&client) && role >= room.minimum_role)
    }
    pub fn publish(
        &mut self,
        client: u64,
        role: ServerRole,
        mut message: ChatMessage,
        now: u64,
    ) -> Result<(ChatMessage, Vec<u64>), String> {
        if !self.enabled {
            return Err("Chat is disabled on this server.".into());
        }
        if message.text.is_empty()
            || message.text.chars().count() > 160
            || message.text.len() > 640
            || message.text.chars().any(char::is_control)
        {
            return Err("Invalid chat message.".into());
        }
        let room = self.rooms.get_mut(&message.room).ok_or("Room not found.")?;
        if role < room.minimum_role || !room.members.contains(&client) {
            return Err("Join the room before posting.".into());
        }
        let (tokens, last) = self.limits.entry(client).or_insert((3.0, now));
        *tokens = (*tokens + now.saturating_sub(*last).min(30) as f32 * 0.5).min(3.0);
        *last = now;
        if *tokens < 1.0 {
            return Err("Wait before sending another message.".into());
        }
        *tokens -= 1.0;
        message.id = self.next_message;
        self.next_message = self
            .next_message
            .checked_add(1)
            .ok_or("Chat message limit reached.")?;
        if room.history.len() == 32 {
            room.history.pop_front();
        }
        room.history.push_back(message.clone());
        Ok((message, room.members.iter().copied().collect()))
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn message() -> ChatMessage {
        ChatMessage {
            id: 0,
            room: "lobby".into(),
            player: 1,
            name: "Pilot".into(),
            text: "Hello".into(),
            time: 1,
        }
    }
    #[test]
    fn official_quick_play_never_allows_chat_even_when_requested_in_config() {
        let mut chat = Chat::new(ServerKind::Official, true);
        assert!(!chat.enabled());
        assert!(chat.list(ServerRole::Owner).is_empty());
        assert!(chat.publish(1, ServerRole::Owner, message(), 1).is_err());
        assert!(
            chat.create("test".into(), "Test".into(), ServerRole::Player)
                .is_err()
        );
    }
    #[test]
    fn private_rooms_check_membership_and_chat_rate_is_bounded() {
        let mut chat = Chat::new(ServerKind::Community, true);
        chat.create("staff".into(), "Staff".into(), ServerRole::Moderator)
            .unwrap();
        assert!(chat.join(1, "staff", ServerRole::Player).is_err());
        chat.join(1, "staff", ServerRole::Moderator).unwrap();
        assert!(!chat.can_read(1, "staff", ServerRole::Player));
        chat.refresh_role(1, ServerRole::Player);
        assert!(chat.can_read(1, "lobby", ServerRole::Player));
        chat.join(1, "lobby", ServerRole::Player).unwrap();
        for _ in 0..3 {
            assert!(chat.publish(1, ServerRole::Player, message(), 1).is_ok());
        }
        assert!(chat.publish(1, ServerRole::Player, message(), 1).is_err());
        assert!(chat.publish(1, ServerRole::Player, message(), 3).is_ok());
        let history = chat.join(1, "lobby", ServerRole::Player).unwrap();
        assert!(history[0].id > 0);
        assert!(history.windows(2).all(|pair| pair[0].id < pair[1].id));
        assert!(chat.publish(2, ServerRole::Player, message(), 3).is_err());
        chat.remove(1);
        assert!(chat.publish(1, ServerRole::Player, message(), 4).is_err());
    }
}
