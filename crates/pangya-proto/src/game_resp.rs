//! Game Server **response** (server → client) packet builders for the lobby flow.
//!
//! Mirrors the C++ `pacote03F` (greeting), `pacote04D` (channel list),
//! `pacote04E` (channel-enter result), `pacote040` (chat), `pacote044` (ack).

use crate::login_resp::{write_fixed_string, write_lp_string};
use crate::write_opcode;

/// `0x3F` — connect greeting. **Raw** packet (no crypto/compress). Body is
/// `[1, 1, session_key]`. Seeds the client's cipher.
pub fn build_greeting(session_key: u8) -> Vec<u8> {
    let mut out = Vec::with_capacity(5);
    write_opcode(0x3F, &mut out);
    out.push(1);
    out.push(1);
    out.push(session_key);
    out
}

/// `0x44` — login ack. With option `0xD3` it's the thin "login accepted"
/// greeting; with `0xE2` + an int32 code it's a denial.
pub fn build_login_ack_d3() -> Vec<u8> {
    let mut out = Vec::with_capacity(5);
    write_opcode(0x44, &mut out);
    out.push(0xD3);
    out.push(0);
    out
}

/// `0x44` — login denied with an error code.
pub fn build_login_denied(code: i32) -> Vec<u8> {
    let mut out = Vec::with_capacity(7);
    write_opcode(0x44, &mut out);
    out.push(0xE2);
    out.extend_from_slice(&code.to_le_bytes());
    out
}

/// Serialize a `CharacterInfo` as the packed 513-byte wire struct, mirroring
/// `pangya_st.h:389` field-for-field:
///
/// `typeid:i32, id:i32, default_hair:u8, default_shirts:u8, gift_flag:u8,
/// purchase:u8, parts_typeid[24], parts_id[24], cblank1[216], auxparts[5],
/// cut_in[4], pcl[5], mastery:i32, card_character[4], card_caddie[4],
/// card_npc[4]` = 513 bytes.
///
/// The 216-byte `cblank1` gap (unknown scratch space in the C++ struct) is
/// zero-filled; it is not modelled in the domain layer yet.
pub fn write_character_info(out: &mut Vec<u8>, ci: &pangya_model::CharacterInfo) {
    let start = out.len();
    out.extend_from_slice(&ci.typeid.to_le_bytes());
    out.extend_from_slice(&ci.id.to_le_bytes());
    out.push(ci.default_hair);
    out.push(ci.default_shirts);
    out.push(ci.gift_flag);
    out.push(ci.purchase);
    for &v in &ci.parts_typeid {
        out.extend_from_slice(&v.to_le_bytes());
    }
    for &v in &ci.parts_id {
        out.extend_from_slice(&v.to_le_bytes());
    }
    out.resize(out.len() + 216, 0); // cblank1 — unknown scratch, zeroed
    for &v in &ci.auxparts {
        out.extend_from_slice(&v.to_le_bytes());
    }
    for &v in &ci.cut_in {
        out.extend_from_slice(&v.to_le_bytes());
    }
    out.extend_from_slice(&ci.pcl);
    out.extend_from_slice(&ci.mastery.to_le_bytes());
    for &v in &ci.card_character {
        out.extend_from_slice(&v.to_le_bytes());
    }
    for &v in &ci.card_caddie {
        out.extend_from_slice(&v.to_le_bytes());
    }
    for &v in &ci.card_npc {
        out.extend_from_slice(&v.to_le_bytes());
    }
    debug_assert_eq!(out.len() - start, 513, "CharacterInfo must be 513 bytes");
}

