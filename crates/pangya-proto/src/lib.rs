//! Typed Pangya wire protocol: opcode enums, request/response structs, and the
//! dispatch layer.
//!
//! Each decoded packet frame (from [`pangya_net::framing::DecodedFrame`]) begins
//! with a little-endian `u16` **opcode** (`tipo` in the C++); the rest is the
//! payload. This crate turns that opcode + payload into a typed enum variant and
//! provides a clean place for handlers to live.
//!
//! ## Design
//!
//! - [`Opcode`] is `#[non_exhaustive]`: every opcode the C++ registered gets a
//!   named variant. Unknown opcodes become [`Opcode::Unknown`] instead of
//!   panicking (the C++ logged them).
//! - Each server scope has its own packet enum (e.g. [`LoginPacket`]) so the
//!   compiler checks that every handler matches its payload type — replacing the
//!   C++ 10,000-entry `void*` dispatch table.
//! - Wire structs use `binrw` with little-endian reads to mirror the C++
//!   `readUint32`/`readString`/etc. helpers.
//!
//! Only the opcodes needed for Milestone 1 (Login + Game lobby) are modelled
//! now; the enum grows as systems are ported.

pub mod game;
pub mod game_resp;
pub mod login;
pub mod login_resp;

pub use game::{
    ChatRequest, EnterChannelRequest, EnterRoomRequest, GameLoginRequest, GamePacket,
    MakeRoomRequest,
};
pub use game_resp::ChannelInfoWire;
pub use login::{LoginOpcode, LoginPacket, LoginRequest, PayloadReader, ProtoError};
pub use login_resp::ServerInfoWire;

/// Read the little-endian `u16` opcode from the start of a decoded frame body.
///
/// Returns the opcode and the remaining payload bytes.
pub fn split_opcode(body: &[u8]) -> Option<(u16, &[u8])> {
    if body.len() < 2 {
        return None;
    }
    let opcode = u16::from_le_bytes([body[0], body[1]]);
    Some((opcode, &body[2..]))
}

/// Write a little-endian `u16` opcode as the prefix of a new outgoing buffer.
pub fn write_opcode(opcode: u16, out: &mut Vec<u8>) {
    out.extend_from_slice(&opcode.to_le_bytes());
}

/// A raw, unrecognised opcode (the body is preserved verbatim).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnknownPacket {
    pub opcode: u16,
    pub body: Vec<u8>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_opcode_le() {
        let body = [0x01, 0x00, 0xAA, 0xBB];
        let (op, rest) = split_opcode(&body).unwrap();
        assert_eq!(op, 0x0001);
        assert_eq!(rest, &[0xAA, 0xBB]);
    }

    #[test]
    fn rejects_too_short_body() {
        assert!(split_opcode(&[0x01]).is_none());
    }

    #[test]
    fn writes_opcode_le() {
        let mut out = Vec::new();
        write_opcode(0x0B, &mut out);
        assert_eq!(out, vec![0x0B, 0x00]);
    }
}
