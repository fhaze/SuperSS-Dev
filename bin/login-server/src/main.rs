//! Login Server — the first client touchpoint.
//!
//! Implements the login flow end-to-end in Rust:
//!
//! 1. Accept a client TCP connection and assign it a random session key (0..15).
//! 2. Send the raw key-exchange first packet.
//! 3. Receive client packets through the XOR codec, dispatch by opcode.
//! 4. For the `0x01` Login opcode: verify credentials → mint an auth key →
//!    send back the login-success + server list. (See
//!    [`pangya_server_core::login::handle_login`].)
//!
//! All other opcodes (select-server, nickname, create-character) are dispatched
//! too; their handlers are added as Milestone-1 systems expand.

use anyhow::Result;
use bytes::BytesMut;
use pangya_config::ServerConfig;
use pangya_net::codec::{Format, PangyaDecoder};
use pangya_net::framing::{self, SessionKey};
use pangya_proto::{split_opcode, LoginPacket};
use pangya_server_core::login::{handle_login, LoginConfig, LoginOutcome};
use pangya_server_core::Runtime;
use rand::Rng;
use std::sync::Arc;
use tokio::net::TcpListener;
use tokio_util::codec::Decoder;
use tracing::{error, info, warn};
use tracing_subscriber::EnvFilter;

const LOG_PREFIX: &str = "LS";

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
        "[{}] Login Server starting up — guid={}, port={}, same_id_login={}, create_user={}",
        LOG_PREFIX,
        cfg.server.guid,
        cfg.server.port,
        cfg.options.same_id_login,
        cfg.options.create_user
    );

    // Connect to the database.
    let pool = pangya_db::connect(&cfg.db.mysql_url())
        .await
        .map_err(|e| anyhow::anyhow!("database connection failed: {e}"))?;
    info!("[{}] connected to database", LOG_PREFIX);

    let login_cfg = LoginConfig {
        create_user: cfg.options.create_user,
        access_flag: cfg.options.access_flag,
        same_id_login: cfg.options.same_id_login,
    };
    let server_uid = cfg.server.guid;

    let runtime = Arc::new(Runtime::new());

    let addr = format!("{}:{}", cfg.server.ip, cfg.server.port);
    let listener = TcpListener::bind(&addr).await?;
    info!("[{}] listening on {addr}", LOG_PREFIX);

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
                        let pool = Arc::clone(&Arc::new(pool.clone()));
                        let cfg = login_cfg;
                        tokio::spawn(handle_client(
                            stream,
                            peer.to_string(),
                            pool,
                            cfg,
                            server_uid,
                        ));
                    }
                    Err(e) => error!("[{}] accept failed: {e}", LOG_PREFIX),
                }
            }
        }
    }

    info!("[{}] Login Server stopped", LOG_PREFIX);
    Ok(())
}

/// Handle one client connection.
async fn handle_client(
    stream: tokio::net::TcpStream,
    peer: String,
    pool: Arc<pangya_db::DbPool>,
    login_cfg: LoginConfig,
    server_uid: u32,
) {
    // Assign a random 0..=15 session key, like the C++ `rand() % 16` on accept.
    let session_key: u8 = rand::thread_rng().gen_range(0..=15);
    let sk = SessionKey(session_key);

    let (read_half, mut write_half) = stream.into_split();

    // Send the raw key-exchange greeting. Mirrors the C++ login_server
    // onAcceptCompleted (login_server.cpp:1045): opcode 0x00 + i32 key +
    // i32 server UID, sent via makeRaw() (no compress/crypt).
    let mut greeting_body = Vec::with_capacity(10);
    greeting_body.extend_from_slice(&0u16.to_le_bytes()); // opcode 0x00
    greeting_body.extend_from_slice(&(session_key as i32).to_le_bytes()); // key
    greeting_body.extend_from_slice(&(server_uid as i32).to_le_bytes()); // server UID
    let mut frame = Vec::new();
    if let Err(e) = framing::encode_raw(&greeting_body, &mut frame) {
        warn!("[{}] {peer}: failed to encode handshake: {e}", LOG_PREFIX);
        return;
    }
    pangya_server_core::packet_log::log_packet(
        pangya_server_core::packet_log::Dir::S2C,
        "LS",
        &greeting_body,
    );
    use tokio::io::AsyncWriteExt;
    if let Err(e) = write_half.write_all(&frame).await {
        warn!("[{}] {peer}: handshake send failed: {e}", LOG_PREFIX);
        return;
    }

    // Decode subsequent client packets with the XOR codec.
    let mut decoder = PangyaDecoder::new(Format::Client, sk);
    let mut buf = BytesMut::with_capacity(4096);
    use tokio::io::AsyncReadExt;
    let mut reader = read_half;
    let mut logged_uid: Option<i64> = None;

    loop {
        let mut tmp = [0u8; 4096];
        match reader.read(&mut tmp).await {
            Ok(0) => {
                info!("[{}] {peer}: client disconnected", LOG_PREFIX);
                break;
            }
            Ok(n) => buf.extend_from_slice(&tmp[..n]),
            Err(e) => {
                warn!("[{}] {peer}: read error: {e}", LOG_PREFIX);
                break;
            }
        }

        // Peel as many complete frames as the buffer holds.
        loop {
            match decoder.decode(&mut buf) {
                Ok(Some(frame_data)) => {
                    pangya_server_core::packet_log::log_packet(
                        pangya_server_core::packet_log::Dir::C2S,
                        "LS",
                        &frame_data.body,
                    );
                    if let Err(e) = dispatch_frame(
                        &frame_data.body,
                        &pool,
                        login_cfg,
                        &mut write_half,
                        sk,
                        &mut logged_uid,
                    )
                    .await
                    {
                        error!("[{}] {peer}: dispatch error: {e}", LOG_PREFIX);
                        return;
                    }
                }
                Ok(None) => break, // need more bytes
                Err(e) => {
                    warn!("[{}] {peer}: decode error: {e}", LOG_PREFIX);
                    return;
                }
            }
        }
    }
}

