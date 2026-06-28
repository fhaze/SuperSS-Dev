//! Domain model for the Pangya server.
//!
//! These are the in-memory types the server logic operates on, distinct from the
//! wire structs in `pangya-proto`. For Milestone 1 this covers the account,
//! auth-key, and server-registry aggregates; the full `PlayerInfo` aggregate
//! (characters, caddies, mascots, warehouse, cards, mail, …) is added as each
//! system is ported.

pub mod account;
pub mod auth;
pub mod channel;
pub mod player;
pub mod server_list;

pub use account::{Account, AuthKey};
pub use auth::{gen_auth_key, md5_hex};
pub use channel::{Channel, ChannelRegistry, Room, RoomRegistry};
pub use player::{
    CaddieInfo, CharacterInfo, ClubSetInfo, MascotInfo, MemberInfo, PlayerCanalInfo,
    PlayerIdentity, PlayerRoomInfo, PlayerState, UserEquip, UserInfo, WarehouseItem,
};
pub use server_list::ServerEntry;

/// Server type (`tipo`) used for inter-server addressing and the client's server
/// list. Values mirror the C++ `m_si.tipo` assignments.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(i16)]
pub enum ServerType {
    /// Login, Game, Rank, Message all report this generic game-server type.
    Game = 1,
    /// The Auth Server hub.
    Auth = 5,
}

impl ServerType {
    pub fn from_raw(raw: i16) -> Option<Self> {
        Some(match raw {
            1 => Self::Game,
            5 => Self::Auth,
            _ => return None,
        })
    }
}