/// `0x44` (option 0) — Full player info ("principal"). Mirrors the C++
/// `principal()` function: serializes the complete player state the client
/// needs before it can function in the lobby. Struct sizes match the C++
/// packed structs; fields we don't have yet are zero-filled.
///
/// `equipped_char` is the player's equipped `CharacterInfo` (or `None` to
/// zero-fill that block — which the client rejects, so callers should always
/// supply a real character).
pub fn build_player_info(
    client_version: &str,
    uid: i64,
    id: &str,
    nickname: &str,
    server_property: i32,
    equipped_char: Option<&pangya_model::CharacterInfo>,
) -> Vec<u8> {
    let mut out = Vec::with_capacity(13000);
    write_opcode(0x44, &mut out);
    out.push(0x00); // option 0 = full player info

    // Client version (length-prefixed string)
    write_lp_string(&mut out, client_version);

    // sala_numero (i16) = -1 (not in a room)
    out.extend_from_slice(&(-1i16).to_le_bytes());

    // MemberInfo (297 bytes packed) — fill id + nickname, zero the rest.
    // Size confirmed empirically from the live C++ server's 0x0044 packet.
    let mi_start = out.len();
    write_fixed_string(&mut out, id, 22); // id[22]
    write_fixed_string(&mut out, nickname, 22); // nick_name[22]
    out.resize(mi_start + 297, 0); // pad to full MemberInfo size

    // uid (u32)
    out.extend_from_slice(&(uid as u32).to_le_bytes());

    // UserInfo (245 bytes) — all zeros (fresh account).
    // Size confirmed empirically from the live C++ server's 0x0044 packet.
    out.resize(out.len() + 245, 0);

    // TrofelInfo (90 bytes) — zeros
    out.resize(out.len() + 90, 0);

    // UserEquip (116 bytes) — zeros
    out.resize(out.len() + 116, 0);

    // Map Statistics: MS_NUM_MAPS(22) entries × 3 modes (normal/natural/GP) × sizeof(43)
    // + 9 seasons × MS_NUM_MAPS(22) × sizeof(43) = 12 arrays total
    out.resize(out.len() + 22 * 12 * 43, 0);

    // CharacterInfo (513 bytes) — the equipped character, or zeros.
    if let Some(ci) = equipped_char {
        write_character_info(&mut out, ci);
    } else {
        out.resize(out.len() + 513, 0);
    }

    // CaddieInfo (25 bytes) — zeros
    out.resize(out.len() + 25, 0);

    // ClubSetInfo (28 bytes) — zeros
    out.resize(out.len() + 28, 0);

    // MascotInfo (70 bytes) — zeros
    out.resize(out.len() + 70, 0);

    // SYSTEMTIME (16 bytes) — current time, zeros suffice
    out.resize(out.len() + 16, 0);

    // flag_login_time (u16) = 2 (already logged in before)
    out.extend_from_slice(&2u16.to_le_bytes());

    // PlayerPapelShopInfo (6 bytes) — zeros
    out.resize(out.len() + 6, 0);

    // i32 = 0
    out.extend_from_slice(&0i32.to_le_bytes());

    // u64 block flags = 0
    out.extend_from_slice(&0u64.to_le_bytes());

    // u32 login count = 0
    out.extend_from_slice(&0u32.to_le_bytes());

    // i32 server property
    out.extend_from_slice(&server_property.to_le_bytes());

    out
}

/// One channel entry as written in the channel-list packet (`0x4D`). Mirrors
/// the C++ `ChannelInfo` struct (`pangya_game_st.h:1934`) exactly, packed:
/// `name[64]`, `max_user:i16`, `curr_user:i16`, `id:u8`, `flag:u32`,
/// `flag2:i32`, `min_level_allow:i32`, `max_level_allow:i32` = 85 bytes.
pub fn write_channel_entry(out: &mut Vec<u8>, ch: &ChannelInfoWire) {
    write_fixed_string(out, &ch.name, 64);
    out.extend_from_slice(&ch.max_user.to_le_bytes());
    out.extend_from_slice(&ch.curr_user.to_le_bytes());
    out.push(ch.id);
    out.extend_from_slice(&ch.flag.to_le_bytes());
    out.extend_from_slice(&ch.flag2.to_le_bytes());
    out.extend_from_slice(&ch.min_level_allow.to_le_bytes());
    out.extend_from_slice(&ch.max_level_allow.to_le_bytes());
}

/// `0x4D` — channel list.
pub fn build_channel_list(channels: &[ChannelInfoWire]) -> Vec<u8> {
    let mut out = Vec::with_capacity(3 + channels.len() * ChannelInfoWire::WIRE_SIZE);
    write_opcode(0x4D, &mut out);
    out.push(channels.len() as u8);
    for c in channels {
        write_channel_entry(&mut out, c);
    }
    out
}

/// `0x4E` — channel-enter result. option: 1 = success/already-in, 2 = full,
/// 6 = error.
pub fn build_channel_enter_result(option: u8) -> Vec<u8> {
    let mut out = Vec::with_capacity(3);
    write_opcode(0x4E, &mut out);
    out.push(option);
    out
}

