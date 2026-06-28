//! The two server roles and their async lifecycle.
//!
//! These traits mirror the virtual hooks of the C++ `unit` and `server` base
//! classes (`onAcceptCompleted`, `onDisconnected`, `onHeartBeat`, `onStart`,
//! `checkPacket`, `shutdown_time`). Each server binary implements one of them.
//!
//! The accept loop, heartbeat task, and shutdown wiring are provided by the
//! framework (built out in Phase 3); for now the traits define the contract.

use crate::session::SessionMap;
use std::sync::Arc;

/// A **listener-only** server: accepts connections but does not dial out.
///
/// The C++ `unit` class — used by the Auth Server and GG Auth Server.
#[allow(async_fn_in_trait)] // fire-and-forget lifecycle hooks; no Send bound needed on the trait
pub trait Unit: Sized {
    /// Called once after the listener is bound, before the accept loop runs.
    /// Mirrors `onStart`.
    async fn on_start(&self) -> anyhow::Result<()>;

    /// Called for each accepted connection after it is registered.
    /// Mirrors `onAcceptCompleted`.
    async fn on_accept(&self, session_id: u64) -> anyhow::Result<()>;

    /// Called when a connection drops. Mirrors `onDisconnected`.
    async fn on_disconnect(&self, session_id: u64);

    /// Called once per second from the heartbeat task. Mirrors `onHeartBeat`.
    async fn on_heartbeat(&self);

    /// Access the live-session registry.
    fn sessions(&self) -> &SessionMap;
}

/// A **listener + auth-client** server: accepts clients and additionally dials
/// the Auth Server hub.
///
/// The C++ `server` class — used by Login, Game, Rank, Message. The outbound
/// auth link (the C++ `unit_auth_server_connect`) is owned by the framework in
/// Phase 3; this trait adds the hooks the link calls back into.
#[allow(async_fn_in_trait)]
pub trait Server: Unit {
    /// Called by the auth link once the hub has confirmed this server's
    /// registration (the `0x0`/`0x1` first-packet-key handshake). Mirrors the
    /// `IUnitAuthServer::requestAskLogin` callback path.
    async fn on_auth_registered(&self) -> anyhow::Result<()>;

    /// A cross-server command relayed through the Auth Server (the generic
    /// `0xD`/`0xE` path). The payload is the inner command bytes.
    async fn on_relayed_command(&self, source_uid: u32, command: &[u8]) -> anyhow::Result<()>;
}

/// Shared runtime handles every server holds: the tokio runtime handle and the
/// shutdown signal. Built out in Phase 3; declared here so binaries can store it.
#[derive(Clone)]
pub struct RuntimeHandles {
    pub shutdown: Arc<tokio::sync::Notify>,
}
