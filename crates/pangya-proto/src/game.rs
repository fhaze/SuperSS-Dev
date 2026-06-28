//! Game Server client→server packet parsing.
//!
//! Opcodes mirror the Game Server dispatch table in
//! `Game Server/Game Server/game_server.cpp`. The client connects, receives the
//! raw `0x3F` greeting, then sends `0x02` to log in.

use crate::{split_opcode, PayloadReader, ProtoError, UnknownPacket};

/// Game Server client→server opcodes used in the lobby flow.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u16)]
#[non_exhaustive]
pub enum GameOpcode {
    Login = 0x02,
    Chat = 0x03,
    EnterChannel = 0x04,
    MakeRoom = 0x08,
    EnterRoom = 0x09,
    LeaveRoom = 0x0A,
}

impl GameOpcode {
    pub fn from_raw(raw: u16) -> Option<Self> {
        Some(match raw {
            0x02 => Self::Login,
            0x03 => Self::Chat,
            0x04 => Self::EnterChannel,
            0x08 => Self::MakeRoom,
            0x09 => Self::EnterRoom,
            0x0A => Self::LeaveRoom,
            _ => return None,
        })
    }
}

/// A parsed Game Server packet (lobby + room subset).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GamePacket {
    Login(GameLoginRequest),
    Chat(ChatRequest),
    EnterChannel(EnterChannelRequest),
    MakeRoom(MakeRoomRequest),
    EnterRoom(EnterRoomRequest),
    LeaveRoom,
    Unknown(UnknownPacket),
}

impl GamePacket {
    pub fn parse(body: &[u8]) -> Result<Self, ProtoError> {
        let (raw, payload) = split_opcode(body).ok_or(ProtoError::EmptyBody)?;
        let mut reader = PayloadReader::new(payload);

        Ok(match GameOpcode::from_raw(raw) {
            Some(GameOpcode::Login) => GamePacket::Login(parse_game_login(&mut reader)?),
            Some(GameOpcode::Chat) => GamePacket::Chat(parse_chat(&mut reader)?),
            Some(GameOpcode::EnterChannel) => {
                GamePacket::EnterChannel(parse_enter_channel(&mut reader)?)
            }
            Some(GameOpcode::MakeRoom) => GamePacket::MakeRoom(parse_make_room(&mut reader)?),
            Some(GameOpcode::EnterRoom) => GamePacket::EnterRoom(parse_enter_room(&mut reader)?),
            Some(GameOpcode::LeaveRoom) => GamePacket::LeaveRoom,
            None => GamePacket::Unknown(UnknownPacket {
                opcode: raw,
                body: payload.to_vec(),
            }),
        })
    }
}

/// `0x02` — Game Server login. Field order from `requestLogin`
/// (game_server.cpp:1086-1126).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GameLoginRequest {
    pub id: String,
    pub uid: i64,
    /// "ntKey" — ignored by the server (logged only).
    pub nt_key: i32,
    /// "command" — ignored.
    pub command: u16,
    pub auth_key: String,
    pub client_version: String,
    /// XOR-encrypted packet version; verified against the server's expected
    /// version (the XOR is applied in the server binary, not the parser).
    pub packet_version: i32,
    pub mac_address: String,
    pub auth_key_2: String,
}

/// `0x03` — lobby/room chat.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChatRequest {
    pub nickname: String,
    pub message: String,
}

/// `0x04` — enter channel (1 byte: channel id).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EnterChannelRequest {
    pub channel_id: u8,
}

/// `0x08` — create room (MakeRoom). Field order from `channel::requestMakeRoom`
/// (channel.cpp:1388-1405).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MakeRoomRequest {
    pub option: u8,
    pub time_vs: u32,
    pub time_30s: u32,
    pub max_player: u8,
    pub tipo: u8,
    pub qntd_hole: u8,
    pub course: u8,
    pub modo: u8,
    /// For M_REPEAT mode: hole_repeat + fixed_hole.
    pub hole_repeat: Option<u8>,
    pub fixed_hole: Option<u32>,
    /// natural + short-game flag.
    pub natural: u32,
    pub name: String,
}

/// `0x09` — enter room.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnterRoomRequest {
    pub room_numero: i16,
    pub password: String,
}

fn parse_game_login(r: &mut PayloadReader) -> Result<GameLoginRequest, ProtoError> {
    let id = r.read_string("login.id")?;
    let uid = r.read_u32("login.uid")? as i64;
    let nt_key = r.read_u32("login.ntkey")? as i32;
    let command = r.read_u16("login.command")?;
    let auth_key = r.read_string("login.authkey")?;
    let client_version = r.read_string("login.clientver")?;
    let packet_version = r.read_u32("login.packetver")? as i32;
    let mac_address = r.read_string("login.mac")?;
    let auth_key_2 = r.read_string("login.authkey2")?;
    Ok(GameLoginRequest {
        id,
        uid,
        nt_key,
        command,
        auth_key,
        client_version,
        packet_version,
        mac_address,
        auth_key_2,
    })
}

fn parse_chat(r: &mut PayloadReader) -> Result<ChatRequest, ProtoError> {
    let nickname = r.read_string("chat.nick")?;
    let message = r.read_string("chat.msg")?;
    Ok(ChatRequest { nickname, message })
}

fn parse_enter_channel(r: &mut PayloadReader) -> Result<EnterChannelRequest, ProtoError> {
    let channel_id = r.read_u8("enter_channel.id")?;
    Ok(EnterChannelRequest { channel_id })
}

