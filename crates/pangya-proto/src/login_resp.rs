//! Login Server **response** (server → client) packet builders.
//!
//! These write the plaintext body that the framing layer then compresses and
//! encrypts. Field layouts mirror the C++ `pacote010`, `pacote002`, etc. in
//! `Login Server/PACKET/packet_func_ls.cpp`.

use crate::write_opcode;

/// A fixed-size string field as written by `addString`'s underlying
/// `addBuffer(&x, N)` for struct members: raw bytes, no length prefix.
pub fn write_fixed_string(out: &mut Vec<u8>, s: &str, len: usize) {
    let bytes = s.as_bytes();
    let n = bytes.len().min(len);
    out.extend_from_slice(&bytes[..n]);
    out.extend(std::iter::repeat(0u8).take(len - n));
}

/// `i16`-length-prefixed UTF-8 string, matching `packet::addString`.
pub fn write_lp_string(out: &mut Vec<u8>, s: &str) {
    let len = s.len() as i16;
    out.extend_from_slice(&len.to_le_bytes());
    out.extend_from_slice(s.as_bytes());
}

/// `0x10` — Login success: sends the auth key the client presents to the Game
/// Server. Mirrors `pacote010`.
pub fn build_login_success(auth_key: &str) -> Vec<u8> {
    let mut out = Vec::with_capacity(2 + 2 + auth_key.len());
    write_opcode(0x10, &mut out);
    write_lp_string(&mut out, auth_key);
    out
}

/// `0x01` — Login error/denied. `code` selects the reason (6 = wrong ID/PW,
/// 0xE2 + detail = already logged in, etc.). Mirrors `pacote001(option)`.
pub fn build_login_error(code: u8, detail: Option<i32>) -> Vec<u8> {
    let mut out = Vec::with_capacity(8);
    write_opcode(0x01, &mut out);
    out.push(code);
    if let Some(d) = detail {
        out.extend_from_slice(&d.to_le_bytes());
    }
    out
}

/// `0x01` — Login success player-info (option 0). Mirrors `pacote001(option=0)`:
/// sends the player's UID, capability, level, nickname. The client needs this
/// to know its own UID before connecting to the Game Server.
pub fn build_login_player_info(
    id: &str,
    uid: i64,
    capability: i32,
    level: i16,
    nickname: &str,
) -> Vec<u8> {
    let mut out = Vec::with_capacity(80 + id.len() + nickname.len());
    write_opcode(0x01, &mut out);
    out.push(0); // option 0 = success
    write_lp_string(&mut out, id);
    out.extend_from_slice(&(uid as i32).to_le_bytes());
    out.extend_from_slice(&capability.to_le_bytes());
    out.extend_from_slice(&level.to_le_bytes());
    out.extend_from_slice(&0i32.to_le_bytes()); // unknown
    out.extend_from_slice(&5i32.to_le_bytes()); // unknown
    write_fixed_string(&mut out, "0000-00-00 00:00:00", 19); // build date
    write_lp_string(&mut out, "302540"); // auth key placeholder
    out.extend_from_slice(&0u32.to_le_bytes()); // unknown
    out.extend_from_slice(&0u32.to_le_bytes()); // unknown
    write_lp_string(&mut out, nickname);
    out.extend_from_slice(&0i16.to_le_bytes());
    out
}

/// `0x03` — Select server response. Mirrors `pacote003`: the game auth key the
/// client presents to the Game Server. `option` is normally 0 (success).
pub fn build_select_server_response(auth_key: &str, option: i32) -> Vec<u8> {
    let mut out = Vec::with_capacity(6 + auth_key.len());
    write_opcode(0x03, &mut out);
    out.extend_from_slice(&option.to_le_bytes());
    write_lp_string(&mut out, auth_key);
    out
}

/// One server-list entry: the 92-byte `ServerInfo` struct, written verbatim.
/// Mirrors `pacote002`'s `addBuffer(&v_element[i], sizeof(ServerInfo))`.
pub fn write_server_info(out: &mut Vec<u8>, info: &ServerInfoWire) {
    write_fixed_string(out, &info.name, 40);
    out.extend_from_slice(&info.uid.to_le_bytes());
    out.extend_from_slice(&info.max_user.to_le_bytes());
    out.extend_from_slice(&info.curr_user.to_le_bytes());
    write_fixed_string(out, &info.ip, 18);
    out.extend_from_slice(&info.port.to_le_bytes());
    out.extend_from_slice(&info.property.to_le_bytes());
    out.extend_from_slice(&info.angelic_wings_num.to_le_bytes());
    out.extend_from_slice(&info.event_flag.to_le_bytes());
    out.extend_from_slice(&info.event_map.to_le_bytes());
    out.extend_from_slice(&info.app_rate.to_le_bytes());
    out.extend_from_slice(&info.unknown.to_le_bytes());
    out.extend_from_slice(&info.img_no.to_le_bytes());
}

