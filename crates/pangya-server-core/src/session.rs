//! Live-connection registry.
//!
//! Replaces the C++ `session_manager` / `player_manager` (a locked container of
//! active sessions keyed by socket). Here a [`SessionMap`] holds lightweight
//! per-connection handles behind a `DashMap`, keyed by a monotonic connection
//! id. The actual socket I/O lives in each server binary's accept loop.

use dashmap::DashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

/// Monotonic connection identifier assigned on accept.
pub type ConnId = u64;

/// A live connection handle stored in the registry. The concrete per-server
/// session type (carrying the player state, auth key, etc.) is held by the
/// server binary; this keeps only the bookkeeping the framework needs.
#[derive(Debug)]
pub struct Session {
    pub id: ConnId,
    pub session_key: u8,
    /// Peer address, for logging and the IP-ban check.
    pub peer_addr: String,
    /// True once the connection has cleared its first/handshake packet.
    pub authorized: std::sync::atomic::AtomicBool,
}

impl Session {
    pub fn new(id: ConnId, session_key: u8, peer_addr: String) -> Arc<Self> {
        Arc::new(Self {
            id,
            session_key,
            peer_addr,
            authorized: std::sync::atomic::AtomicBool::new(false),
        })
    }

    pub fn mark_authorized(&self) {
        self.authorized.store(true, Ordering::Release);
    }

    pub fn is_authorized(&self) -> bool {
        self.authorized.load(Ordering::Acquire)
    }
}

/// A thread-safe registry of live sessions.
#[derive(Default)]
pub struct SessionMap {
    next_id: AtomicU64,
    sessions: DashMap<ConnId, Arc<Session>>,
}

impl SessionMap {
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a new connection, returning its id and the stored handle.
    pub fn insert(&self, session_key: u8, peer_addr: String) -> (ConnId, Arc<Session>) {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let session = Session::new(id, session_key, peer_addr);
        self.sessions.insert(id, session.clone());
        (id, session)
    }

    pub fn get(&self, id: ConnId) -> Option<Arc<Session>> {
        self.sessions.get(&id).map(|r| r.clone())
    }

    pub fn remove(&self, id: ConnId) {
        self.sessions.remove(&id);
    }

    pub fn len(&self) -> usize {
        self.sessions.len()
    }

    pub fn is_empty(&self) -> bool {
        self.sessions.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn insert_get_remove() {
        let map = SessionMap::new();
        let (id, s) = map.insert(7, "1.2.3.4:1234".into());
        assert_eq!(s.session_key, 7);
        assert!(map.get(id).is_some());
        assert_eq!(map.len(), 1);

        map.remove(id);
        assert!(map.get(id).is_none());
        assert!(map.is_empty());
    }

    #[test]
    fn ids_are_monotonic() {
        let map = SessionMap::new();
        let (a, _) = map.insert(0, "x".into());
        let (b, _) = map.insert(0, "y".into());
        assert_eq!(b, a + 1);
    }

    #[test]
    fn authorization_flag_round_trips() {
        let s = Session::new(0, 1, "p".into());
        assert!(!s.is_authorized());
        s.mark_authorized();
        assert!(s.is_authorized());
    }
}
