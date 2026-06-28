//! Login Server packet opcodes and typed request/response structs.
//!
//! Opcodes are taken from the C++ registration in
//! `Login Server/Login Server/login_server.cpp:96-102`:
//!
//! | opcode | C++ handler | meaning |
//! |--------|-------------|---------|
//! | `0x01` | `packet001` | Login (id, password, options, mac) |
//! | `0x03` | `packet003` | Select game server (server uid + auth key request) |
//! | `0x04` | `packet004` | Notify player went down on a game server |
//! | `0x06` | `packet006` | Save nickname |
//! | `0x07` | `packet007` | Check nickname availability |
//! | `0x08` | `packet008` | Create first character (typeid, default hair/shirts) |
//! | `0x0B` | `packet00B` | Re-login |
//!
//! Strings on the wire are `i16` length-prefix + bytes (see `packet::readString`).

use crate::{split_opcode, UnknownPacket};
use thiserror::Error;

/// All Login Server client→server opcodes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u16)]
#[non_exhaustive]
pub enum LoginOpcode {
    Login = 0x01,
    SelectServer = 0x03,
    DownPlayerOnGameServer = 0x04,
    SaveNickname = 0x06,
    CheckNickname = 0x07,
    CreateFirstCharacter = 0x08,
    ReLogin = 0x0B,
}

impl LoginOpcode {
    /// Map a raw opcode to a typed variant, or `None` if unrecognised.
    pub fn from_raw(raw: u16) -> Option<Self> {
        Some(match raw {
            0x01 => Self::Login,
            0x03 => Self::SelectServer,
            0x04 => Self::DownPlayerOnGameServer,
            0x06 => Self::SaveNickname,
            0x07 => Self::CheckNickname,
            0x08 => Self::CreateFirstCharacter,
            0x0B => Self::ReLogin,
            _ => return None,
        })
    }
}

/// A parsed Login Server packet: a typed request, or an [`UnknownPacket`] for
/// opcodes this server doesn't handle.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LoginPacket {
    Login(LoginRequest),
    SelectServer(SelectServerRequest),
    DownPlayerOnGameServer,
    SaveNickname(SaveNicknameRequest),
    CheckNickname(CheckNicknameRequest),
    CreateFirstCharacter(CreateFirstCharacterRequest),
    ReLogin(LoginRequest),
    Unknown(UnknownPacket),
}

impl LoginPacket {
    /// Parse a decoded frame body (opcode prefix + payload) into a typed packet.
    pub fn parse(body: &[u8]) -> Result<Self, ProtoError> {
        let (raw, payload) = split_opcode(body).ok_or(ProtoError::EmptyBody)?;
        let opcode = LoginOpcode::from_raw(raw);

        let mut reader = PayloadReader::new(payload);

        Ok(match opcode {
            Some(LoginOpcode::Login) => parse_login(&mut reader)?,
            Some(LoginOpcode::ReLogin) => match parse_login(&mut reader)? {
                LoginPacket::Login(req) => LoginPacket::ReLogin(req),
                other => other,
            },
            Some(LoginOpcode::SelectServer) => parse_select_server(&mut reader)?,
            Some(LoginOpcode::DownPlayerOnGameServer) => LoginPacket::DownPlayerOnGameServer,
            Some(LoginOpcode::SaveNickname) => parse_save_nick(&mut reader)?,
            Some(LoginOpcode::CheckNickname) => parse_check_nick(&mut reader)?,
            Some(LoginOpcode::CreateFirstCharacter) => parse_create_char(&mut reader)?,
            None => LoginPacket::Unknown(UnknownPacket {
                opcode: raw,
                body: payload.to_vec(),
            }),
        })
    }
}

// ── request structs ───────────────────────────────────────────────────────────

/// `0x01` / `0x0B` — Login (and re-login) request.
///
/// Layout: `id`, `password`, `opt_count: u8`, `opt_count × u64`, `mac_address`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoginRequest {
    pub id: String,
    pub password: String,
    /// Opaque client option flags (passed through verbatim).
    pub options: Vec<u64>,
    pub mac_address: String,
}

/// `0x03` — Select game server: a server UID to connect to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SelectServerRequest {
    pub server_uid: u32,
}

/// `0x06` — Save nickname (the nickname is UTF-8, originally wide-string).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SaveNicknameRequest {
    pub nickname: String,
}

/// `0x07` — Check whether a nickname is available.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CheckNicknameRequest {
    pub nickname: String,
}

