//! Server runtime: the accept loop and heartbeat task that drive a server
//! role.
//!
//! This is the concrete machinery behind the [`crate::Unit`] / [`crate::Server`]
//! traits — what the C++ implemented with IOCP/epoll worker pools, a `monitor`
//! thread, and `commandScan`. Here it is one `tokio` task per connection plus a
//! single heartbeat interval, with no platform `#ifdef` soup.

use crate::session::SessionMap;
use std::sync::Arc;
use tokio::net::TcpListener;
use tokio_util::codec::Framed;
use tracing::{error, info, warn};

/// A server runtime holds the shared session registry and a shutdown signal.
/// Each role (Login/Game/…) owns one and feeds it to [`run_accept_loop`].
#[derive(Clone)]
pub struct Runtime {
    pub sessions: Arc<SessionMap>,
    pub shutdown: Arc<tokio::sync::Notify>,
}

impl Default for Runtime {
    fn default() -> Self {
        Self {
            sessions: Arc::new(SessionMap::new()),
            shutdown: Arc::new(tokio::sync::Notify::new()),
        }
    }
}

impl Runtime {
    pub fn new() -> Self {
        Self::default()
    }

    /// Signal every task to shut down.
    pub fn shutdown(&self) {
        self.shutdown.notify_waiters();
    }
}

/// Run a 1-second heartbeat loop until shutdown is signalled.
///
/// `tick` is called once per second — the `onHeartBeat` hook. Spawn this as a
/// tokio task alongside [`run_accept_loop`].
pub async fn run_heartbeat<F>(shutdown: Arc<tokio::sync::Notify>, mut tick: F)
where
    F: FnMut() + Send + 'static,
{
    let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(1));
    loop {
        tokio::select! {
            _ = shutdown.notified() => {
                info!("heartbeat task shutting down");
                break;
            }
            _ = interval.tick() => {
                tick();
            }
        }
    }
}

/// Bind a TCP listener on `addr` and log it. Shared by every server role.
pub async fn bind(addr: &str) -> anyhow::Result<TcpListener> {
    let listener = TcpListener::bind(addr).await?;
    info!("listening on {addr}");
    Ok(listener)
}

/// A framed connection: the decoded-frame stream plus the session handle.
///
/// Concrete servers consume this in their per-connection task. The decoder is
/// chosen from the connection's [`crate::ConnRole`].
pub type FramedConn = Framed<tokio::net::TcpStream, pangya_net::codec::PangyaDecoder>;

/// Helper to log a per-connection error uniformly before it disconnects.
pub fn log_conn_error(peer: &str, err: impl std::fmt::Display) {
    // Truncation on EOF is expected (client dropped mid-frame); log at debug,
    // everything else is a real decode failure.
    if matches!(
        err.to_string().as_str(),
        s if s.contains("Truncated")
    ) {
        warn!(peer, "connection closed mid-frame: {err}");
    } else {
        error!(peer, "connection error: {err}");
    }
}
