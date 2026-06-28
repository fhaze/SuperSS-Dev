//! Game Server — the main gameplay process.
//!
//! Implements the Milestone 1 lobby flow in Rust:
//!
//! 1. On accept, send the raw `0x3F` greeting with a per-connection session key.
//! 2. Receive `0x02` login → verify the auth key → send `0x44` ack + channel list.
//! 3. Receive `0x04` enter-channel → send `0x4E` result.
//! 4. Receive `0x03` chat → broadcast `0x40` to the channel.

mod state;

use anyhow::Result;
use bytes::BytesMut;
use pangya_config::ServerConfig;
use pangya_db::DbPool;
use pangya_model::ChannelRegistry;
use pangya_net::codec::{Format, PangyaDecoder};
use pangya_net::framing::{self, SessionKey};
use pangya_proto::{game_resp, split_opcode, ChatRequest, EnterChannelRequest, GamePacket};
use pangya_server_core::game_login::{handle_game_login, GameLoginOutcome};
use pangya_server_core::Runtime;
use rand::Rng;
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio_util::codec::Decoder;
use tracing::{error, info, warn};
use tracing_subscriber::EnvFilter;

use state::ServerState;

const LOG_PREFIX: &str = "GS";

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .init();

    let cfg = ServerConfig::load("server.ini")
        .map_err(|e| anyhow::anyhow!("failed to load server.ini: {e}"))?;

    let n_channels = cfg.channels.as_ref().map(|c| c.num_channels).unwrap_or(0);
    info!(
        "[{}] Game Server starting up — guid={}, port={}, channels={}, game_guard_auth={}",
        LOG_PREFIX, cfg.server.guid, cfg.server.port, n_channels, cfg.game_guard_auth
    );

    let pool = Arc::new(
        pangya_db::connect(&cfg.db.mysql_url())
            .await
            .map_err(|e| anyhow::anyhow!("database connection failed: {e}"))?,
    );
    info!("[{}] connected to database", LOG_PREFIX);

    // Build the channel registry from config.
    let mut registry = ChannelRegistry::new();
    if let Some(channels) = &cfg.channels {
        for (i, c) in channels.channels.iter().enumerate() {
            registry.insert(pangya_model::Channel::new(
                (i + 1) as u8,
                c.name.clone(),
                c.max_user,
                c.max_level,
                c.flag,
            ));
        }
    }
    let channels_snapshot: Vec<pangya_model::Channel> = registry
        .list()
        .iter()
        .map(|c| pangya_model::Channel::new(c.id, c.name.clone(), c.max_user, c.max_level, c.flag))
        .collect();

    let runtime = Arc::new(Runtime::new());
    let state = Arc::new(ServerState::new(Arc::new(registry)));

    let addr = format!("{}:{}", cfg.server.ip, cfg.server.port);
    let listener = TcpListener::bind(&addr).await?;
    info!("[{}] listening on {addr}", LOG_PREFIX);

    loop {
        tokio::select! {
            _ = runtime.shutdown.notified() => {
                info!("[{}] shutdown signal received", LOG_PREFIX);
                break;
            }
            accept = listener.accept() => {
                match accept {
                    Ok((stream, peer)) => {
                        let pool = Arc::clone(&pool);
                        let state = Arc::clone(&state);
                        let channels = channels_snapshot.clone();
                        tokio::spawn(handle_client(stream, peer.to_string(), pool, state, channels));
                    }
                    Err(e) => error!("[{}] accept failed: {e}", LOG_PREFIX),
                }
            }
        }
    }

    info!("[{}] Game Server stopped", LOG_PREFIX);
    Ok(())
}

