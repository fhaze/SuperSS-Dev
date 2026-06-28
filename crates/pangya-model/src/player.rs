//! Player-related domain types: the loaded player state and its sub-aggregates.
//!
//! These mirror the C++ structs in `pangya_game_st.h` (the POD `player_info`,
//! `UserInfo`/`UserInfoEx`, `UserEquip`, `CharacterInfo`) and `player_info.h`
//! (the `PlayerInfo` aggregate). Only the fields needed for Milestone 1 (login
//! + lobby) are modelled now; the full aggregate grows per system.

use crate::account::Account;

/// The minimal identity row from `ProcGetPlayerInfoGame` (the C++ `player_info`
/// POD). Field order matches the DB result, not the struct declaration.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PlayerIdentity {
    pub uid: i64,
    pub id: String,
    pub nickname: String,
    pub level: u16,
    /// The account block-state bitfield (`IDStateBlockFlag`).
    pub id_state: u64,
    pub block_time: i32,
}

/// `MemberInfoEx` — identity data sent to the client in the player-info packet.
/// Mirrors `pangya_game_st.h:500`. Fixed-size arrays are kept as `Vec`/`String`
/// in the domain layer; the wire layer re-packs them.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MemberInfo {
    pub id: String,
    pub nickname: String,
    pub guild_name: String,
    pub guild_mark_img: String,
    pub capability: u32,
    pub oid: u32,
    pub guild_uid: u32,
    pub state_flag: u16,
    pub sex: i8,
    pub level: i8,
    pub do_tutorial: bool,
    pub school: i32,
    pub manner_flag: i16,
}

/// A subset of `UserInfo` (`pangya_game_st.h:607`) — the live game stats the
/// lobby cares about. The full 60+ field struct is expanded as systems need it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct UserInfo {
    pub pang: u64,
    /// Cash currency. In the C++ this is `PlayerInfo::cookie` (separate from the
    /// `UserInfo` struct); colocated here as the other spendable balance. Sent in
    /// the `0x96` packet, not the principal's UserInfo block.
    pub cookie: u64,
    pub exp: u32,
    pub level: u8,
    pub jogado: i32, // games played
}

/// `UserEquip` — the persisted equipment slot indices (`pangya_game_st.h:1003`).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct UserEquip {
    pub caddie_id: i32,
    pub character_id: i32,
    pub clubset_id: i32,
    pub ball_typeid: i32,
    pub item_slot: [i32; 10],
    pub skin_id: [i32; 6],
    pub skin_typeid: [i32; 6],
    pub mascot_id: i32,
    pub poster: [i32; 2],
}

/// `CharacterInfo` (`pangya_st.h:389`) — one character instance.
///
/// Carries every field the 513-byte wire struct needs (see
/// `pangya-proto::game_resp::write_character_info`). The full 81-column DB
/// struct grows per system; fields the lobby doesn't touch yet default to zero.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CharacterInfo {
    pub typeid: i32,
    pub id: i32,
    pub default_hair: u8,
    pub default_shirts: u8,
    pub gift_flag: u8,
    pub purchase: u8,
    /// Equipped part typeids and their per-instance ids (slots 1..24).
    pub parts_typeid: [i32; 24],
    pub parts_id: [i32; 24],
    /// Auxiliary parts (rings, etc.), 5 slots.
    pub auxparts: [i32; 5],
    /// Cut-in ids, 4 slots.
    pub cut_in: [i32; 4],
    /// Character stats — power/control/accuracy/spin/curve.
    pub pcl: [u8; 5],
    pub mastery: i32,
    /// Card slots: character / caddie / NPC, 4 slots each.
    pub card_character: [i32; 4],
    pub card_caddie: [i32; 4],
    pub card_npc: [i32; 4],
}

impl CharacterInfo {
    /// Build a minimal valid character for a typeid with the given PCL stats.
    ///
    /// Used by the dev fallback (when no DB row exists) and by tests. Only the
    /// identity + stats fields are set; parts/equipment are left empty, which the
    /// client accepts for a beginner character.
    pub fn from_iff(typeid: i32, id: i32, pcl: [u8; 5]) -> Self {
        Self {
            typeid,
            id,
            pcl,
            ..Default::default()
        }
    }
}