/// Dispatch one decoded frame body.
async fn dispatch_frame<W>(
    body: &[u8],
    pool: &pangya_db::DbPool,
    login_cfg: LoginConfig,
    writer: &mut W,
    sk: SessionKey,
    logged_uid: &mut Option<i64>,
) -> anyhow::Result<()>
where
    W: tokio::io::AsyncWriteExt + Unpin,
{
    let (opcode, payload) =
        split_opcode(body).ok_or_else(|| anyhow::anyhow!("empty packet body"))?;

    match opcode {
        0x01 | 0x0B => {
            // Login / Re-login
            let packet = LoginPacket::parse(body)?;
            if let pangya_proto::LoginPacket::Login(req) = packet {
                let outcome = handle_login(pool, &req, login_cfg).await?;
                // Capture the UID for subsequent opcodes (server-select).
                if let LoginOutcome::Success { .. } = &outcome {
                    // The handle_login already verified credentials; extract uid
                    // from the account lookup we can infer from the request.
                    if let Some(acc) = pangya_db::repos::account_by_id(pool, &req.id).await? {
                        *logged_uid = Some(acc.uid);
                    }
                }
                send_outcome(outcome, writer, sk).await?;
            }
        }
        0x03 => {
            // Select Server — client picked a game server. Mint a game auth key
            // and return it so the client can connect to the Game Server.
            if let Some(uid) = *logged_uid {
                // Read the requested server UID (u32 LE).
                let mut reader = pangya_proto::PayloadReader::new(payload);
                let _server_uid = reader.read_u32("select_server.uid").unwrap_or(0);

                let auth_key = pangya_model::gen_auth_key(&mut rand::thread_rng());
                pangya_db::repos::mint_game_auth_key(pool, uid, &auth_key).await?;
                info!("[{}] minted game auth key for uid={uid}", LOG_PREFIX);

                let body = pangya_proto::login_resp::build_select_server_response(&auth_key, 0);
                pangya_server_core::packet_log::log_packet(
                    pangya_server_core::packet_log::Dir::S2C,
                    "LS",
                    &body,
                );
                let mut frame = Vec::with_capacity(body.len() + 16);
                let low_key: u8 = rand::Rng::gen_range(&mut rand::thread_rng(), 1..=255);
                framing::encode_server(&body, sk, low_key, &mut frame)?;
                writer.write_all(&frame).await?;
            } else {
                warn!("[{}] select server before login", LOG_PREFIX);
            }
        }
        _ => {
            info!("[{}] unhandled opcode {opcode:#06x}", LOG_PREFIX);
        }
    }
    Ok(())
}

/// Encode and send a [`LoginOutcome`] back to the client.
async fn send_outcome<W>(
    outcome: LoginOutcome,
    writer: &mut W,
    sk: SessionKey,
) -> anyhow::Result<()>
where
    W: tokio::io::AsyncWriteExt + Unpin,
{
    let bodies = match outcome {
        LoginOutcome::Success { bodies } => bodies,
        LoginOutcome::Denied { body, .. } => vec![body],
    };

    for body in bodies {
        pangya_server_core::packet_log::log_packet(
            pangya_server_core::packet_log::Dir::S2C,
            "LS",
            &body,
        );
        let mut frame = Vec::with_capacity(body.len() + 16);
        let low_key: u8 = rand::thread_rng().gen_range(1..=255);
        framing::encode_server(&body, sk, low_key, &mut frame)?;
        writer.write_all(&frame).await?;
    }
    Ok(())
}