/// `0x08` — Create the player's first character.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CreateFirstCharacterRequest {
    pub typeid: u32,
    pub default_hair: u8,
    pub default_shirts: u8,
}

// ── payload reader + parsers ──────────────────────────────────────────────────

#[derive(Debug, Error)]
pub enum ProtoError {
    #[error("decoded body had no opcode")]
    EmptyBody,
    #[error("payload truncated: {0}")]
    Truncated(&'static str),
    #[error("invalid string: {0}")]
    InvalidString(String),
}

/// A tiny big-endian-free reader over a borrowed payload slice. Mirrors the C++
/// `packet` read cursor (`readUint32`, `readString`, …), all little-endian.
pub struct PayloadReader<'a> {
    buf: &'a [u8],
    pos: usize,
}

impl<'a> PayloadReader<'a> {
    pub fn new(buf: &'a [u8]) -> Self {
        Self { buf, pos: 0 }
    }

    fn need(&self, n: usize, ctx: &'static str) -> Result<(), ProtoError> {
        if self.pos + n > self.buf.len() {
            return Err(ProtoError::Truncated(ctx));
        }
        Ok(())
    }

    pub fn read_u8(&mut self, ctx: &'static str) -> Result<u8, ProtoError> {
        self.need(1, ctx)?;
        let v = self.buf[self.pos];
        self.pos += 1;
        Ok(v)
    }

    pub fn read_u16(&mut self, ctx: &'static str) -> Result<u16, ProtoError> {
        self.need(2, ctx)?;
        let v = u16::from_le_bytes([self.buf[self.pos], self.buf[self.pos + 1]]);
        self.pos += 2;
        Ok(v)
    }

    pub fn read_u32(&mut self, ctx: &'static str) -> Result<u32, ProtoError> {
        self.need(4, ctx)?;
        let v = u32::from_le_bytes(self.buf[self.pos..self.pos + 4].try_into().unwrap());
        self.pos += 4;
        Ok(v)
    }

    pub fn read_u64(&mut self, ctx: &'static str) -> Result<u64, ProtoError> {
        self.need(8, ctx)?;
        let v = u64::from_le_bytes(self.buf[self.pos..self.pos + 8].try_into().unwrap());
        self.pos += 8;
        Ok(v)
    }

    /// Read an `i16`-length-prefixed string, matching `packet::readString`.
    /// A length ≤ 0 yields an empty string.
    ///
    /// The wire bytes are decoded **lossily**: the Pangya client sends raw bytes
    /// in its locale encoding (Shift-JIS for JP), which are not valid UTF-8.
    /// Strict UTF-8 decoding would reject room names / chat from non-ASCII
    /// clients, so invalid byte sequences are replaced with the replacement
    /// char (`�`) rather than erroring. Use [`Self::read_raw_string`] for fields
    /// that must round-trip the exact bytes back to the wire (e.g. room names).
    pub fn read_string(&mut self, ctx: &'static str) -> Result<String, ProtoError> {
        let len = self.read_u16(ctx)? as i16;
        if len <= 0 {
            return Ok(String::new());
        }
        let len = len as usize;
        self.need(len, ctx)?;
        let bytes = &self.buf[self.pos..self.pos + len];
        self.pos += len;
        Ok(String::from_utf8_lossy(bytes).into_owned())
    }

    /// Read an `i16`-length-prefixed string as **raw bytes**, preserving the
    /// exact wire content. Use this for fields that are forwarded back to the
    /// client verbatim (e.g. room names), where lossy UTF-8 decoding would
    /// destroy Shift-JIS bytes irreversibly.
    pub fn read_raw_string(&mut self, ctx: &'static str) -> Result<Vec<u8>, ProtoError> {
        let len = self.read_u16(ctx)? as i16;
        if len <= 0 {
            return Ok(Vec::new());
        }
        let len = len as usize;
        self.need(len, ctx)?;
        let bytes = self.buf[self.pos..self.pos + len].to_vec();
        self.pos += len;
        Ok(bytes)
    }
}

fn parse_login(r: &mut PayloadReader) -> Result<LoginPacket, ProtoError> {
    let id = r.read_string("login.id")?;
    let password = r.read_string("login.password")?;
    let opt_count = r.read_u8("login.opt_count")?;
    let mut options = Vec::with_capacity(opt_count as usize);
    for _ in 0..opt_count {
        options.push(r.read_u64("login.options")?);
    }
    let mac_address = r.read_string("login.mac")?;
    Ok(LoginPacket::Login(LoginRequest {
        id,
        password,
        options,
        mac_address,
    }))
}