/// `0x95` — pre-channel-enter notice. Mirrors `pacote095(sub_tipo=0x102)`:
/// sent before `0x4E` on channel enter. Triggers the client to request MSN data.
pub fn build_channel_enter_notice(option: u8) -> Vec<u8> {
    let mut out = Vec::with_capacity(5);
    write_opcode(0x95, &mut out);
    out.extend_from_slice(&0x102u16.to_le_bytes()); // sub_tipo
    out.push(option);
    out
}

/// `0x46` — lobby player data. option 4 = first player (clears view),
/// option 5 = remaining players, option 1 = broadcast "player joined".
/// For an empty lobby (just this player), option 4 with one entry suffices.
pub fn build_lobby_players(option: u8, player_count: u8) -> Vec<u8> {
    let mut out = Vec::with_capacity(4);
    write_opcode(0x46, &mut out);
    out.push(option);
    out.push(player_count);
    out
}

/// `0x47` — room list in the channel. option 0, then count, then room entries.
/// For now sends an empty room list (no rooms created yet).
pub fn build_lobby_room_list(room_count: u8) -> Vec<u8> {
    let mut out = Vec::with_capacity(4);
    write_opcode(0x47, &mut out);
    out.push(0); // option
    out.push(room_count);
    out
}

/// `0x4B` — Change Player Item response. Mirrors `pacote04B`: error code + type.
/// Sent in response to `0x000B` (Change Player Item on Channel).
pub fn build_change_item_result(error: i32, item_type: u8) -> Vec<u8> {
    let mut out = Vec::with_capacity(7);
    write_opcode(0x4B, &mut out);
    out.extend_from_slice(&error.to_le_bytes());
    if error == 0 {
        out.push(item_type);
    }
    out
}

/// `0x4B` — Change Player Item response for an equipped **character** (type 4).
/// Mirrors `pacote04B` for the character case: `error:i32, type:u8(=4),
/// oid:u32, CharacterInfo(513)`. The client sends `0x000B` with type=4 on login
/// and expects the full equipped character back — a zeroed `CharacterInfo`
/// (typeid=0) causes it to hang in "Loading..." / disconnect.
pub fn build_change_item_character(
    error: i32,
    oid: u32,
    ci: &pangya_model::CharacterInfo,
) -> Vec<u8> {
    let mut out = Vec::with_capacity(9 + 513);
    write_opcode(0x4B, &mut out);
    out.extend_from_slice(&error.to_le_bytes());
    if error == 0 {
        out.push(4); // type = Character
        out.extend_from_slice(&oid.to_le_bytes());
        write_character_info(&mut out, ci);
    }
    out
}

// ── equipment cascade (LoginTask::sendCompleteData) ──────────────────────────
//
// After the principal packet (`0x44` option 0), the C++ server sends a burst of
// collection packets that the client needs to finish loading. For a fresh
// account the lists are mostly empty, but the client still expects each packet.

/// `0x70` — character list. Mirrors `pacote070`:
/// `opcode(2) + count(2) + count(2) + count × CharacterInfo(513)`.
pub fn build_character_list(chars: &[pangya_model::CharacterInfo]) -> Vec<u8> {
    let mut out = Vec::with_capacity(6 + chars.len() * 513);
    write_opcode(0x70, &mut out);
    let n = chars.len() as i16;
    out.extend_from_slice(&n.to_le_bytes());
    out.extend_from_slice(&n.to_le_bytes());
    for ci in chars {
        write_character_info(&mut out, ci);
    }
    out
}

/// `0x71` — caddie list. Mirrors `pacote071`:
/// `opcode(2) + count(2) + count(2) + count × CaddieInfo(25)`.
/// Empty for a fresh account.
pub fn build_caddie_list(count: u16) -> Vec<u8> {
    let mut out = Vec::with_capacity(6);
    write_opcode(0x71, &mut out);
    out.extend_from_slice(&(count as i16).to_le_bytes());
    out.extend_from_slice(&(count as i16).to_le_bytes());
    out
}

/// `0x73` — warehouse items. Mirrors `pacote073`:
/// `opcode(2) + count(2) + count(2) + count × WarehouseItem`.
/// Empty for a fresh account.
pub fn build_warehouse_list(count: u16) -> Vec<u8> {
    let mut out = Vec::with_capacity(6);
    write_opcode(0x73, &mut out);
    out.extend_from_slice(&(count as i16).to_le_bytes());
    out.extend_from_slice(&(count as i16).to_le_bytes());
    out
}

