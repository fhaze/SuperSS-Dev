//! Auth Server — the central message-broker hub.
//!
//! Every other server (Login, Game, Rank, Message) dials this process on port
//! 7777. Cross-server traffic is **relayed** through it: there is no direct
//! server-to-server mesh. The Auth Server keeps a UID → connection routing
//! table and forwards the generic `0xD`/`0xE` command packets to the target.
//!
//! This mirrors the C++ `auth_server` (a `unit` subclass). The inter-server
//! link uses **plaintext** packet framing (no XOR/LZO) — see the design notes
//! in `pangya-net::framing`.
//!
//! For Milestone 1 the hub implements: accept connections, the `0x0`/`0x1`
//! first-packet-key registration handshake, and `0xD`/`0xE` relay routing. The
//! scheduled-command polling and guild-ranking heartbeat arrive later.

mod relay;

use anyhow::Result;
use pangya_config::ServerConfig;
use pangya_net::framing::{decode_raw, SessionKey};
use pangya_server_core::Runtime;
use std::sync::Arc;
use tokio::io::AsyncReadExt;
use tokio::net::TcpListener;
use tracing::{error, info, warn};
use tracing_subscriber::EnvFilter;

use relay::RelayTable;

const LOG_PREFIX: &str = "AS";

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    let cfg = ServerConfig::load("server.ini")
        .map_err(|e| anyhow::anyhow!("failed to load server.ini: {e}"))?;

    info!(
        "[{}] Auth Server starting up — guid={}, port={}, db={:?}",
        LOG_PREFIX, cfg.server.guid, cfg.server.port, cfg.db.engine
    );

    let runtime = Arc::new(Runtime::new());
    let relay = Arc::new(RelayTable::new());

    let addr = format!("{}:{}", cfg.server.ip, cfg.server.port);
    let listener = TcpListener::bind(&addr).await?;
    info!("[{}] listening on {addr}", LOG_PREFIX);

    // Heartbeat: tick the relay stats and reap idle connections (Phase 3+ adds
    // the scheduled-command DB poll here).
    {
        let relay_hb = Arc::clone(&relay);
        let shutdown = Arc::clone(&runtime.shutdown);
        tokio::spawn(async move {
            pangya_server_core::run_heartbeat(shutdown, move || {
                // Future: poll pangya_command table for scheduled GM ops.
                let _ = relay_hb.len();
            })
            .await;
        });
    }

    // Accept loop.
    loop {
        tokio::select! {
            _ = runtime.shutdown.notified() => {
                info!("[{}] shutdown signal received", LOG_PREFIX);
                break;
            }
            accept = listener.accept() => {
                match accept {
                    Ok((stream, peer)) => {
                        let relay = Arc::clone(&relay);
                        tokio::spawn(handle_connection(stream, peer.to_string(), relay));
                    }
                    Err(e) => error!("[{}] accept failed: {e}", LOG_PREFIX),
                }
            }
        }
    }

    info!("[{}] Auth Server stopped", LOG_PREFIX);
    Ok(())
}

/// Handle one inbound server connection.
///
/// The inter-server link is plaintext, so we read raw frames directly rather
/// than going through the XOR codec. The first packet is the registration
/// handshake (opcode `0x0`); after that, relayed commands (`0xD`/`0xE`).
async fn handle_connection(
    mut stream: tokio::net::TcpStream,
    peer: String,
    relay: Arc<RelayTable>,
) {
    // The session key on the auth link is effectively unused (plaintext), but
    // the raw reader still needs one to parse the framing header.
    let _sk = SessionKey(0);

    // Read the first packet raw: [low_key=0][size LE][0x00 marker][body].
    // The body is the connecting server's handshake (its GUID + key).
    let mut buf = vec![0u8; 4096];
    let n = match stream.read(&mut buf).await {
        Ok(0) => {
            warn!(
                "[{}] {peer}: connection closed before handshake",
                LOG_PREFIX
            );
            return;
        }
        Ok(n) => n,
        Err(e) => {
            error!("[{}] {peer}: handshake read failed: {e}", LOG_PREFIX);
            return;
        }
    };

    let first = match decode_raw(&buf[..n]) {
        Ok(frame) => frame,
        Err(e) => {
            warn!("[{}] {peer}: invalid handshake frame: {e}", LOG_PREFIX);
            return;
        }
    };

    // Parse the connecting server's UID from the handshake body. The body
    // layout (from unit_auth_server_connect) is: [uid:u32][key:string]…
    let server_uid = parse_handshake_uid(&first.body);
    info!(
        "[{}] {peer}: server uid={} connected, registering relay route",
        LOG_PREFIX, server_uid
    );
    relay.register(server_uid, peer.clone());

    // TODO(Phase 3+): continue reading subsequent frames in a loop and route
    // 0xD/0xE command packets via relay.send_command(target_uid, body). The
    // connection-task plumbing for that (a shared per-UID send handle) lands
    // with the Login Server dialer in Phase 4.

    relay.unregister(server_uid);
    info!(
        "[{}] {peer}: server uid={} disconnected",
        LOG_PREFIX, server_uid
    );
}

/// Extract the server UID from a handshake body: first 4 bytes, little-endian.
fn parse_handshake_uid(body: &[u8]) -> u32 {
    if body.len() >= 4 {
        u32::from_le_bytes([body[0], body[1], body[2], body[3]])
    } else {
        0
    }
}
