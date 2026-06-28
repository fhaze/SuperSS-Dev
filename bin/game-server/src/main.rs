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
    // The room the player is currently in (set on create/enter), so a
    // `0x0A` change-room-info knows which room to mutate.
    let mut current_room: Option<u32> = None;
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
                                    current_room = Some(created.id);
                                    info!(
                                        "[{}] {peer}: created room {} '{}' (course={}, modo={}, max={})",
                                        LOG_PREFIX, created.id, String::from_utf8_lossy(&created.name), req.course, req.modo, req.max_player
                                    );
                                    // 0x49: option 0 (success) + full RoomInfo.
                                    let room_wire = game_resp::RoomInfoWire::from_room(&created);
                                    let ack = game_resp::build_make_room_result(0, &room_wire);
                                    let _ = send_server(&ack, sk, &mut write_half).await;

                                    // 0x4A: room state update (mirrors
                                    // room::sendUpdate → pacote04A). Sent right
                                    // after 0x49 so the client syncs room config.
                                    let update = game_resp::build_room_update(&room_wire);
                                    let _ = send_server(&update, sk, &mut write_half).await;

                                    // 0x48: player list in the room (mirrors
                                    // room::sendCharacter option 0). The client
                                    // needs this to fully transition into the
                                    // room UI (enables course select, ready, etc.).
                                    // Build a PlayerRoomInfo from the creator's
                                    // identity + equipped character.
                                    if let Some(eq) = &equipment {
                                        let equipped_char = eq
                                            .characters
                                            .iter()
                                            .find(|c| c.id == eq.equip.character_id)
                                            .or_else(|| eq.characters.first());
                                        // state_flag bits: master(3), sexo(5),
                                        // ready(9). The live C++ server marks the
                                        // room master ready (room.cpp:1141); the
                                        // captured master had state_flag=0x0228
                                        // (master+sexo+ready). The gender bit is
                                        // required for the client to render the
                                        // player and enable the room UI.
                                        let mut state_flag = 0b0000_1000u16; // master (bit 3)
                                        if eq.sex != 0 {
                                            state_flag |= 0b0010_0000; // sexo (bit 5)
                                        }
                                        state_flag |= 0b0000_0010_0000_0000; // ready (bit 9)
                                        let pri = pangya_model::PlayerRoomInfo {
                                            // Must match the player's lobby oid
                                            // (0) so the client recognizes itself
                                            // in the room — see build_player_canal_info.
                                            oid: 0,
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
                                        let players_pkt = game_resp::build_room_players(&[pri]);
                                        let _ =
                                            send_server(&players_pkt, sk, &mut write_half).await;
                                    }

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
                                    if entered {
                                        current_room = Some(room_id);
                                    }
                                    info!(
                                        "[{}] {peer}: enter room {room_id} -> {}",
                                        LOG_PREFIX,
                                        if entered { "ok" } else { "denied" }
                                    );
                                }
                            }
                        }
                        0x0A => {
                            // Change Room Info (requestChangeInfoRoom). The room
                            // master changes settings — course, holes, mode, etc.
                            // Opcode 0x0A is INFO_CHANGE (game_server.cpp:302),
                            // NOT leave-room. Mirrors room::requestChangeInfoRoom:
                            // apply each change, then broadcast 0x4A (room update)
                            // and 0x47 (lobby room list). Only the master may change
                            // the info (room.cpp:1456).
                            if let (Some(u), Some(room_id)) = (uid, current_room) {
                                match parse_change_room_info(&frame.body[2..]) {
                                    Some(changes) => {
                                        let updated = state.update_room(room_id, |room| {
                                            if room.master as i64 == u {
                                                for ch in &changes {
                                                    apply_room_change(room, ch);
                                                }
                                            }
                                        });
                                        if let Some(room) = updated {
                                            if room.master as i64 == u {
                                                let wire =
                                                    game_resp::RoomInfoWire::from_room(&room);
                                                // 0x4A: room state update (course etc.).
                                                let upd = game_resp::build_room_update(&wire);
                                                let _ =
                                                    send_server(&upd, sk, &mut write_half).await;
                                                // 0x47 option 3: refresh the lobby list.
                                                let list =
                                                    game_resp::build_room_list(&[wire], 3);
                                                if let Some(ch) = current_channel {
                                                    state.broadcast_channel(ch, &list).await;
                                                }
                                                let _ =
                                                    send_server(&list, sk, &mut write_half).await;
                                            } else {
                                                warn!("[{}] {peer}: non-master tried to change room info", LOG_PREFIX);
                                            }
                                        }
                                    }
                                    None => warn!(
                                        "[{}] {peer}: malformed 0x0A change-room-info",
                                        LOG_PREFIX
                                    ),
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
        // The object id (m_oid) is a per-session handle the client learns at
        // login (principal MemberInfo.oid, which we leave 0) and uses to find
        // *itself* in lobby/room packets. It is NOT the uid. It must match the
        // oid in the 0x48 room packet, else the client can't identify itself in
        // the room and grays the room UI (course select, stats). For the single
        // test player this is 0; multiplayer needs a real per-session counter.
        oid: 0,
        sala_numero: -1, // in lobby, not in a room
        nickname: nickname.to_owned(),
        level: 1,
        state_flag,
        ..Default::default()
    }
}

/// One field change inside a `0x0A` change-room-info packet. Variants we model
/// mutate the room; `Other` is a recognized type whose value we consume (to keep
/// the parse aligned) but don't persist yet.
#[derive(Debug)]
enum RoomChange {
    Name(Vec<u8>),
    Senha(Vec<u8>),
    Tipo(u8),
    Course(u8),
    QntdHole(u8),
    Modo(u8),
    TempoVs(u32),
    MaxPlayer(u8),
    Artefato(u32),
    Other,
}

/// Parse a `0x0A` change-room-info body: `flag:i16, num_info:u8`, then
/// `num_info × (type:u8, value…)`. Mirrors `room::requestChangeInfoRoom`
/// (`room.cpp:1446`) and the `RoomInfo::INFO_CHANGE` enum. Returns `None` on a
/// truncated/unknown body so the caller leaves the room untouched.
fn parse_change_room_info(b: &[u8]) -> Option<Vec<RoomChange>> {
    fn u8at(b: &[u8], p: &mut usize) -> Option<u8> {
        let v = b.get(*p).copied()?;
        *p += 1;
        Some(v)
    }
    fn u16at(b: &[u8], p: &mut usize) -> Option<u16> {
        let s = b.get(*p..*p + 2)?;
        *p += 2;
        Some(u16::from_le_bytes([s[0], s[1]]))
    }
    fn u32at(b: &[u8], p: &mut usize) -> Option<u32> {
        let s = b.get(*p..*p + 4)?;
        *p += 4;
        Some(u32::from_le_bytes([s[0], s[1], s[2], s[3]]))
    }
    // i16-length-prefixed bytes (matching `packet::addString`/`readString`).
    fn lp(b: &[u8], p: &mut usize) -> Option<Vec<u8>> {
        let len = u16at(b, p)? as usize;
        let s = b.get(*p..*p + len)?;
        *p += len;
        Some(s.to_vec())
    }

    let mut p = 0usize;
    let _flag = u16at(b, &mut p)?; // INFO_CHANGE flag (unused)
    let num = u8at(b, &mut p)?;
    let mut out = Vec::with_capacity(num as usize);
    for _ in 0..num {
        let change = match u8at(b, &mut p)? {
            0 => RoomChange::Name(lp(b, &mut p)?),                  // NAME
            1 => RoomChange::Senha(lp(b, &mut p)?),                 // SENHA
            2 => RoomChange::Tipo(u8at(b, &mut p)?),               // TIPO
            3 => RoomChange::Course(u8at(b, &mut p)?),             // COURSE
            4 => RoomChange::QntdHole(u8at(b, &mut p)?),           // QNTD_HOLE
            5 => RoomChange::Modo(u8at(b, &mut p)?),               // MODO
            6 => RoomChange::TempoVs(u16at(b, &mut p)? as u32 * 1000), // TEMPO_VS (s→ms)
            7 => RoomChange::MaxPlayer(u8at(b, &mut p)?),          // MAX_PLAYER
            8 => {
                u8at(b, &mut p)?;
                RoomChange::Other
            } // TEMPO_30S
            9 => {
                u8at(b, &mut p)?;
                RoomChange::Other
            } // STATE_FLAG (AFK)
            11 => {
                u8at(b, &mut p)?;
                RoomChange::Other
            } // HOLE_REPEAT
            12 => {
                u32at(b, &mut p)?;
                RoomChange::Other
            } // FIXED_HOLE
            13 => RoomChange::Artefato(u32at(b, &mut p)?),         // ARTEFATO
            14 => {
                u32at(b, &mut p)?;
                RoomChange::Other
            } // NATURAL
            _ => return None, // unknown type — can't keep the cursor aligned
        };
        out.push(change);
    }
    Some(out)
}

/// Apply one parsed change to the room (mirrors the `set*` calls in
/// `room::requestChangeInfoRoom`).
fn apply_room_change(room: &mut pangya_model::Room, change: &RoomChange) {
    match change {
        RoomChange::Name(n) => room.name = n.clone(),
        RoomChange::Senha(s) => room.senha_flag = if s.is_empty() { 1 } else { 0 },
        RoomChange::Tipo(v) => room.tipo_show = *v,
        RoomChange::Course(v) => room.course = *v,
        RoomChange::QntdHole(v) => room.qntd_hole = *v,
        RoomChange::Modo(v) => room.modo = *v,
        RoomChange::TempoVs(v) => room.time_vs = *v,
        RoomChange::MaxPlayer(v) => room.max_player = *v,
        RoomChange::Artefato(v) => room.artefato = *v,
        RoomChange::Other => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_course_change() {
        // flag=0 (i16), num_info=1, type=3 (COURSE), value=14
        let body = [0u8, 0, 1, 3, 14];
        let changes = parse_change_room_info(&body).expect("parses");
        assert_eq!(changes.len(), 1);
        let mut room = pangya_model::Room {
            master: 1,
            course: 0,
            ..Default::default()
        };
        for c in &changes {
            apply_room_change(&mut room, c);
        }
        assert_eq!(room.course, 14);
    }

    #[test]
    fn parse_multi_change_course_and_holes() {
        // num_info=2: COURSE=5, then QNTD_HOLE=18
        let body = [0u8, 0, 2, 3, 5, 4, 18];
        let changes = parse_change_room_info(&body).expect("parses");
        let mut room = pangya_model::Room::default();
        for c in &changes {
            apply_room_change(&mut room, c);
        }
        assert_eq!((room.course, room.qntd_hole), (5, 18));
    }

    #[test]
    fn parse_rejects_truncated() {
        // Claims 1 change but the value byte is missing.
        assert!(parse_change_room_info(&[0u8, 0, 1, 3]).is_none());
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