/// `0xE1` — mascot list. Mirrors `pacote0E1`:
/// `opcode(2) + count(1) + count × MascotInfo(70)`.
/// Empty for a fresh account.
pub fn build_mascot_list(count: u8) -> Vec<u8> {
    let mut out = Vec::with_capacity(3);
    write_opcode(0xE1, &mut out);
    out.push(count);
    out
}

/// `0x72` — user equip. Mirrors `pacote072`:
/// `opcode(2) + UserEquip(116)`. The 116-byte struct is zero-filled until the
/// equipment tables land; the equipped character_id is set so the client knows
/// which character it is using.
pub fn build_user_equip(equipped_char_id: i32) -> Vec<u8> {
    let mut out = Vec::with_capacity(2 + 116);
    write_opcode(0x72, &mut out);
    // UserEquip layout (pangya_game_st.h:1003): caddie_id, character_id,
    // clubset_id, ball_typeid, item_slot[10], skin_id[6], skin_typeid[6],
    // mascot_id, poster[2]. Zero everything except character_id.
    out.resize(out.len() + 4, 0); // caddie_id = 0
    out.extend_from_slice(&equipped_char_id.to_le_bytes()); // character_id
    out.resize(out.len() + 104, 0); // rest of UserEquip
    out
}

/// `0x6B` — Set Notice (attendance/caddie holiday). Simple ack with option.
pub fn build_notice_ack(option: u8) -> Vec<u8> {
    let mut out = Vec::with_capacity(3);
    write_opcode(0x6B, &mut out);
    out.push(option);
    out
}

/// `0x1B1` — response to `0x00FE`. Mirrors `pacote1B1`: a fixed handshake
/// confirmation the client expects right after receiving player info.
pub fn build_handshake_confirm() -> Vec<u8> {
    let mut out = Vec::with_capacity(34);
    write_opcode(0x1B1, &mut out);
    out.extend_from_slice(&0x0132DC55i32.to_le_bytes());
    out.push(0x19);
    out.extend_from_slice(&[0u8; 6]);
    out.extend_from_slice(&0x2211u16.to_le_bytes());
    out.extend_from_slice(&[0u8; 17]);
    out.push(0x11);
    out.extend_from_slice(&0u16.to_le_bytes());
    out
}

/// `0x40` — lobby chat broadcast. option 0 = normal, 0x80 = GM.
pub fn build_chat(option: u8, nickname: &str, message: &str) -> Vec<u8> {
    let mut out = Vec::with_capacity(3 + nickname.len() + message.len() + 4);
    write_opcode(0x40, &mut out);
    out.push(option);
    write_lp_string(&mut out, nickname);
    write_lp_string(&mut out, message);
    out
}

/// `0x40` (option 7) — a server notice broadcast (e.g. a GM announcement).
pub fn build_notice(source: &str, message: &str) -> Vec<u8> {
    let mut out = Vec::with_capacity(3 + source.len() + message.len() + 4);
    write_opcode(0x40, &mut out);
    out.push(7); // notice
    write_lp_string(&mut out, source);
    write_lp_string(&mut out, message);
    out
}

/// One room entry as written in the room-list packet (`0x47`). Mirrors the
/// lobby-visible fields of `RoomInfo` (`pangya_game_st.h:2417`).
pub fn write_room_entry(out: &mut Vec<u8>, room: &RoomInfoWire) {
    write_fixed_string(out, &room.name, 64);
    out.push(room.senha_flag);
    out.push(room.state);
    out.push(room.flag);
    out.push(room.max_player);
    out.push(room.num_player);
    write_fixed_string(out, &room.key, 17);
    out.push(room._30s);
    out.push(room.qntd_hole);
    out.push(room.tipo_show);
    out.extend_from_slice(&room.numero.to_le_bytes());
    out.push(room.modo);
    out.push(room.course);
    out.extend_from_slice(&room.time_vs.to_le_bytes());
    out.extend_from_slice(&room.trofel.to_le_bytes());
    out.extend_from_slice(&room.state_flag.to_le_bytes());
    out.extend_from_slice(&room.rate_pang.to_le_bytes());
    out.extend_from_slice(&room.rate_exp.to_le_bytes());
    out.push(room.flag_gm);
    out.extend_from_slice(&room.master.to_le_bytes());
    out.push(room.tipo_ex);
    out.extend_from_slice(&room.artefato.to_le_bytes());
    // natural (u32) + grand_prix tail omitted for the lobby-list view.
}