/// `CaddieInfo` (`pangya_game_st.h:1068`) — the 25-byte wire struct for one
/// owned caddie. Fields mirror the C++ packed struct.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CaddieInfo {
    pub id: i32,
    pub typeid: i32,
    pub parts_typeid: i32,
    pub level: u8,
    pub exp: u32,
    pub rent_flag: u8,
    pub end_date_unix: u16,
    pub parts_end_date_unix: u16,
    pub purchase: u8,
    pub check_end: i16,
}

/// `ClubSetInfo` (`pangya_game_st.h:1144`) — the 28-byte wire struct for the
/// equipped clubset's stats. `slot_c`/`enchant_c` are workshop upgrades; they
/// stay zero until the clubset-stats system lands.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ClubSetInfo {
    pub id: i32,
    pub typeid: i32,
    pub slot_c: [i16; 5],
    pub enchant_c: [i16; 5],
}

/// `MascotInfo` (`pangya_game_st.h:1171`) — the 62-byte wire struct for one
/// owned mascot. The `data` SYSTEMTIME (rental expiry) is not modelled yet.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MascotInfo {
    pub id: i32,
    pub typeid: i32,
    pub level: u8,
    pub exp: u32,
    pub message: String,
    pub tipo: i16,
    pub flag: u8,
}

/// `WarehouseItem` (`pangya_game_st.h:1209`) — the 196-byte wire struct for one
/// owned warehouse item. The UCC (user-created content) and Card sub-structs are
/// not persisted yet; only the core item fields are modelled here.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct WarehouseItem {
    pub id: i32,
    pub typeid: i32,
    pub ano: i32,
    pub c: [i16; 5],
    pub purchase: u8,
    pub flag: u8,
    pub apply_date: i64,
    pub end_date: i64,
    pub item_type: u8,
    // UCC (79B), Card (48B), ClubsetWorkshop (28B) sub-structs are zero-filled
    // on the wire until those features land.
}

/// `PlayerRoomInfo` (`pangya_game_st.h:2189`) — the 348-byte wire struct for one
/// player inside a room (sent in `0x48`). Only the lobby-visible identity fields
/// are modelled; the rest are zero-filled on the wire. The `PlayerRoomInfoEx`
/// variant appends a full `CharacterInfo` (513 bytes).
#[derive(Debug, Clone, Default)]
pub struct PlayerRoomInfo {
    pub oid: u32,
    pub nickname: String,
    pub guild_name: String,
    pub position: u8,
    pub capability: u32,
    pub title: u32,
    pub char_typeid: u32,
    pub skin: [u32; 6],
    /// Bitfield: team, away, master, sex, ready, quit-rate flags, etc.
    pub state_flag: u16,
    pub level: u8,
    pub uid: u32,
    pub mascot_typeid: u32,
    /// The player's equipped character, appended for the `Ex` variant.
    pub character: Option<CharacterInfo>,
}

/// `PlayerCanalInfo` (`pangya_game_st.h:2149`) — the 200-byte wire struct for
/// one player in the channel lobby (sent in `0x46`).
#[derive(Debug, Clone, Default)]
pub struct PlayerCanalInfo {
    pub uid: u32,
    pub oid: u32,
    /// Room number (-1 = in lobby, not in a room).
    pub sala_numero: i16,
    pub nickname: String,
    pub level: u8,
    pub capability: u32,
    pub title: i32,
    pub team_point: i32,
    /// Bitfield: away, sexo, quiter_1/2, azinha, icon_angel.
    pub state_flag: u8,
    pub guild_uid: u32,
    pub guild_index_mark: u32,
}

/// The assembled player aggregate — what `LoginTask` loads and `principal()`
/// serializes. Currently carries the identity + member info + equip + characters;
/// the full aggregate (caddies, mascots, warehouse, cards, mail, …) is added per
/// system. Built from `Account` + `PlayerIdentity` + sub-collection repos.
#[derive(Debug, Clone, Default)]
pub struct PlayerState {
    pub identity: PlayerIdentity,
    pub member: MemberInfo,
    pub user_info: UserInfo,
    pub equip: UserEquip,
    pub characters: Vec<CharacterInfo>,
    /// Whether this connection has cleared the game-server login gate
    /// (the C++ `m_is_authorized`).
    pub authorized: bool,
}

impl PlayerState {
    pub fn from_account(account: &Account) -> Self {
        let mut s = Self::default();
        s.identity.uid = account.uid;
        s.identity.id = account.id.clone();
        s.identity.nickname = account.nickname.clone();
        s.identity.level = 1;
        s
    }
}