fn parse_make_room(r: &mut PayloadReader) -> Result<MakeRoomRequest, ProtoError> {
    let option = r.read_u8("make_room.option")?;
    let time_vs = r.read_u32("make_room.time_vs")?;
    let time_30s = r.read_u32("make_room.time_30s")?;
    let max_player = r.read_u8("make_room.max_player")?;
    let tipo = r.read_u8("make_room.tipo")?;
    let qntd_hole = r.read_u8("make_room.qntd_hole")?;
    let course = r.read_u8("make_room.course")?;
    let modo = r.read_u8("make_room.modo")?;

    // M_REPEAT (modo 4 in the C++ RoomInfo::M_REPEAT) carries extra fields.
    const M_REPEAT: u8 = 4;
    let (hole_repeat, fixed_hole) = if modo == M_REPEAT {
        (
            Some(r.read_u8("make_room.hole_repeat")?),
            Some(r.read_u32("make_room.fixed_hole")?),
        )
    } else {
        (None, None)
    };

    let natural = r.read_u32("make_room.natural")?;
    let name = r.read_string("make_room.name")?;

    Ok(MakeRoomRequest {
        option,
        time_vs,
        time_30s,
        max_player,
        tipo,
        qntd_hole,
        course,
        modo,
        hole_repeat,
        fixed_hole,
        natural,
        name,
    })
}

fn parse_enter_room(r: &mut PayloadReader) -> Result<EnterRoomRequest, ProtoError> {
    let room_numero = r.read_u16("enter_room.numero")? as i16;
    let password = r.read_string("enter_room.password")?;
    Ok(EnterRoomRequest {
        room_numero,
        password,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn str_bytes(s: &str) -> Vec<u8> {
        let mut v = (s.len() as u16).to_le_bytes().to_vec();
        v.extend_from_slice(s.as_bytes());
        v
    }

    fn login_body() -> Vec<u8> {
        let mut body = vec![0x02, 0x00]; // opcode Login
        body.extend(str_bytes("tester"));
        body.extend(&12345u32.to_le_bytes()); // uid
        body.extend(&0u32.to_le_bytes()); // ntKey
        body.extend(&0u16.to_le_bytes()); // command
        body.extend(str_bytes("ABCDEF12")); // authKey
        body.extend(str_bytes("SS.R7.995.00")); // client version
        body.extend(&0u32.to_le_bytes()); // packet version
        body.extend(str_bytes("AA:BB:CC")); // mac
        body.extend(str_bytes("ABCDEF12")); // authKey2
        body
    }

    #[test]
    fn parses_game_login() {
        let pkt = GamePacket::parse(&login_body()).unwrap();
        match pkt {
            GamePacket::Login(req) => {
                assert_eq!(req.id, "tester");
                assert_eq!(req.uid, 12345);
                assert_eq!(req.auth_key, "ABCDEF12");
                assert_eq!(req.client_version, "SS.R7.995.00");
                assert_eq!(req.mac_address, "AA:BB:CC");
            }
            other => panic!("expected Login, got {other:?}"),
        }
    }

    #[test]
    fn parses_chat() {
        let mut body = vec![0x03, 0x00];
        body.extend(str_bytes("player"));
        body.extend(str_bytes("hello lobby"));
        let pkt = GamePacket::parse(&body).unwrap();
        match pkt {
            GamePacket::Chat(req) => {
                assert_eq!(req.nickname, "player");
                assert_eq!(req.message, "hello lobby");
            }
            other => panic!("expected Chat, got {other:?}"),
        }
    }

    #[test]
    fn parses_enter_channel() {
        let body = vec![0x04, 0x00, 0x02];
        let pkt = GamePacket::parse(&body).unwrap();
        assert!(matches!(
            pkt,
            GamePacket::EnterChannel(EnterChannelRequest { channel_id: 2 })
        ));
    }

    #[test]
    fn unknown_opcode_preserved() {
        let body = vec![0xFE, 0x01, 0x99];
        let pkt = GamePacket::parse(&body).unwrap();
        assert!(matches!(pkt, GamePacket::Unknown(_)));
    }

    #[test]
    fn parses_make_room() {
        let mut body = vec![0x08, 0x00]; // opcode MakeRoom
        body.push(0); // option
        body.extend(&100u32.to_le_bytes()); // time_vs
        body.extend(&30u32.to_le_bytes()); // time_30s
        body.push(4); // max_player
        body.push(0); // tipo
        body.push(18); // qntd_hole
        body.push(0); // course (Blue Lagoon)
        body.push(0); // modo (not repeat)
        body.extend(&0u32.to_le_bytes()); // natural
        body.extend(str_bytes("My Room"));
        let pkt = GamePacket::parse(&body).unwrap();
        match pkt {
            GamePacket::MakeRoom(req) => {
                assert_eq!(req.name, "My Room");
                assert_eq!(req.max_player, 4);
                assert_eq!(req.qntd_hole, 18);
                assert!(req.hole_repeat.is_none());
            }
            other => panic!("expected MakeRoom, got {other:?}"),
        }
    }

    #[test]
    fn parses_leave_room() {
        let body = vec![0x0A, 0x00];
        assert!(matches!(
            GamePacket::parse(&body).unwrap(),
            GamePacket::LeaveRoom
        ));
    }
}