/// `0x47` — room list within a channel.
pub fn build_room_list(rooms: &[RoomInfoWire]) -> Vec<u8> {
    let mut out = Vec::with_capacity(3 + rooms.len() * 110);
    write_opcode(0x47, &mut out);
    out.push(0); // option 0
    out.push(rooms.len() as u8);
    for r in rooms {
        write_room_entry(&mut out, r);
    }
    out
}

/// `0x49` — create-room result (room id assigned). A simple ack variant.
pub fn build_create_room_result(room_numero: i16) -> Vec<u8> {
    let mut out = Vec::with_capacity(5);
    write_opcode(0x49, &mut out);
    out.extend_from_slice(&room_numero.to_le_bytes());
    out
}

/// The room-list wire entry. Fields mirror the C++ `RoomInfo` struct.
#[derive(Debug, Clone, Default)]
pub struct RoomInfoWire {
    pub name: String,
    pub senha_flag: u8,
    pub state: u8,
    pub flag: u8,
    pub max_player: u8,
    pub num_player: u8,
    pub key: String,
    pub _30s: u8,
    pub qntd_hole: u8,
    pub tipo_show: u8,
    pub numero: i16,
    pub modo: u8,
    pub course: u8,
    pub time_vs: u32,
    pub trofel: u32,
    pub state_flag: u16,
    pub rate_pang: u32,
    pub rate_exp: u32,
    pub flag_gm: u8,
    pub master: i32,
    pub tipo_ex: u8,
    pub artefato: u32,
}

impl RoomInfoWire {
    pub fn from_room(r: &pangya_model::Room) -> Self {
        Self {
            name: r.name.clone(),
            senha_flag: r.senha_flag,
            state: r.state,
            flag: r.flag,
            max_player: r.max_player,
            num_player: r.num_player,
            key: String::new(),
            _30s: 30,
            qntd_hole: r.qntd_hole,
            tipo_show: r.tipo_show,
            numero: r.numero,
            modo: r.modo,
            course: r.course,
            time_vs: r.time_vs,
            trofel: r.trofel,
            state_flag: r.state_flag,
            rate_pang: r.rate_pang,
            rate_exp: r.rate_exp,
            flag_gm: r.flag_gm,
            master: r.master,
            tipo_ex: r.tipo_ex,
            artefato: r.artefato,
        }
    }
}
/// The channel-list wire entry. Mirrors `ChannelInfo` (`pangya_game_st.h:1934`).
#[derive(Debug, Clone)]
pub struct ChannelInfoWire {
    pub name: String,
    pub max_user: i16,
    pub curr_user: i16,
    pub id: u8,
    pub flag: u32,
    pub flag2: i32,
    pub min_level_allow: i32,
    pub max_level_allow: i32,
}

impl ChannelInfoWire {
    /// On-wire size, matching `sizeof(ChannelInfo)` in the C++ (packed).
    pub const WIRE_SIZE: usize = 85;

