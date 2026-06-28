//! Packet hex logging — mirrors the Pangya GM API format so captured output
//! can be diffed directly against the live C++ server's `packets` endpoint.
//!
//! Every decoded (post-decrypt) frame is logged at INFO with:
//!   `[PKT] dir=DIRECTION opcode=0xNNNN size=N hex=<payload>`
//!
//! The `hex` is the payload **after** the 2-byte opcode (matching the API's
//! `hex` field), and `size` is the full plaintext length.

use tracing::info;

/// Direction of a packet relative to the server.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Dir {
    /// Client → Server (received).
    C2S,
    /// Server → Client (sent).
    S2C,
}

impl Dir {
    fn as_str(self) -> &'static str {
        match self {
            Dir::C2S => "C2S",
            Dir::S2C => "S2C",
        }
    }
}

/// Log a decoded frame body in the API-compatible format.
///
/// `body` is the full plaintext (opcode + payload), exactly as produced by
/// the framing decoder or the response builder.
pub fn log_packet(dir: Dir, srv: &str, body: &[u8]) {
    if body.len() < 2 {
        info!(
            "[PKT] dir={} srv={} size={} (too short to parse opcode)",
            dir.as_str(),
            srv,
            body.len()
        );
        return;
    }
    let opcode = u16::from_le_bytes([body[0], body[1]]);
    let payload = &body[2..];
    let hex = hex::encode(payload);
    info!(
        "[PKT] dir={} srv={} opcode=0x{:04X} size={} hex={}",
        dir.as_str(),
        srv,
        opcode,
        body.len(),
        if hex.len() > 128 {
            format!("{}...(truncated, {} bytes)", &hex[..128], payload.len())
        } else {
            hex
        }
    );
}
