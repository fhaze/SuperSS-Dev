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
