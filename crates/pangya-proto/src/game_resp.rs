//! Game Server **response** (server → client) packet builders for the lobby flow.
//!
//! Mirrors the C++ `pacote03F` (greeting), `pacote04D` (channel list),
//! `pacote04E` (channel-enter result), `pacote040` (chat), `pacote044` (ack).

use crate::login_resp::{write_fixed_bytes, write_fixed_string, write_lp_bytes, write_lp_string};
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

/// Serialize a `UserEquip` as the packed 116-byte wire struct, mirroring
/// `pangya_game_st.h:1003`: `caddie_id, character_id, clubset_id, ball_typeid,
/// item_slot[10], skin_id[6], skin_typeid[6], mascot_id, poster[2]`.
pub fn write_user_equip(out: &mut Vec<u8>, ue: &pangya_model::UserEquip) {
    let start = out.len();
    out.extend_from_slice(&ue.caddie_id.to_le_bytes());
    out.extend_from_slice(&ue.character_id.to_le_bytes());
    out.extend_from_slice(&ue.clubset_id.to_le_bytes());
    out.extend_from_slice(&ue.ball_typeid.to_le_bytes());
    for &v in &ue.item_slot {
        out.extend_from_slice(&v.to_le_bytes());
    }
    for &v in &ue.skin_id {
        out.extend_from_slice(&v.to_le_bytes());
    }
    for &v in &ue.skin_typeid {
        out.extend_from_slice(&v.to_le_bytes());
    }
    out.extend_from_slice(&ue.mascot_id.to_le_bytes());
    for &v in &ue.poster {
        out.extend_from_slice(&v.to_le_bytes());
    }
    debug_assert_eq!(out.len() - start, 116, "UserEquip must be 116 bytes");
}

/// Serialize a `CaddieInfo` as the packed 25-byte wire struct, mirroring
/// `pangya_game_st.h:1068`.
pub fn write_caddie_info(out: &mut Vec<u8>, ci: &pangya_model::CaddieInfo) {
    let start = out.len();
    out.extend_from_slice(&ci.id.to_le_bytes());
    out.extend_from_slice(&ci.typeid.to_le_bytes());
    out.extend_from_slice(&ci.parts_typeid.to_le_bytes());
    out.push(ci.level);
    out.extend_from_slice(&ci.exp.to_le_bytes());
    out.push(ci.rent_flag);
    out.extend_from_slice(&ci.end_date_unix.to_le_bytes());
    out.extend_from_slice(&ci.parts_end_date_unix.to_le_bytes());
    out.push(ci.purchase);
    out.extend_from_slice(&ci.check_end.to_le_bytes());
    debug_assert_eq!(out.len() - start, 25, "CaddieInfo must be 25 bytes");
}

/// Serialize a `ClubSetInfo` as the packed 28-byte wire struct, mirroring
/// `pangya_game_st.h:1144`.
pub fn write_clubset_info(out: &mut Vec<u8>, csi: &pangya_model::ClubSetInfo) {
    let start = out.len();
    out.extend_from_slice(&csi.id.to_le_bytes());
    out.extend_from_slice(&csi.typeid.to_le_bytes());
    for &v in &csi.slot_c {
        out.extend_from_slice(&v.to_le_bytes());
    }
    for &v in &csi.enchant_c {
        out.extend_from_slice(&v.to_le_bytes());
    }
    debug_assert_eq!(out.len() - start, 28, "ClubSetInfo must be 28 bytes");
}

/// Serialize a `MascotInfo` as the packed 62-byte wire struct, mirroring
/// `pangya_game_st.h:1171`. The `data` SYSTEMTIME (16 bytes) is zero-filled.
pub fn write_mascot_info(out: &mut Vec<u8>, mi: &pangya_model::MascotInfo) {
    let start = out.len();
    out.extend_from_slice(&mi.id.to_le_bytes());
    out.extend_from_slice(&mi.typeid.to_le_bytes());
    out.push(mi.level);
    out.extend_from_slice(&mi.exp.to_le_bytes());
    write_fixed_string(out, &mi.message, 30);
    out.extend_from_slice(&mi.tipo.to_le_bytes());
    out.resize(out.len() + 16, 0); // SYSTEMTIME data — zero-filled
    out.push(mi.flag);
    debug_assert_eq!(out.len() - start, 62, "MascotInfo must be 62 bytes");
}