/// Per-client connection handler.
async fn handle_client(
    stream: tokio::net::TcpStream,
    peer: String,
    pool: Arc<DbPool>,
    state: Arc<ServerState>,
    channels: Vec<pangya_model::Channel>,
) {
    let session_key: u8 = rand::thread_rng().gen_range(0..=15);
    let sk = SessionKey(session_key);

    let (read_half, mut write_half) = stream.into_split();

    // Send the raw 0x3F greeting via raw framing (makeRaw in C++): no crypto,
    // no compress. Body = opcode 0x3F + [1, 1, session_key].
    let greeting_body = game_resp::build_greeting(session_key);
    let mut greeting_frame = Vec::new();
    if let Err(e) = framing::encode_raw(&greeting_body, &mut greeting_frame) {
        warn!("[{}] {peer}: failed to encode greeting: {e}", LOG_PREFIX);
        return;
    }
    pangya_server_core::packet_log::log_packet(
        pangya_server_core::packet_log::Dir::S2C,
        "GS",
        &greeting_body,
    );
    if let Err(e) = write_half.write_all(&greeting_frame).await {
        warn!("[{}] {peer}: greeting send failed: {e}", LOG_PREFIX);
        return;
    }

    let mut decoder = PangyaDecoder::new(Format::Client, sk);
    let mut buf = BytesMut::with_capacity(8192);
    let mut reader = read_half;

    let mut uid: Option<i64> = None;
    let mut nickname: String = String::new();
    let mut current_channel: Option<u8> = None;
    // The player's loaded equipment (characters, caddies, warehouse, mascots,
    // equip slots, clubset). Used by the 0x000B (change item) handler to answer
    // any item-type request from real state.
    let mut equipment: Option<pangya_server_core::game_login::PlayerEquipment> = None;

    loop {
        let mut tmp = [0u8; 8192];
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

        loop {
            match decoder.decode(&mut buf) {
                Ok(Some(frame)) => {
                    pangya_server_core::packet_log::log_packet(
                        pangya_server_core::packet_log::Dir::C2S,
                        "GS",
                        &frame.body,
                    );
                    let (opcode, _payload) = split_opcode(&frame.body).unwrap_or((0xFFFF, &[]));
                    match opcode {
                        0x02 => {
                            if handle_login_packet(
                                &frame.body,
                                &pool,
                                &channels,
                                &mut uid,
                                &mut nickname,
                                &mut equipment,
                                sk,
                                &mut write_half,
                                &peer,
                            )
                            .await
                            {
                                return;
                            }
                        }
                        0x03 => {
                            if let Ok(GamePacket::Chat(ChatRequest { nickname, message })) =
                                GamePacket::parse(&frame.body)
                            {
                                // Check for a GM command first (`/notice`, etc.).
                                use pangya_server_core::gm::{try_gm_command, GmResult};
                                match try_gm_command(&nickname, &message) {
                                    GmResult::Broadcast(body) => {
                                        // Broadcast to the channel (or back to sender).
                                        if let Some(ch) = current_channel {
                                            state.broadcast_channel(ch, &body).await;
                                        }
                                        let _ = send_server(&body, sk, &mut write_half).await;
                                    }
                                    GmResult::Handled | GmResult::Invalid(_) => {
                                        // Acknowledge silently; the GM sees no chat echo.
                                    }
                                    GmResult::NotACommand => {
                                        let chat = game_resp::build_chat(0, &nickname, &message);
                                        if let Some(ch) = current_channel {
                                            state.broadcast_channel(ch, &chat).await;
                                        } else {
                                            let _ = send_server(&chat, sk, &mut write_half).await;
                                        }
                                    }
                                }
                            }
                        }
                        0x04 => {
                            if let Ok(GamePacket::EnterChannel(EnterChannelRequest {
                                channel_id,
                            })) = GamePacket::parse(&frame.body)
                            {
                                if let Some(old) = current_channel {
                                    if let Some(u) = uid {
                                        state.leave_channel(old, u).await;
                                    }
                                }
                                let result = match uid {
                                    Some(u) => match state.enter_channel(channel_id, u).await {
                                        state::EnterResult::Success => {
                                            current_channel = Some(channel_id);
                                            1u8
                                        }
                                        state::EnterResult::Full => 2u8,
                                        state::EnterResult::NotFound => 6u8,
                                    },
                                    None => {
                                        warn!(
                                            "[{}] {peer}: enter channel before login",
                                            LOG_PREFIX
                                        );
                                        6u8
                                    }
                                };
                                // On success, send the full channel-enter sequence
                                // matching the C++ channel::enterChannel + enterLobby:
                                // 0x95 (notice) → 0x4E (result) → 0x46 (lobby players)
                                // → 0x47 (room list).
                                if result == 1 {
                                    let notice = game_resp::build_channel_enter_notice(0);
                                    let _ = send_server(&notice, sk, &mut write_half).await;
                                }
                                let body = game_resp::build_channel_enter_result(result);
                                let _ = send_server(&body, sk, &mut write_half).await;
                                if result == 1 {
                                    // Lobby data: send this player's canal info
                                    // (option 4 = first player, clears view), then
                                    // the room list.
                                    let pci = build_player_canal_info(uid, &nickname, &equipment);
                                    let players = game_resp::build_lobby_players(4, &[pci]);
                                    let _ = send_server(&players, sk, &mut write_half).await;
                                    let rooms = game_resp::build_lobby_room_list(0);
                                    let _ = send_server(&rooms, sk, &mut write_half).await;
                                }
                            }
                        }
                        0x08 => {
                            // MakeRoom (create room). Build a full Room from the
                            // request fields, register it, and reply with 0x49
                            // carrying the complete RoomInfo struct (mirrors
                            // channel::requestMakeRoom → room::sendMake → pacote049).
                            if let Some(u) = uid {
                                if let Ok(GamePacket::MakeRoom(req)) =
                                    GamePacket::parse(&frame.body)
                                {
                                    let room = pangya_model::Room {
                                        id: 0, // assigned by create_room_full
                                        name: req.name.clone(),  // Vec<u8> raw bytes
                                        key: pangya_model::Room::generate_key_pub(),
                                        senha_flag: if req.password.is_empty() { 1 } else { 0 },
                                        state: 1,      // waiting
                                        flag: 0,
                                        max_player: req.max_player,
                                        num_player: 1,
                                        qntd_hole: req.qntd_hole,
                                        tipo_show: req.tipo,
                                        numero: 0, // assigned
                                        modo: req.modo,
                                        course: req.course,
                                        time_vs: req.time_vs,
                                        trofel: 0,
                                        state_flag: 0,
                                        rate_pang: 100,
                                        rate_exp: 100,
                                        flag_gm: 0,
                                        master: u as i32,
                                        tipo_ex: 0xFF, // ~0 for normal rooms (mirrors room.cpp:979)
                                        artefato: req.artefato,
                                        leader_uid: u,
                                        players: vec![u],
                                    };
                                    let created = state.create_room_full(room);
                                    info!(
                                        "[{}] {peer}: created room {} '{}' (course={}, modo={}, max={})",
                                        LOG_PREFIX, created.id, String::from_utf8_lossy(&created.name), req.course, req.modo, req.max_player
                                    );
                                    // The C++ sends these in a specific order
                                    // (channel.cpp:1614-1618):
                                    //   sendUpdate() → 0x4A (room state)
                                    //   sendMake()   → 0x49 (room created)
                                    //   sendCharacter() → 0x48 (player list)
                                    // Batch the core 3 into a single TCP write —
                                    // the client may expect them to arrive together.
                                    let room_wire = game_resp::RoomInfoWire::from_room(&created);

                                    let update = game_resp::build_room_update(&room_wire);
                                    let ack = game_resp::build_make_room_result(0, &room_wire);

                                    // Build the 0x48 player list.
                                    let players_pkt = if let Some(eq) = &equipment {
                                        let equipped_char = eq
                                            .characters
                                            .iter()
                                            .find(|c| c.id == eq.equip.character_id)
                                            .or_else(|| eq.characters.first());
                                        let mut state_flag = 0b0000_1000u16; // master (bit 3)
                                        if eq.sex != 0 {
                                            state_flag |= 0b0010_0000; // sexo (bit 5)
                                        }
                                        // Master is always ready (room.cpp:1141).
                                        state_flag |= 0b0010_0000_0000_0000; // ready (bit 9)
                                        let pri = pangya_model::PlayerRoomInfo {
                                            oid: u as u32,
                                            nickname: nickname.clone(),
                                            position: 1,
                                            char_typeid: equipped_char
                                                .map(|c| c.typeid as u32)
                                                .unwrap_or(0),
                                            state_flag,
                                            level: 1,
                                            uid: u as u32,
                                            character: equipped_char.cloned(),
                                            ..Default::default()
                                        };
                                        game_resp::build_room_players(&[pri])
                                    } else {
                                        Vec::new()
                                    };

                                    // Send 0x4A + 0x49 + 0x48 as a single batched write.
                                    let mut batch = Vec::new();
                                    for body in [&update, &ack, &players_pkt] {
                                        if body.is_empty() {
                                            continue;
                                        }
                                        pangya_server_core::packet_log::log_packet(
                                            pangya_server_core::packet_log::Dir::S2C,
                                            "GS",
                                            body,
                                        );
                                        let low_key: u8 = rand::thread_rng().gen_range(1..=255);
                                        if let Err(e) = framing::encode_server(body, sk, low_key, &mut batch) {
                                            warn!("[{}] {peer}: frame encode error: {e}", LOG_PREFIX);
                                        }
                                    }
                                    let _ = write_half.write_all(&batch).await;

                                    // 0x47 option 1: broadcast the new room to the
                                    // channel lobby (mirrors sendUpdateRoomInfo(ri, 1)).
                                    // The creator also receives this — it's needed for
                                    // the client to register the room in the lobby list.
                                    let rooms_pkt = game_resp::build_room_list(&[room_wire.clone()], 1);
                                    let _ = send_server(&rooms_pkt, sk, &mut write_half).await;

                                    // 0x46 option 3: update the player's lobby state
                                    // (mirrors sendUpdatePlayerInfo(session, 3)).
                                    // This carries the player's updated sala_numero,
                                    // telling the client it's now in a room. Without
                                    // this the client may keep room features disabled.
                                    let mut pci = build_player_canal_info(uid, &nickname, &equipment);
                                    pci.sala_numero = created.numero;
                                    let player_update = game_resp::build_lobby_players(3, &[pci]);
                                    let _ = send_server(&player_update, sk, &mut write_half).await;
                                }
                            } else {
                                warn!("[{}] {peer}: create room before login", LOG_PREFIX);
                            }
                        }
                        0x09 => {
                            // EnterRoom
                            if let Some(u) = uid {
                                if let Ok(GamePacket::EnterRoom(req)) =
                                    GamePacket::parse(&frame.body)
                                {
                                    let room_id = req.room_numero as u32;
                                    let entered = state.room_add_player(room_id, u);
                                    info!(
                                        "[{}] {peer}: enter room {room_id} -> {}",
                                        LOG_PREFIX,
                                        if entered { "ok" } else { "denied" }
                                    );
                                }
                            }
                        }
                        0x0A => {
                            // LeaveRoom
                            if let Some(u) = uid {
                                // Best-effort: leave whatever room the player is in.
                                for room in state.list_rooms() {
                                    if room.players.contains(&u) {
                                        state.room_remove_player(room.id, u);
                                        break;
                                    }
                                }
                            }
                        }
                        0xFE => {
                            // Handshake confirm — client expects 0x1B1 response.
                            // Without this the client hangs after login.
                            let resp = game_resp::build_handshake_confirm();
                            let _ = send_server(&resp, sk, &mut write_half).await;
                        }
                        0x0B | 0x0C => {
                            // Change Player Item — 0x0B is on Channel, 0x0C is in
                            // Room. Both have the same format (type byte + item id)
                            // and the same 0x4B response (pacote04B):
                            // error(4) + type(1) + oid(4) + struct.
                            // type 1=Caddie(25B), 2=Ball(4B), 3=ClubSet(28B),
                            // 4=Character(513B), 5=Mascot(62B).
                            // The client sends 0x0C right after room creation to
                            // sync the equipped character — without a response the
                            // room UI stays incomplete (course select grayed).
                            let payload = &frame.body[2..];
                            let item_type = payload.first().copied().unwrap_or(0);

                            let resp = if let Some(eq) = &equipment {
                                match item_type {
                                    // Caddie — find the equipped caddie by id.
                                    1 => {
                                        let ci = eq
                                            .caddies
                                            .iter()
                                            .find(|c| c.id == eq.equip.caddie_id)
                                            .or_else(|| eq.caddies.first());
                                        if let Some(ci) = ci {
                                            game_resp::build_change_item_caddie(0, ci.id as u32, ci)
                                        } else {
                                            game_resp::build_change_item_result(0, 1)
                                        }
                                    }
                                    // Ball — return the equipped ball typeid.
                                    2 => game_resp::build_change_item_ball(
                                        0,
                                        0,
                                        eq.equip.ball_typeid,
                                    ),
                                    // ClubSet — return the equipped clubset stats.
                                    3 => game_resp::build_change_item_clubset(
                                        0,
                                        eq.clubset_info.id as u32,
                                        &eq.clubset_info,
                                    ),
                                    // Character — find the equipped character by id.
                                    4 => {
                                        let ci = eq
                                            .characters
                                            .iter()
                                            .find(|c| c.id == eq.equip.character_id)
                                            .or_else(|| eq.characters.first());
                                        if let Some(ci) = ci {
                                            game_resp::build_change_item_character(
                                                0,
                                                ci.id as u32,
                                                ci,
                                            )
                                        } else {
                                            game_resp::build_change_item_result(0, 4)
                                        }
                                    }
                                    // Mascot — find the equipped mascot by id.
                                    5 => {
                                        let mi = eq
                                            .mascots
                                            .iter()
                                            .find(|m| m.id == eq.equip.mascot_id)
                                            .or_else(|| eq.mascots.first());
                                        if let Some(mi) = mi {
                                            game_resp::build_change_item_mascot(
                                                0,
                                                mi.id as u32,
                                                mi,
                                            )
                                        } else {
                                            game_resp::build_change_item_result(0, 5)
                                        }
                                    }
                                    _ => game_resp::build_change_item_result(0, item_type),
                                }
                            } else {
                                // No equipment loaded (pre-login) — best-effort empty ack.
                                game_resp::build_change_item_result(0, item_type)
                            };
                            let _ = send_server(&resp, sk, &mut write_half).await;
                        }
                        0x81 => {
                            // Enter multiplayer lobby (requestEnterLobby /
                            // enterLobbyMultiPlayer). Mirrors the C++ sequence:
                            // 0x46 (players, option 4 clears view) → 0x47 (rooms)
                            // → 0xF5 (enter-lobby ack).
                            let pci = build_player_canal_info(uid, &nickname, &equipment);
                            let players = game_resp::build_lobby_players(4, &[pci]);
                            let _ = send_server(&players, sk, &mut write_half).await;
                            let rooms = game_resp::build_lobby_room_list(0);
                            let _ = send_server(&rooms, sk, &mut write_half).await;
                            let ack = game_resp::build_enter_lobby_ack();
                            let _ = send_server(&ack, sk, &mut write_half).await;
                        }
                        0x16E => {
                            // Check Attendance Reward — respond with empty notice.
                            let resp = game_resp::build_notice_ack(0);
                            let _ = send_server(&resp, sk, &mut write_half).await;
                        }
                        0x09C => {
                            // Last5Player request — no-op ack (client tolerates silence).
                        }
                        _ => info!("[{}] {peer}: unhandled opcode {opcode:#06x}", LOG_PREFIX),
                    }
                }
                Ok(None) => break,
                Err(e) => {
                    warn!("[{}] {peer}: decode error: {e}", LOG_PREFIX);
                    return;
                }
            }
        }
    }

    if let (Some(ch), Some(u)) = (current_channel, uid) {
        state.leave_channel(ch, u).await;
    }
}