    /// Build from the domain `Channel` (and its registry id).
    pub fn from_channel(id: u8, ch: &pangya_model::Channel) -> Self {
        Self {
            name: ch.name.clone(),
            max_user: ch.max_user.min(i16::MAX as u32) as i16,
            curr_user: ch.curr_user().min(i16::MAX as u32) as i16,
            id,
            flag: ch.flag,
            flag2: 0,
            min_level_allow: 0,
            max_level_allow: ch.max_level as i32,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn greeting_body_format() {
        let body = build_greeting(7);
        assert_eq!(body, vec![0x3F, 0x00, 1, 1, 7]);
    }

    #[test]
    fn login_ack_d3_format() {
        let body = build_login_ack_d3();
        assert_eq!(body, vec![0x44, 0x00, 0xD3, 0x00]);
    }

    #[test]
    fn login_denied_format() {
        let body = build_login_denied(500020);
        assert_eq!(body[0..2], [0x44, 0x00]);
        assert_eq!(body[2], 0xE2);
        assert_eq!(i32::from_le_bytes(body[3..7].try_into().unwrap()), 500020);
    }

    #[test]
    fn channel_list_format() {
        // Layout matches the real ChannelInfo struct (pangya_game_st.h:1934),
        // validated against captured 0x004D packets from the live C++ server.
        let channels = vec![ChannelInfoWire {
            name: "Beginners".into(),
            max_user: 500,
            curr_user: 12,
            id: 0,
            flag: 512,
            flag2: 0,
            min_level_allow: 0,
            max_level_allow: 16,
        }];
        let body = build_channel_list(&channels);
        assert_eq!(body[0..2], [0x4D, 0x00]);
        assert_eq!(body[2], 1); // count
                                // Total: opcode(2) + count(1) + one 85-byte entry.
        assert_eq!(body.len(), 3 + ChannelInfoWire::WIRE_SIZE);
        assert_eq!(&body[3..12], b"Beginners"); // first 9 of the 64-byte name
                                                // max_user (i16 LE) at offset 3 + 64 = 67
        assert_eq!(i16::from_le_bytes([body[67], body[68]]), 500);
        // max_level_allow (i32 LE) at the end of the 85-byte entry
        let ml_off = 3 + 64 + 2 + 2 + 1 + 4 + 4 + 4; // = 84 from entry start +3
        assert_eq!(
            i32::from_le_bytes(body[ml_off..ml_off + 4].try_into().unwrap()),
            16
        );
    }

    #[test]
    fn chat_format() {
        let body = build_chat(0, "player", "hi");
        assert_eq!(body[0..2], [0x40, 0x00]);
        assert_eq!(body[2], 0); // normal option
    }

    #[test]
    fn character_info_is_513_bytes_with_correct_layout() {
        let ci = pangya_model::CharacterInfo::from_iff(0x04000001, 1, [9, 11, 6, 2, 2]);
        let mut buf = Vec::new();
        write_character_info(&mut buf, &ci);
        assert_eq!(buf.len(), 513);

        // typeid (i32 LE) at offset 0
        assert_eq!(
            i32::from_le_bytes(buf[0..4].try_into().unwrap()),
            0x04000001
        );
        // id (i32 LE) at offset 4
        assert_eq!(i32::from_le_bytes(buf[4..8].try_into().unwrap()), 1);
        // pcl[5] offset: typeid(4)+id(4)+4 bytes+parts_typeid(96)+parts_id(96)
        // +cblank1(216)+auxparts(20)+cut_in(16) = 456
        let pcl_off = 4 + 4 + 4 + 24 * 4 + 24 * 4 + 216 + 5 * 4 + 4 * 4;
        assert_eq!(&buf[pcl_off..pcl_off + 5], &[9, 11, 6, 2, 2]);
    }

    #[test]
    fn player_info_carries_equipped_character() {
        let ci = pangya_model::CharacterInfo::from_iff(0x04000001, 1, [9, 11, 6, 2, 2]);
        let with_char = build_player_info("SS.R7.995.00", 1, "test", "Tester", 2048, Some(&ci));
        let without = build_player_info("SS.R7.995.00", 1, "test", "Tester", 2048, None);
        // Same total size whether or not a character is supplied.
        assert_eq!(with_char.len(), without.len());

        // Locate the CharacterInfo block (it follows the map-statistics block).
        // Compute the offset the same way build_player_info lays it out.
        let ci_off = 2 /*opcode*/ + 1 /*option*/
            + 2 + 12 // lp client version ("SS.R7.995.00")
            + 2     // sala_numero
            + 297   // MemberInfo
            + 4     // uid
            + 245   // UserInfo
            + 90    // TrofelInfo
            + 116   // UserEquip
            + 22 * 12 * 43; // map statistics
        let typeid = i32::from_le_bytes(
            with_char[ci_off..ci_off + 4].try_into().unwrap(),
        );
        assert_eq!(typeid, 0x04000001, "equipped character typeid must be present");
        assert_eq!(
            i32::from_le_bytes(without[ci_off..ci_off + 4].try_into().unwrap()),
            0,
            "no-character variant zeroes the block"
        );
    }

    #[test]
    fn change_item_character_response_format() {
        let ci = pangya_model::CharacterInfo::from_iff(0x04000001, 1, [9, 11, 6, 2, 2]);
        let body = build_change_item_character(0, 7, &ci);
        assert_eq!(body[0..2], [0x4B, 0x00]); // opcode
        assert_eq!(i32::from_le_bytes(body[2..6].try_into().unwrap()), 0); // error
        assert_eq!(body[6], 4); // type = Character
        assert_eq!(u32::from_le_bytes(body[7..11].try_into().unwrap()), 7); // oid
        // Followed by the 513-byte CharacterInfo.
        assert_eq!(body.len(), 11 + 513);
        assert_eq!(
            i32::from_le_bytes(body[11..15].try_into().unwrap()),
            0x04000001
        );
    }
}
