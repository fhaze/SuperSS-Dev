//! Channel and room domain types — the lobby state the Game Server manages.

use std::collections::HashMap;

/// A channel (lobby instance) the player can enter. Built from `[CHANNELn]`
/// config; runtime state tracks who's inside.
#[derive(Debug, Clone)]
pub struct Channel {
    pub id: u8,
    pub name: String,
    pub max_user: u32,
    pub max_level: u32,
    pub flag: u32,
    /// Connected player UIDs currently in this channel.
    pub players: Vec<i64>,
}

impl Channel {
    pub fn new(id: u8, name: String, max_user: u32, max_level: u32, flag: u32) -> Self {
        Self {
            id,
            name,
            max_user,
            max_level,
            flag,
            players: Vec::new(),
        }
    }

    pub fn curr_user(&self) -> u32 {
        self.players.len() as u32
    }

    pub fn is_full(&self) -> bool {
        self.curr_user() >= self.max_user
    }

    /// True if `level` is permitted to enter this channel.
    pub fn admits_level(&self, level: u16) -> bool {
        // Beginners channel (flag bit set) caps at max_level; open channels
        // accept any level up to max_level. The C++ checkEnterChannel logic.
        level <= self.max_level as u16
    }

    pub fn add_player(&mut self, uid: i64) {
        if !self.players.contains(&uid) {
            self.players.push(uid);
        }
    }

    pub fn remove_player(&mut self, uid: i64) {
        self.players.retain(|&u| u != uid);
    }
}

/// The Game Server's channel registry, indexed by channel id.
#[derive(Debug, Default)]
pub struct ChannelRegistry {
    pub channels: HashMap<u8, Channel>,
}

impl ChannelRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn insert(&mut self, channel: Channel) {
        self.channels.insert(channel.id, channel);
    }

    pub fn get(&self, id: u8) -> Option<&Channel> {
        self.channels.get(&id)
    }

    pub fn get_mut(&mut self, id: u8) -> Option<&mut Channel> {
        self.channels.get_mut(&id)
    }

    pub fn list(&self) -> Vec<&Channel> {
        let mut v: Vec<&Channel> = self.channels.values().collect();
        v.sort_by_key(|c| c.id);
        v
    }
}

/// A room (match instance) within a channel. Mirrors the lobby-visible fields
/// of `RoomInfo` (`pangya_game_st.h:2417`). Full match state arrives with
/// gameplay (Phase 8).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Room {
    pub id: u32,
    pub name: Vec<u8>,
    /// Room security key (17 bytes). Random printable chars generated on
    /// creation (mirrors room::geraSecurityKey). The client validates room
    /// operations against this key.
    pub key: [u8; 17],
    /// 1 = no password (open), 0 = password required. Mirrors `senha_flag`.
    pub senha_flag: u8,
    /// 1 = waiting (in lobby), 0 = playing. Mirrors `state`.
    pub state: u8,
    /// 1 = joinable after start. Mirrors `flag`.
    pub flag: u8,
    pub max_player: u8,
    pub num_player: u8,
    pub qntd_hole: u8,
    pub tipo_show: u8,
    pub numero: i16,
    pub modo: u8,
    pub course: u8,
    pub time_vs: u32,
    pub trofel: u32,
    pub state_flag: u16,
    pub rate_pang: u32,
    pub rate_exp: u32,
    pub flag_gm: u8,
    pub master: i32,
    pub tipo_ex: u8,
    pub artefato: u32,
    /// The leader/creator's UID. Not part of the wire struct (used for routing).
    pub leader_uid: i64,
    /// The member UIDs currently in the room (not part of the wire struct).
    pub players: Vec<i64>,
}

impl Room {
    /// Create a new room with sensible defaults (mirrors `RoomInfo::clear`).
    pub fn new(id: u32, name: Vec<u8>, leader_uid: i64) -> Self {
        Self {
            id,
            name,
            key: Self::generate_key(),
            senha_flag: 1, // open
            state: 1,      // waiting
            max_player: 4,
            num_player: 1,
            qntd_hole: 18,
            numero: id as i16,
            leader_uid,
            players: vec![leader_uid],
            ..Default::default()
        }
    }

    /// Generate a 17-byte room security key (16 random printable chars + NUL).
    /// Mirrors `room::geraSecurityKey` — chars in the range 60..254.
    pub fn generate_key_pub() -> [u8; 17] {
        Self::generate_key()
    }

    fn generate_key() -> [u8; 17] {
        use std::time::{SystemTime, UNIX_EPOCH};
        // Simple xorshift seeded from the wall clock — sufficient for a
        // non-cryptographic room key (the C++ uses MT19937).
        let seed = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(0)
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        let mut state = seed.max(1);
        let mut key = [0u8; 17];
        for k in &mut key[..16] {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            *k = ((state % 195) + 60) as u8;
        }
        key[16] = 0; // NUL terminator
        key
    }