#[allow(clippy::too_many_arguments)] // private dispatch helper, args grow with state
async fn handle_login_packet<W: AsyncWriteExt + Unpin>(
    body: &[u8],
    pool: &DbPool,
    channels: &[pangya_model::Channel],
    uid: &mut Option<i64>,
    nickname: &mut String,
    equipment: &mut Option<pangya_server_core::game_login::PlayerEquipment>,
    sk: SessionKey,
    writer: &mut W,
    peer: &str,
) -> bool {
    match GamePacket::parse(body) {
        Ok(GamePacket::Login(req)) => {
            let outcome = handle_game_login(pool, &req, channels).await;
            match outcome {
                Ok(GameLoginOutcome::Accepted {
                    uid: logged_uid,
                    nickname: logged_nick,
                    bodies,
                    equipment: loaded_eq,
                }) => {
                    *uid = Some(logged_uid);
                    *nickname = logged_nick;
                    *equipment = Some(*loaded_eq);
                    for body in bodies {
                        if let Err(e) = send_server(&body, sk, writer).await {
                            warn!("[{}] {peer}: send error: {e}", LOG_PREFIX);
                            return true;
                        }
                    }
                    false
                }
                Ok(GameLoginOutcome::Denied { body, disconnect }) => {
                    let _ = send_server(&body, sk, writer).await;
                    disconnect
                }
                Err(e) => {
                    error!("[{}] {peer}: login handler error: {e}", LOG_PREFIX);
                    false
                }
            }
        }
        _ => {
            warn!("[{}] {peer}: unexpected non-login 0x02", LOG_PREFIX);
            false
        }
    }
}

