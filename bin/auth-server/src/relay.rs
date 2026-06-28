//! The Auth Server's UID → connection routing table.
//!
//! Mirrors the C++ `auth_server` connection registry that the
//! `FIND_TARGET_AND_SEND` macro consults. For Milestone 1 it tracks which
//! server UIDs are connected; the actual send-channel wiring is added when
//! the Login Server dialer (Phase 4) establishes bidirectional relay.
#![allow(dead_code)] // forward-facing relay API consumed by Phase 4+ relay wiring

use dashmap::DashMap;
use std::sync::Arc;

/// One connected server route.
#[derive(Debug, Clone)]
pub struct Route {
    pub server_uid: u32,
    pub peer: String,
}

/// Thread-safe registry of connected servers, keyed by server UID.
#[derive(Default)]
pub struct RelayTable {
    routes: DashMap<u32, Route>,
}

impl RelayTable {
    pub fn new() -> Self {
        Self::default()
    }

    /// Register (or replace) a server route.
    pub fn register(&self, server_uid: u32, peer: String) {
        self.routes.insert(server_uid, Route { server_uid, peer });
    }

    /// Remove a server route.
    pub fn unregister(&self, server_uid: u32) {
        self.routes.remove(&server_uid);
    }

    /// Look up a route by UID (for relay targeting).
    pub fn get(&self, server_uid: u32) -> Option<Route> {
        self.routes.get(&server_uid).map(|r| r.clone())
    }

    /// Number of connected servers.
    pub fn len(&self) -> usize {
        self.routes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.routes.is_empty()
    }

    /// All currently-registered server UIDs (for "broadcast to all" relays).
    pub fn server_uids(&self) -> Vec<u32> {
        self.routes.iter().map(|r| *r.key()).collect()
    }
}

pub type SharedRelayTable = Arc<RelayTable>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn register_lookup_unregister() {
        let table = RelayTable::new();
        table.register(20203, "1.2.3.4:5555".into());
        table.register(10103, "5.6.7.8:6666".into());

        assert_eq!(table.len(), 2);
        assert_eq!(table.get(20203).unwrap().peer, "1.2.3.4:5555");
        assert!(table.get(99999).is_none());

        let mut uids = table.server_uids();
        uids.sort();
        assert_eq!(uids, vec![10103, 20203]);

        table.unregister(20203);
        assert_eq!(table.len(), 1);
        assert!(table.get(20203).is_none());
    }

    #[test]
    fn register_replaces_existing() {
        let table = RelayTable::new();
        table.register(1, "old".into());
        table.register(1, "new".into());
        assert_eq!(table.len(), 1);
        assert_eq!(table.get(1).unwrap().peer, "new");
    }
}