/// Serialize a `WarehouseItem` as the packed 196-byte wire struct, mirroring
/// `pangya_game_st.h:1209`. The UCC (79B), Card (48B), and ClubsetWorkshop
/// (28B) sub-structs are zero-filled until those features land.
pub fn write_warehouse_item(out: &mut Vec<u8>, wi: &pangya_model::WarehouseItem) {
    let start = out.len();
    out.extend_from_slice(&wi.id.to_le_bytes());
    out.extend_from_slice(&wi.typeid.to_le_bytes());
    out.extend_from_slice(&wi.ano.to_le_bytes());
    for &v in &wi.c {
        out.extend_from_slice(&v.to_le_bytes());
    }
    out.push(wi.purchase);
    out.push(wi.flag);
    out.extend_from_slice(&wi.apply_date.to_le_bytes());
    out.extend_from_slice(&wi.end_date.to_le_bytes());
    out.push(wi.item_type);
    out.resize(out.len() + 79, 0); // UCC sub-struct — zero-filled
    out.resize(out.len() + 48, 0); // Card sub-struct — zero-filled
    out.resize(out.len() + 28, 0); // ClubsetWorkshop — zero-filled
    debug_assert_eq!(out.len() - start, 196, "WarehouseItem must be 196 bytes");
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
    equip: Option<&pangya_model::UserEquip>,
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

    // UserEquip (116 bytes) — the loaded equipment slots, or zeros.
    if let Some(ue) = equip {
        write_user_equip(&mut out, ue);
    } else {
        out.resize(out.len() + 116, 0);
    }

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

/// Serialize a `PlayerCanalInfo` as the packed 200-byte wire struct, mirroring
/// `pangya_game_st.h:2149`. Guild-mark and unknown tail fields are zero-filled.
pub fn write_player_canal_info(out: &mut Vec<u8>, pci: &pangya_model::PlayerCanalInfo) {
    let start = out.len();
    out.extend_from_slice(&pci.uid.to_le_bytes());
    out.extend_from_slice(&pci.oid.to_le_bytes());
    out.extend_from_slice(&pci.sala_numero.to_le_bytes());
    write_fixed_string(out, &pci.nickname, 22);
    out.push(pci.level);
    out.extend_from_slice(&pci.capability.to_le_bytes());
    out.extend_from_slice(&pci.title.to_le_bytes());
    out.extend_from_slice(&pci.team_point.to_le_bytes());
    out.push(pci.state_flag);
    out.extend_from_slice(&pci.guild_uid.to_le_bytes());
    out.extend_from_slice(&pci.guild_index_mark.to_le_bytes());
    out.resize(out.len() + 12, 0); // guild_mark_img[12]
    out.extend_from_slice(&0u16.to_le_bytes()); // flag_visible_gm
    out.extend_from_slice(&0i32.to_le_bytes()); // l_unknown
    out.resize(out.len() + 22, 0); // nickNT[22]
    out.resize(out.len() + 106, 0); // unknown106[106]
    debug_assert_eq!(out.len() - start, 200, "PlayerCanalInfo must be 200 bytes");
}

/// `0x46` — lobby player data. Mirrors `pacote046`:
/// `opcode(2) + option:u8 + count:u8 + count × PlayerCanalInfo(200)`.
/// Option 4 = first player (clears view), 5 = remaining, 1 = broadcast join.
pub fn build_lobby_players(option: u8, players: &[pangya_model::PlayerCanalInfo]) -> Vec<u8> {
    let mut out = Vec::with_capacity(4 + players.len() * 200);
    write_opcode(0x46, &mut out);
    out.push(option);
    out.push(players.len() as u8);
    for pci in players {
        write_player_canal_info(&mut out, pci);
    }
    out
}

/// `0x47` — room list in the channel. option 0, then count, then room entries.
/// For now sends an empty room list (no rooms created yet).
/// `0x47` — empty room list for channel enter / Game Play lobby.
/// Uses the full `pacote047` format: `count:u8(0) + option:i8(0) + numero:i16(-1)`.
pub fn build_lobby_room_list(_room_count: u8) -> Vec<u8> {
    let mut out = Vec::with_capacity(6);
    write_opcode(0x47, &mut out);
    out.push(0); // count = 0 (no rooms)
    out.push(0); // option = 0 (full list)
    out.extend_from_slice(&(-1i16).to_le_bytes()); // numero = -1
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

/// `0x4B` — Change Player Item response for an equipped **caddie** (type 1).
/// `error:i32, type:u8(=1), oid:u32, CaddieInfo(25)`.
pub fn build_change_item_caddie(
    error: i32,
    oid: u32,
    ci: &pangya_model::CaddieInfo,
) -> Vec<u8> {
    let mut out = Vec::with_capacity(9 + 25);
    write_opcode(0x4B, &mut out);
    out.extend_from_slice(&error.to_le_bytes());
    if error == 0 {
        out.push(1); // type = Caddie
        out.extend_from_slice(&oid.to_le_bytes());
        write_caddie_info(&mut out, ci);
    }
    out
}

/// `0x4B` — Change Player Item response for an equipped **ball** (type 2).
/// `error:i32, type:u8(=2), oid:u32, ball_typeid:u32`.
pub fn build_change_item_ball(error: i32, oid: u32, ball_typeid: i32) -> Vec<u8> {
    let mut out = Vec::with_capacity(13);
    write_opcode(0x4B, &mut out);
    out.extend_from_slice(&error.to_le_bytes());
    if error == 0 {
        out.push(2); // type = Ball
        out.extend_from_slice(&oid.to_le_bytes());
        out.extend_from_slice(&ball_typeid.to_le_bytes());
    }
    out
}

/// `0x4B` — Change Player Item response for an equipped **clubset** (type 3).
/// `error:i32, type:u8(=3), oid:u32, ClubSetInfo(28)`.
pub fn build_change_item_clubset(
    error: i32,
    oid: u32,
    csi: &pangya_model::ClubSetInfo,
) -> Vec<u8> {
    let mut out = Vec::with_capacity(9 + 28);
    write_opcode(0x4B, &mut out);
    out.extend_from_slice(&error.to_le_bytes());
    if error == 0 {
        out.push(3); // type = ClubSet
        out.extend_from_slice(&oid.to_le_bytes());
        write_clubset_info(&mut out, csi);
    }
    out
}

/// `0x4B` — Change Player Item response for an equipped **mascot** (type 5).
/// `error:i32, type:u8(=5), oid:u32, MascotInfo(62)`.
pub fn build_change_item_mascot(
    error: i32,
    oid: u32,
    mi: &pangya_model::MascotInfo,
) -> Vec<u8> {
    let mut out = Vec::with_capacity(9 + 62);
    write_opcode(0x4B, &mut out);
    out.extend_from_slice(&error.to_le_bytes());
    if error == 0 {
        out.push(5); // type = Mascot
        out.extend_from_slice(&oid.to_le_bytes());
        write_mascot_info(&mut out, mi);
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
pub fn build_caddie_list(caddies: &[pangya_model::CaddieInfo]) -> Vec<u8> {
    let mut out = Vec::with_capacity(6 + caddies.len() * 25);
    write_opcode(0x71, &mut out);
    let n = caddies.len() as i16;
    out.extend_from_slice(&n.to_le_bytes());
    out.extend_from_slice(&n.to_le_bytes());
    for ci in caddies {
        write_caddie_info(&mut out, ci);
    }
    out
}

/// `0x73` — warehouse items. Mirrors `pacote073`:
/// `opcode(2) + count(2) + count(2) + count × WarehouseItem(196)`.
pub fn build_warehouse_list(items: &[pangya_model::WarehouseItem]) -> Vec<u8> {
    let mut out = Vec::with_capacity(6 + items.len() * 196);
    write_opcode(0x73, &mut out);
    let n = items.len() as i16;
    out.extend_from_slice(&n.to_le_bytes());
    out.extend_from_slice(&n.to_le_bytes());
    for wi in items {
        write_warehouse_item(&mut out, wi);
    }
    out
}

/// `0xE1` — mascot list. Mirrors `pacote0E1`:
/// `opcode(2) + count(1) + count × MascotInfo(62)`.
pub fn build_mascot_list(mascots: &[pangya_model::MascotInfo]) -> Vec<u8> {
    let mut out = Vec::with_capacity(3 + mascots.len() * 62);
    write_opcode(0xE1, &mut out);
    out.push(mascots.len() as u8);
    for mi in mascots {
        write_mascot_info(&mut out, mi);
    }
    out
}

/// `0x72` — user equip. Mirrors `pacote072`: `opcode(2) + UserEquip(116)`.
pub fn build_user_equip(equip: &pangya_model::UserEquip) -> Vec<u8> {
    let mut out = Vec::with_capacity(2 + 116);
    write_opcode(0x72, &mut out);
    write_user_equip(&mut out, equip);
    out
}

/// `0x6B` — Set Notice (attendance/caddie holiday). Simple ack with option.
pub fn build_notice_ack(option: u8) -> Vec<u8> {
    let mut out = Vec::with_capacity(3);
    write_opcode(0x6B, &mut out);
    out.push(option);
    out
}

/// `0xF5` — enter multiplayer lobby ack. Mirrors `pacote0F5`: a bare opcode with
/// no payload, sent in response to `0x0081` (requestEnterLobby) after the lobby
/// data sequence (0x46 players + 0x47 rooms).
pub fn build_enter_lobby_ack() -> Vec<u8> {
    let mut out = Vec::with_capacity(2);
    write_opcode(0xF5, &mut out);
    out
}

/// Serialize a `PlayerRoomInfo` as the packed wire struct. With `include_char`
/// true, appends the full `CharacterInfo` (513 bytes) — the `PlayerRoomInfoEx`
/// variant used by `0x48`. Without it, writes the 348-byte base struct.
///
/// Mirrors `pangya_game_st.h:2189`. Fields we don't model (guild marks, shop,
/// location, 106-byte unknown tail, etc.) are zero-filled.
pub fn write_player_room_info(out: &mut Vec<u8>, pri: &pangya_model::PlayerRoomInfo, include_char: bool) {
    let start = out.len();
    out.extend_from_slice(&pri.oid.to_le_bytes());
    write_fixed_string(out, &pri.nickname, 22);
    write_fixed_string(out, &pri.guild_name, 20);
    out.push(pri.position);
    out.extend_from_slice(&pri.capability.to_le_bytes());
    out.extend_from_slice(&pri.title.to_le_bytes());
    out.extend_from_slice(&pri.char_typeid.to_le_bytes());
    for &v in &pri.skin {
        out.extend_from_slice(&v.to_le_bytes());
    }
    out.extend_from_slice(&pri.state_flag.to_le_bytes());
    out.push(pri.level);
    out.push(0); // icon_angel
    out.push(0x0A); // ucUnknown_0A (Place = Room)
    out.extend_from_slice(&0u32.to_le_bytes()); // guild_uid
    out.resize(out.len() + 12, 0); // guild_mark_img[12]
    out.extend_from_slice(&0u32.to_le_bytes()); // guild_mark_index
    out.extend_from_slice(&pri.uid.to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes()); // state_lounge
    out.extend_from_slice(&0u16.to_le_bytes()); // usUnknown_flg
    out.extend_from_slice(&0u32.to_le_bytes()); // state
    out.resize(out.len() + 12, 0); // location (3 floats)
    out.extend_from_slice(&0u32.to_le_bytes()); // shop.active
    out.resize(out.len() + 64, 0); // shop.name[64]
    out.extend_from_slice(&pri.mascot_typeid.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes()); // flag_item_boost
    out.extend_from_slice(&0u32.to_le_bytes()); // ulUnknown_flg
    out.resize(out.len() + 22, 0); // id_NT[22]
    out.resize(out.len() + 106, 0); // ucUnknown106[106]
    out.push(0); // convidado bitfield
    out.extend_from_slice(&0f32.to_le_bytes()); // avg_score
    out.resize(out.len() + 3, 0); // ucUnknown3[3]
    debug_assert_eq!(out.len() - start, 348, "PlayerRoomInfo base must be 348 bytes");
    if include_char {
        if let Some(ci) = &pri.character {
            write_character_info(out, ci);
        } else {
            out.resize(out.len() + 513, 0);
        }
    }
}

/// `0x48` — players in room. Mirrors `pacote048` (option 0, the "first player"
/// / room-enter case): `opcode(2) + option:u8 + numero:i16(-1) + count:i8 +
/// count × PlayerRoomInfoEx(861) + final_zero:u8`.
pub fn build_room_players(players: &[pangya_model::PlayerRoomInfo]) -> Vec<u8> {
    let mut out = Vec::with_capacity(7 + players.len() * 861);
    write_opcode(0x48, &mut out);
    out.push(0); // option 0 = first player / enter
    out.extend_from_slice(&(-1i16).to_le_bytes()); // numero = -1
    out.push(players.len() as u8); // count (signed i8 on the wire, 0..127)
    for pri in players {
        write_player_room_info(&mut out, pri, true); // Ex variant (with CharacterInfo)
    }
    out.push(0); // final list terminator
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
    write_fixed_bytes(out, &room.name, 64);
    out.push(room.senha_flag);
    out.push(room.state);
    out.push(room.flag);
    out.push(room.max_player);
    out.push(room.num_player);
    write_fixed_bytes(out, &room.key, 17);
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
/// `0x47` — room list / room update. Mirrors `pacote047`:
/// `opcode(2) + count:u8 + option:i8 + numero:i16(-1) + count × RoomInfo`.
/// Option 0 = full list (count = rooms.len()), 1 = room created,
/// 2 = room destroyed.
pub fn build_room_list(rooms: &[RoomInfoWire], option: i8) -> Vec<u8> {
    let mut out = Vec::with_capacity(6 + rooms.len() * 221);
    write_opcode(0x47, &mut out);
    // For option 0, count = number of rooms; for option 1/2, count = 1.
    let count = if option == 0 {
        rooms.len() as u8
    } else {
        rooms.len().min(1) as u8
    };
    out.push(count);
    out.push(option as u8);
    out.extend_from_slice(&(-1i16).to_le_bytes()); // numero = -1 (constant)
    for r in rooms {
        // pacote047 uses sizeof(RoomInfo) = the full 221-byte struct.
        write_room_info_full(&mut out, r);
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

/// Write the **full** `RoomInfo` struct (221 bytes, packed) — used by `0x0049`
/// (create-room result). Mirrors `pangya_game_st.h:2417` including the
/// `time_30s`, guild marks, natural, and grand-prix tail that the lobby-list
/// `write_room_entry` omits.
pub fn write_room_info_full(out: &mut Vec<u8>, room: &RoomInfoWire) {
    let start = out.len();
    write_fixed_bytes(out, &room.name, 64); // nome[64]
    out.push(room.senha_flag); // senha_flag (bitfield byte)
    out.push(room.state); // state (bitfield byte)
    out.push(room.flag);
    out.push(room.max_player);
    out.push(room.num_player);
    write_fixed_bytes(out, &room.key, 17); // key[17]
    out.push(room._30s);
    out.push(room.qntd_hole);
    out.push(room.tipo_show);
    out.extend_from_slice(&room.numero.to_le_bytes());
    out.push(room.modo);
    out.push(room.course);
    out.extend_from_slice(&room.time_vs.to_le_bytes());
    out.extend_from_slice(&room.time_30s.to_le_bytes());
    out.extend_from_slice(&room.trofel.to_le_bytes());
    out.extend_from_slice(&room.state_flag.to_le_bytes());
    // RoomGuildInfo (76 bytes): guild_1_uid, guild_2_uid, marks[12×2],
    // index_mark[2×2], names[20×2] — all zero for a non-guild room.
    out.resize(out.len() + 76, 0);
    out.extend_from_slice(&room.rate_pang.to_le_bytes());
    out.extend_from_slice(&room.rate_exp.to_le_bytes());
    out.push(room.flag_gm);
    out.extend_from_slice(&room.master.to_le_bytes());
    out.push(room.tipo_ex);
    out.extend_from_slice(&room.artefato.to_le_bytes());
    out.extend_from_slice(&room.natural.to_le_bytes());
    // RoomGrandPrixInfo (16 bytes): dados_typeid, rank_typeid, tempo, active.
    out.resize(out.len() + 16, 0);
    debug_assert_eq!(out.len() - start, 221, "RoomInfo must be 221 bytes");
}

/// `0x49` — create-room result. Mirrors `pacote049`:
/// `opcode(2) + option:i16 + RoomInfo(221)`. Option 0 = success, 2 = error.
pub fn build_make_room_result(option: i16, room: &RoomInfoWire) -> Vec<u8> {
    let mut out = Vec::with_capacity(2 + 2 + 221);
    write_opcode(0x49, &mut out);
    out.extend_from_slice(&option.to_le_bytes());
    if option == 0 {
        write_room_info_full(&mut out, room);
    }
    out
}

/// `0x4A` — room state update. Mirrors `pacote04A`:
/// `opcode(2) + option:i16(-1) + tipo_show:u8 + course:u8 + qntd_hole:u8 +
/// modo:u8 + natural:u32 + max_player:u8 + _30s:i8 + state_flag:u8 +
/// time_vs:u32 + time_30s:u32 + trofel:u32 + senha_flag:u8 + name:lp_string`.
/// Sent to the room after `0x49` (create) so the client syncs the room state.
pub fn build_room_update(room: &RoomInfoWire) -> Vec<u8> {
    let mut out = Vec::with_capacity(44);
    write_opcode(0x4A, &mut out);
    out.extend_from_slice(&(-1i16).to_le_bytes()); // option = -1 (constant)
    out.push(room.tipo_show);
    out.push(room.course);
    out.push(room.qntd_hole);
    out.push(room.modo);
    out.extend_from_slice(&room.natural.to_le_bytes());
    out.push(room.max_player);
    out.push(room._30s as i8 as u8); // _30s (signed, always 30)
    out.push((room.state_flag & 0xFF) as u8); // state_flag (low byte only)
    out.extend_from_slice(&room.time_vs.to_le_bytes());
    out.extend_from_slice(&room.time_30s.to_le_bytes());
    out.extend_from_slice(&room.trofel.to_le_bytes());
    out.push(room.senha_flag);
    write_lp_bytes(&mut out, &room.name);
    out
}

/// The room-list wire entry. Fields mirror the C++ `RoomInfo` struct.
#[derive(Debug, Clone, Default)]
pub struct RoomInfoWire {
    pub name: Vec<u8>,
    pub senha_flag: u8,
    pub state: u8,
    pub flag: u8,
    pub max_player: u8,
    pub num_player: u8,
    pub key: [u8; 17],
    pub _30s: u8,
    pub qntd_hole: u8,
    pub tipo_show: u8,
    pub numero: i16,
    pub modo: u8,
    pub course: u8,
    pub time_vs: u32,
    pub time_30s: u32,
    pub trofel: u32,
    pub state_flag: u16,
    pub rate_pang: u32,
    pub rate_exp: u32,
    pub flag_gm: u8,
    pub master: i32,
    pub tipo_ex: u8,
    pub artefato: u32,
    pub natural: u32,
}

impl RoomInfoWire {
    pub fn from_room(r: &pangya_model::Room) -> Self {
        Self {
            name: r.name.clone(),  // Vec<u8> — preserves raw Shift-JIS bytes
            senha_flag: r.senha_flag,
            state: r.state,
            flag: r.flag,
            max_player: r.max_player,
            num_player: r.num_player,
            key: r.key,
            _30s: 30,
            qntd_hole: r.qntd_hole,
            tipo_show: r.tipo_show,
            numero: r.numero,
            modo: r.modo,
            course: r.course,
            time_vs: r.time_vs,
            time_30s: 0,
            trofel: r.trofel,
            state_flag: r.state_flag,
            rate_pang: r.rate_pang,
            rate_exp: r.rate_exp,
            flag_gm: r.flag_gm,
            master: r.master,
            tipo_ex: r.tipo_ex,
            artefato: r.artefato,
            natural: 0,
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
        let with_char = build_player_info("SS.R7.995.00", 1, "test", "Tester", 2048, Some(&ci), None);
        let without = build_player_info("SS.R7.995.00", 1, "test", "Tester", 2048, None, None);
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

    #[test]
    fn user_equip_is_116_bytes_with_correct_layout() {
        let mut ue = pangya_model::UserEquip::default();
        ue.character_id = 1;
        ue.clubset_id = 0x01400001;
        ue.ball_typeid = 0x02000000;
        let mut buf = Vec::new();
        write_user_equip(&mut buf, &ue);
        assert_eq!(buf.len(), 116);
        assert_eq!(i32::from_le_bytes(buf[0..4].try_into().unwrap()), 0); // caddie_id
        assert_eq!(i32::from_le_bytes(buf[4..8].try_into().unwrap()), 1); // character_id
        assert_eq!(
            i32::from_le_bytes(buf[8..12].try_into().unwrap()),
            0x01400001 // clubset_id
        );
        assert_eq!(
            i32::from_le_bytes(buf[12..16].try_into().unwrap()),
            0x02000000 // ball_typeid
        );
    }

    #[test]
    fn caddie_info_is_25_bytes() {
        let ci = pangya_model::CaddieInfo {
            id: 5,
            typeid: 0x03000001,
            level: 1,
            ..Default::default()
        };
        let mut buf = Vec::new();
        write_caddie_info(&mut buf, &ci);
        assert_eq!(buf.len(), 25);
        assert_eq!(i32::from_le_bytes(buf[0..4].try_into().unwrap()), 5); // id
        assert_eq!(
            i32::from_le_bytes(buf[4..8].try_into().unwrap()),
            0x03000001 // _typeid
        );
    }

    #[test]
    fn clubset_info_is_28_bytes() {
        let csi = pangya_model::ClubSetInfo {
            id: 2,
            typeid: 0x01400001,
            ..Default::default()
        };
        let mut buf = Vec::new();
        write_clubset_info(&mut buf, &csi);
        assert_eq!(buf.len(), 28);
        assert_eq!(i32::from_le_bytes(buf[0..4].try_into().unwrap()), 2); // id
        assert_eq!(
            i32::from_le_bytes(buf[4..8].try_into().unwrap()),
            0x01400001 // _typeid
        );
    }

    #[test]
    fn mascot_info_is_62_bytes() {
        let mi = pangya_model::MascotInfo {
            id: 3,
            typeid: 0x06000001,
            message: "Hi".into(),
            ..Default::default()
        };
        let mut buf = Vec::new();
        write_mascot_info(&mut buf, &mi);
        assert_eq!(buf.len(), 62);
        assert_eq!(i32::from_le_bytes(buf[0..4].try_into().unwrap()), 3); // id
        assert_eq!(&buf[13..15], b"Hi"); // message[0..2] (after id+typeid+level+exp)
    }

    #[test]
    fn warehouse_item_is_196_bytes() {
        let wi = pangya_model::WarehouseItem {
            id: 1,
            typeid: 0x01400001,
            ..Default::default()
        };
        let mut buf = Vec::new();
        write_warehouse_item(&mut buf, &wi);
        assert_eq!(buf.len(), 196);
        assert_eq!(i32::from_le_bytes(buf[0..4].try_into().unwrap()), 1); // id
        assert_eq!(
            i32::from_le_bytes(buf[4..8].try_into().unwrap()),
            0x01400001 // _typeid
        );
    }

    #[test]
    fn collection_lists_serialize_entries() {
        let caddies = vec![pangya_model::CaddieInfo {
            id: 1,
            typeid: 0x03000001,
            ..Default::default()
        }];
        let body = build_caddie_list(&caddies);
        assert_eq!(body[0..2], [0x71, 0x00]);
        assert_eq!(i16::from_le_bytes(body[2..4].try_into().unwrap()), 1);
        assert_eq!(body.len(), 6 + 25);

        let mascots: Vec<pangya_model::MascotInfo> = vec![];
        let body = build_mascot_list(&mascots);
        assert_eq!(body[0..2], [0xE1, 0x00]);
        assert_eq!(body[2], 0);
    }

    #[test]
    fn change_item_variants_format() {
        let csi = pangya_model::ClubSetInfo {
            id: 2,
            typeid: 0x01400001,
            ..Default::default()
        };
        let body = build_change_item_clubset(0, 7, &csi);
        assert_eq!(body[6], 3); // type = ClubSet
        assert_eq!(body.len(), 11 + 28);

        let body = build_change_item_ball(0, 0, 0x02000000);
        assert_eq!(body[6], 2); // type = Ball
        assert_eq!(
            i32::from_le_bytes(body[11..15].try_into().unwrap()),
            0x02000000
        );
    }
}