/// `0x02` — Server list (game servers). Mirrors `pacote002`.
pub fn build_server_list(servers: &[ServerInfoWire]) -> Vec<u8> {
    let mut out = Vec::with_capacity(3 + servers.len() * 92);
    write_opcode(0x02, &mut out);
    out.push(servers.len() as u8);
    for s in servers {
        write_server_info(&mut out, s);
    }
    out
}

/// `0x09` — Message-server list. Same body shape as `0x02`. Mirrors `pacote009`.
pub fn build_message_server_list(servers: &[ServerInfoWire]) -> Vec<u8> {
    let mut out = Vec::with_capacity(3 + servers.len() * 92);
    write_opcode(0x09, &mut out);
    out.push(servers.len() as u8);
    for s in servers {
        write_server_info(&mut out, s);
    }
    out
}

/// The 92-byte `ServerInfo` wire struct (`pangya_st.h:178`). All fields are
/// little-endian on the wire.
#[derive(Debug, Clone)]
pub struct ServerInfoWire {
    pub name: String,
    pub uid: i32,
    pub max_user: i32,
    pub curr_user: i32,
    pub ip: String,
    pub port: i32,
    pub property: u32,
    pub angelic_wings_num: i32,
    pub event_flag: u16,
    pub event_map: i16,
    pub app_rate: i16,
    pub unknown: i16,
    pub img_no: i16,
}

impl ServerInfoWire {
    /// On-wire size, matching `sizeof(ServerInfo)` in the C++.
    pub const WIRE_SIZE: usize = 92;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn login_success_body_format() {
        let body = build_login_success("DEADBEEF");
        assert_eq!(body[0..2], [0x10, 0x00]); // opcode LE
        assert_eq!(body[2..4], [8, 0]); // string length 8 as i16 LE
        assert_eq!(&body[4..], b"DEADBEEF");
    }

    #[test]
    fn login_error_body_format() {
        let body = build_login_error(6, None);
        assert_eq!(body, vec![0x01, 0x00, 0x06]);

        let body = build_login_error(0xE2, Some(500010));
        assert_eq!(body[0..2], [0x01, 0x00]);
        assert_eq!(body[2], 0xE2);
        assert_eq!(i32::from_le_bytes(body[3..7].try_into().unwrap()), 500010);
    }

    #[test]
    fn server_info_is_92_bytes() {
        let info = ServerInfoWire {
            name: "Test Server".into(),
            uid: 20203,
            max_user: 2000,
            curr_user: 0,
            ip: "127.0.0.1".into(),
            port: 20203,
            property: 0,
            angelic_wings_num: 0,
            event_flag: 0,
            event_map: 0,
            app_rate: 0,
            unknown: 0,
            img_no: 2,
        };
        let mut buf = Vec::new();
        write_server_info(&mut buf, &info);
        assert_eq!(buf.len(), ServerInfoWire::WIRE_SIZE);
    }

    #[test]
    fn server_list_round_trips_count_and_entries() {
        let servers = vec![
            ServerInfoWire {
                name: "A".into(),
                uid: 1,
                max_user: 10,
                curr_user: 2,
                ip: "1.1.1.1".into(),
                port: 100,
                property: 0,
                angelic_wings_num: 0,
                event_flag: 0,
                event_map: 0,
                app_rate: 0,
                unknown: 0,
                img_no: 0,
            },
            ServerInfoWire {
                name: "B".into(),
                uid: 2,
                max_user: 20,
                curr_user: 5,
                ip: "2.2.2.2".into(),
                port: 200,
                property: 0,
                angelic_wings_num: 0,
                event_flag: 0,
                event_map: 0,
                app_rate: 0,
                unknown: 0,
                img_no: 0,
            },
        ];
        let body = build_server_list(&servers);
        assert_eq!(body[0..2], [0x02, 0x00]); // opcode
        assert_eq!(body[2], 2); // count
        assert_eq!(body.len(), 3 + 2 * ServerInfoWire::WIRE_SIZE);
        // First entry's uid is at offset 3 (opcode+count) + 40 (name) = 43.
        assert_eq!(i32::from_le_bytes(body[43..47].try_into().unwrap()), 1);
    }

    #[test]
    fn fixed_string_is_nul_padded() {
        let mut buf = Vec::new();
        write_fixed_string(&mut buf, "ab", 5);
        assert_eq!(buf, vec![b'a', b'b', 0, 0, 0]);
    }
}