/// Build a `PlayerCanalInfo` for the connected player, for the lobby player
/// list (`0x46`). Uses the connection's uid, nickname, and account sex.
fn build_player_canal_info(
    uid: Option<i64>,
    nickname: &str,
    equipment: &Option<pangya_server_core::game_login::PlayerEquipment>,
) -> pangya_model::PlayerCanalInfo {
    let uid_val = uid.unwrap_or(0) as u32;
    // state_flag bits: sexo(1), azinha(4). Gender is required for the client.
    let mut state_flag = 0u8;
    if let Some(eq) = equipment {
        if eq.sex != 0 {
            state_flag |= 0b0000_0010; // sexo (bit 1)
        }
        state_flag |= 0b0001_0000; // azinha (<3% quit rate)
    }
    pangya_model::PlayerCanalInfo {
        uid: uid_val,
        oid: uid_val,
        sala_numero: -1, // in lobby, not in a room
        nickname: nickname.to_owned(),
        level: 1,
        state_flag,
        ..Default::default()
    }
}

async fn send_server<W: AsyncWriteExt + Unpin>(
    body: &[u8],
    sk: SessionKey,
    writer: &mut W,
) -> anyhow::Result<()> {
    pangya_server_core::packet_log::log_packet(
        pangya_server_core::packet_log::Dir::S2C,
        "GS",
        body,
    );
    let mut frame = Vec::with_capacity(body.len() + 16);
    let low_key: u8 = rand::thread_rng().gen_range(1..=255);
    framing::encode_server(body, sk, low_key, &mut frame)?;
    writer.write_all(&frame).await?;
    Ok(())
}