fn parse_select_server(r: &mut PayloadReader) -> Result<LoginPacket, ProtoError> {
    let server_uid = r.read_u32("select_server.uid")?;
    Ok(LoginPacket::SelectServer(SelectServerRequest {
        server_uid,
    }))
}

fn parse_save_nick(r: &mut PayloadReader) -> Result<LoginPacket, ProtoError> {
    let nickname = r.read_string("save_nick.name")?;
    Ok(LoginPacket::SaveNickname(SaveNicknameRequest { nickname }))
}

fn parse_check_nick(r: &mut PayloadReader) -> Result<LoginPacket, ProtoError> {
    let nickname = r.read_string("check_nick.name")?;
    Ok(LoginPacket::CheckNickname(CheckNicknameRequest {
        nickname,
    }))
}

fn parse_create_char(r: &mut PayloadReader) -> Result<LoginPacket, ProtoError> {
    let typeid = r.read_u32("create_char.typeid")?;
    let default_hair = r.read_u8("create_char.hair")?;
    let default_shirts = r.read_u8("create_char.shirts")?;
    Ok(LoginPacket::CreateFirstCharacter(
        CreateFirstCharacterRequest {
            typeid,
            default_hair,
            default_shirts,
        },
    ))
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
        let mut body = vec![0x01, 0x00]; // opcode Login
        body.extend(str_bytes("tester"));
        body.extend(str_bytes("secret"));
        body.push(2); // opt_count
        body.extend(
            &[0u64, 1u64]
                .iter()
                .flat_map(|x| x.to_le_bytes())
                .collect::<Vec<_>>(),
        );
        body.extend(str_bytes("AA:BB:CC:DD:EE:FF"));
        body
    }

    #[test]
    fn parses_login_request() {
        let pkt = LoginPacket::parse(&login_body()).unwrap();
        match pkt {
            LoginPacket::Login(req) => {
                assert_eq!(req.id, "tester");
                assert_eq!(req.password, "secret");
                assert_eq!(req.options, vec![0u64, 1u64]);
                assert_eq!(req.mac_address, "AA:BB:CC:DD:EE:FF");
            }
            other => panic!("expected Login, got {other:?}"),
        }
    }

    #[test]
    fn parses_select_server() {
        let mut body = vec![0x03, 0x00]; // opcode SelectServer
        body.extend(&20203u32.to_le_bytes());
        let pkt = LoginPacket::parse(&body).unwrap();
        match pkt {
            LoginPacket::SelectServer(req) => assert_eq!(req.server_uid, 20203),
            other => panic!("expected SelectServer, got {other:?}"),
        }
    }

    #[test]
    fn parses_create_first_character() {
        let mut body = vec![0x08, 0x00]; // opcode CreateFirstCharacter
        body.extend(&0x4D000001u32.to_le_bytes());
        body.push(3); // default_hair
        body.push(5); // default_shirts
        let pkt = LoginPacket::parse(&body).unwrap();
        match pkt {
            LoginPacket::CreateFirstCharacter(req) => {
                assert_eq!(req.typeid, 0x4D000001);
                assert_eq!(req.default_hair, 3);
                assert_eq!(req.default_shirts, 5);
            }
            other => panic!("expected CreateFirstCharacter, got {other:?}"),
        }
    }

    #[test]
    fn unknown_opcode_is_preserved() {
        let body = vec![0xFF, 0xFF, 0xDE, 0xAD];
        let pkt = LoginPacket::parse(&body).unwrap();
        match pkt {
            LoginPacket::Unknown(u) => {
                assert_eq!(u.opcode, 0xFFFF);
                assert_eq!(u.body, vec![0xDE, 0xAD]);
            }
            other => panic!("expected Unknown, got {other:?}"),
        }
    }

    #[test]
    fn down_player_opcode_has_no_payload() {
        let body = vec![0x04, 0x00];
        assert!(matches!(
            LoginPacket::parse(&body).unwrap(),
            LoginPacket::DownPlayerOnGameServer
        ));
    }

    #[test]
    fn truncated_payload_is_an_error() {
        // A Login opcode with no payload at all.
        let body = vec![0x01, 0x00];
        assert!(matches!(
            LoginPacket::parse(&body),
            Err(ProtoError::Truncated(_))
        ));
    }
}
