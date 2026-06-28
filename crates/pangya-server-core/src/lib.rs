//! Shared server framework: connection lifecycle, heartbeat, and dispatch.
//!
//! Models the two base roles from the C++ core:
//!
//! - [`Server`] — listens for clients **and** dials the Auth Server hub. Used by
//!   Login, Game, Rank, Message. (The C++ `server` class.)
//! - [`Unit`] — listens only, no outbound link. Used by Auth and GG Auth
//!   servers. (The C++ `unit` class.)
//!
//! Both roles share the same async lifecycle: an accept loop, a 1-second
//! heartbeat task, and per-connection packet handling driven by a generic
//! dispatcher. There is **no global singleton** — every handle is passed
//! explicitly, replacing the C++ `ssv::sv` / `NormalManagerDB` globals.

pub mod dispatch;
pub mod game_login;
pub mod gm;
pub mod login;
pub mod packet_log;
pub mod runtime;
pub mod server;
pub mod session;

pub use dispatch::{Dispatch, HandlerResult};
pub use runtime::{bind, log_conn_error, run_heartbeat, FramedConn, Runtime};
pub use server::{Server as ServerRole, Unit};
pub use session::SessionMap;

/// A per-connection role identifier, used to pick the wire format and session
/// key when accepting a connection.
#[derive(Debug, Clone, Copy)]
pub struct ConnRole {
    /// The session key (high nibble 0..=15) assigned to this connection, like
    /// the C++ `rand() % 16` on accept.
    pub session_key: u8,
    /// Whether this side is the server (3-byte header) or emulating a client
    /// (4-byte header). For accepted client connections this is the server
    /// format; for outbound auth-server links it's the client format.
    pub is_server_format: bool,
}
