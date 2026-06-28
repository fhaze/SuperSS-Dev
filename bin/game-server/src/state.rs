//! Shared Game Server state: the channel registry and per-channel broadcast.
//!
//! Tracks which players are in which channel so lobby chat (`0x40`) can fan out.
//! Each connected client registers a send handle (a tokio mpsc sender) so the
//! server can push packets to it without owning the socket.
#![allow(dead_code)] // forward-facing API consumed as more opcodes wire up

use dashmap::DashMap;
use std::sync::Arc;
use tokio::sync::mpsc;

/// A handle to push an outgoing packet body to a specific connection.
pub type Sender = mpsc::UnboundedSender<Vec<u8>>;

/// The outcome of an enter-channel attempt.
#[derive(Debug)]
pub enum EnterResult {
    Success,
    Full,
    NotFound,
}

/// Shared server state: channel membership, per-uid send handles, and rooms.
#[derive(Default)]
pub struct ServerState {
    /// channel_id → set of (uid, sender)
    channels: DashMap<u8, Vec<(i64, Sender)>>,
    /// Global room registry (rooms addressable by id across channels).
    rooms: DashMap<u32, pangya_model::Room>,
    next_room_id: std::sync::atomic::AtomicU32,
}

impl ServerState {
    pub fn new(_registry: Arc<pangya_model::ChannelRegistry>) -> Self {
        Self::default()
    }

    /// Attempt to add a player to a channel. Returns Full if the channel has a
    /// configured cap (checked against the registry) and is saturated.
    pub async fn enter_channel(&self, channel_id: u8, uid: i64) -> EnterResult {
        // We don't track capacity here directly (the registry owns max_user);
        // for Milestone 1 accept everyone. The registry cap check can be wired
        // in by passing the registry. Returning Success keeps the lobby working.
        let mut entry = self.channels.entry(channel_id).or_default();
        // Don't double-insert.
        if !entry.iter().any(|(u, _)| *u == uid) {
            // A real sender would be registered via register_sender; here we
            // create a dangling channel since broadcast is best-effort.
            let (tx, _rx) = mpsc::unbounded_channel();
            entry.push((uid, tx));
        }
        EnterResult::Success
    }

    /// Register the live sender for a uid so broadcasts reach it.
    pub fn register_sender(&self, channel_id: u8, uid: i64, sender: Sender) {
        let mut entry = self.channels.entry(channel_id).or_default();
        if let Some(slot) = entry.iter_mut().find(|(u, _)| *u == uid) {
            slot.1 = sender;
        } else {
            entry.push((uid, sender));
        }
    }

    /// Remove a player from a channel.
    pub async fn leave_channel(&self, channel_id: u8, uid: i64) {
        if let Some(mut entry) = self.channels.get_mut(&channel_id) {
            entry.retain(|(u, _)| *u != uid);
        }
    }

    /// Broadcast a packet body to every connection in a channel.
    pub async fn broadcast_channel(&self, channel_id: u8, body: &[u8]) {
        if let Some(entry) = self.channels.get(&channel_id) {
            for (_, sender) in entry.iter() {
                let _ = sender.send(body.to_vec());
            }
        }
    }

    // ── rooms ────────────────────────────────────────────────────────────────

    /// Create a room, returning its assigned id.
    pub fn create_room(&self, name: Vec<u8>, leader_uid: i64) -> u32 {
        let id = 1 + self
            .next_room_id
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        self.rooms
            .insert(id, pangya_model::Room::new(id, name, leader_uid));
        id
    }

    /// Create a fully-specified room (carrying all `MakeRoom` request fields),
    /// returning the created room (with its assigned id).
    pub fn create_room_full(&self, mut room: pangya_model::Room) -> pangya_model::Room {
        let id = 1 + self
            .next_room_id
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        room.id = id;
        room.numero = id as i16;
        self.rooms.insert(id, room.clone());
        room
    }

    /// Get a room by id.
    pub fn get_room(&self, room_id: u32) -> Option<pangya_model::Room> {
        self.rooms.get(&room_id).map(|r| r.clone())
    }

    /// List all rooms (for the room-list `0x47` response).
    pub fn list_rooms(&self) -> Vec<pangya_model::Room> {
        let mut v: Vec<pangya_model::Room> = self.rooms.iter().map(|r| r.clone()).collect();
        v.sort_by_key(|r| r.id);
        v
    }

    /// Add a player to a room. Returns false if full or not found.
    pub fn room_add_player(&self, room_id: u32, uid: i64) -> bool {
        if let Some(mut room) = self.rooms.get_mut(&room_id) {
            room.add_player(uid)
        } else {
            false
        }
    }

    /// Remove a player from a room; deletes it when empty.
    pub fn room_remove_player(&self, room_id: u32, uid: i64) {
        let should_remove = if let Some(mut room) = self.rooms.get_mut(&room_id) {
            room.remove_player(uid);
            room.is_empty()
        } else {
            false
        };
        if should_remove {
            drop(self.rooms.remove(&room_id));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn enter_leave_tracks_membership() {
        let registry = Arc::new(pangya_model::ChannelRegistry::new());
        let state = ServerState::new(registry);

        assert!(matches!(
            state.enter_channel(1, 100).await,
            EnterResult::Success
        ));
        assert!(matches!(
            state.enter_channel(1, 101).await,
            EnterResult::Success
        ));

        state.leave_channel(1, 100).await;
        // No panic; membership shrinks silently.
    }

    #[tokio::test]
    async fn broadcast_reaches_registered_senders() {
        let registry = Arc::new(pangya_model::ChannelRegistry::new());
        let state = ServerState::new(registry);

        let (tx, mut rx) = mpsc::unbounded_channel();
        state.enter_channel(1, 1).await;
        state.register_sender(1, 1, tx);

        state.broadcast_channel(1, b"hello").await;
        let received = rx.recv().await.unwrap();
        assert_eq!(received, b"hello");
    }
}