    pub fn is_full(&self) -> bool {
        self.num_player >= self.max_player
    }

    pub fn add_player(&mut self, uid: i64) -> bool {
        if self.is_full() || self.players.contains(&uid) {
            return false;
        }
        self.players.push(uid);
        self.num_player = self.players.len() as u8;
        true
    }

    pub fn remove_player(&mut self, uid: i64) {
        self.players.retain(|&u| u != uid);
        self.num_player = self.players.len() as u8;
    }

    pub fn is_empty(&self) -> bool {
        self.players.is_empty()
    }
}

/// The Game Server's room registry, keyed by room id. Rooms are global (not
/// per-channel) in this simplified model; the C++ tracks them per-channel.
#[derive(Debug, Default)]
pub struct RoomRegistry {
    rooms: std::collections::HashMap<u32, Room>,
    next_id: u32,
}

impl RoomRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Create a room, returning its assigned id.
    pub fn create(&mut self, name: Vec<u8>, leader_uid: i64) -> u32 {
        self.next_id += 1;
        let id = self.next_id;
        self.rooms.insert(id, Room::new(id, name, leader_uid));
        id
    }

    pub fn get(&self, id: u32) -> Option<&Room> {
        self.rooms.get(&id)
    }

    pub fn get_mut(&mut self, id: u32) -> Option<&mut Room> {
        self.rooms.get_mut(&id)
    }

    pub fn list(&self) -> Vec<&Room> {
        let mut v: Vec<&Room> = self.rooms.values().collect();
        v.sort_by_key(|r| r.id);
        v
    }

    /// Add a player to a room. Returns `false` if full or not found.
    pub fn add_player(&mut self, room_id: u32, uid: i64) -> bool {
        match self.rooms.get_mut(&room_id) {
            Some(room) => room.add_player(uid),
            None => false,
        }
    }

    /// Remove a player from a room; deletes the room if it becomes empty.
    pub fn remove_player(&mut self, room_id: u32, uid: i64) {
        if let Some(room) = self.rooms.get_mut(&room_id) {
            room.remove_player(uid);
            if room.is_empty() {
                self.rooms.remove(&room_id);
            }
        }
    }

    pub fn len(&self) -> usize {
        self.rooms.len()
    }

    pub fn is_empty(&self) -> bool {
        self.rooms.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn channel_admit_and_capacity() {
        let mut ch = Channel::new(1, "Beginners".into(), 500, 16, 512);
        assert!(ch.admits_level(16));
        assert!(!ch.admits_level(17));

        ch.add_player(100);
        ch.add_player(100); // dedup
        assert_eq!(ch.curr_user(), 1);
        ch.remove_player(100);
        assert_eq!(ch.curr_user(), 0);
    }

    #[test]
    fn channel_registry_lists_in_id_order() {
        let mut reg = ChannelRegistry::new();
        reg.insert(Channel::new(2, "B".into(), 100, 70, 0));
        reg.insert(Channel::new(1, "A".into(), 100, 70, 0));
        let list = reg.list();
        assert_eq!(list.iter().map(|c| c.id).collect::<Vec<_>>(), vec![1, 2]);
    }

    #[test]
    fn channel_full_check() {
        let mut ch = Channel::new(1, "X".into(), 2, 70, 0);
        ch.add_player(1);
        assert!(!ch.is_full());
        ch.add_player(2);
        assert!(ch.is_full());
    }

    #[test]
    fn room_create_add_remove() {
        let mut room = Room::new(1, "Test Room".as_bytes().to_vec(), 100);
        assert_eq!(room.num_player, 1);
        assert_eq!(room.senha_flag, 1); // open
        assert_eq!(room.state, 1); // waiting

        assert!(room.add_player(101));
        assert_eq!(room.num_player, 2);
        // Can't re-add.
        assert!(!room.add_player(101));

        room.remove_player(100);
        assert_eq!(room.num_player, 1);
        assert!(!room.is_empty());
        room.remove_player(101);
        assert!(room.is_empty());
    }

    #[test]
    fn room_capacity_enforced() {
        let mut room = Room::new(1, "Small".as_bytes().to_vec(), 1);
        room.max_player = 2;
        assert!(room.add_player(2));
        assert!(!room.add_player(3)); // full
        assert_eq!(room.num_player, 2);
    }

    #[test]
    fn room_registry_creates_and_lists() {
        let mut reg = RoomRegistry::new();
        let id1 = reg.create("Room A".as_bytes().to_vec(), 100);
        let id2 = reg.create("Room B".as_bytes().to_vec(), 200);
        assert_eq!(id2, id1 + 1);

        let list = reg.list();
        assert_eq!(list.len(), 2);

        // Leaving a room removes it when empty.
        reg.remove_player(id1, 100);
        assert_eq!(reg.list().len(), 1);
    }
}
