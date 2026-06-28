//! Server-registry domain type: one row of the server list shown to clients.

use crate::ServerType;

/// A live server entry (the `pangya_server_list` table, refreshed by each
/// server's register heartbeat and read by the Login Server).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerEntry {
    pub name: String,
    pub uid: u32,
    pub ip: String,
    pub port: u16,
    pub max_user: u32,
    pub curr_user: u32,
    pub tipo: ServerType,
    pub state: i16,
    pub exp_rate: u32,
    pub pang_rate: u32,
    pub img_no: u16,
    pub property: u32,
}
