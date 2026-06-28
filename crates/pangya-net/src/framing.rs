//! Pangya packet framing — encodes/decodes the three on-wire formats, tying
//! together [`crate::crypt`], [`crate::compress`], and [`crate::size_codec`].
//!
//! ## The three formats
//!
//! All three share a header whose first byte is `low_key` (the per-packet key
//! selector) followed by a little-endian `u16` payload length named `size`.
//! `size` counts the bytes **after** the header.
//!
//! | Format | Header | Header len | Body | Used for |
//! |--------|--------|-----------|------|----------|
//! | `Server` | `low_key`, `size` | 3 | compressed + encrypted | server → client |
//! | `Client` | `low_key`, `size`, `seq` | 4 | encrypted only | client → server (and server→client when emulating a client) |
//! | `Raw` | `low_key=0`, `size` | 3 | plaintext + a leading `0x00` marker | the first key-exchange packet |
//!
//! ### Server body layout (encrypted)
//! ```text
//! [derived_key=1B][size encoded as 4 base-255 bytes][LZO-compressed plaintext]
//! ```
//! The whole body is XOR-encrypted as one block. The first decrypted byte must
//! equal the cipher's check byte.
//!
//! ### Client body layout (encrypted)
//! ```text
//! [derived_key=1B][plaintext]
//! ```
//! `size = plaintext_len + 1` (the +1 is the derived-key byte). Encrypted only,
//! no compression. `seq` is always 0 on send.
//!
//! ### Raw body layout (plaintext)
//! ```text
//! [0x00 marker][plaintext]
//! ```
//! `low_key = 0` identifies a raw packet; `size = plaintext_len + 1`.

use crate::compress;
use crate::crypt::{Crypt, CryptError};
use crate::size_codec;

use thiserror::Error;

/// On-wire header length for each format.
pub const SERVER_HEADER_LEN: usize = 3;
pub const CLIENT_HEADER_LEN: usize = 4;

#[derive(Debug, Error)]
pub enum FrameError {
    #[error("packet too short for a header ({got} bytes)")]
    ShortHeader { got: usize },
    #[error("declared payload size {declared} exceeds available bytes {available}")]
    Truncated { declared: usize, available: usize },
    #[error("raw packet marker byte was {0:#x}, expected 0x00")]
    BadRawMarker(u8),
    #[error("decrypt failed: {0}")]
    Decrypt(#[from] CryptError),
    #[error("decompress failed: {0}")]
    Decompress(#[from] compress::CompressError),
    #[error("empty plaintext body")]
    EmptyBody,
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}

// ─────────────────────────────────────────────────────────────────────────────
// Encode
// ─────────────────────────────────────────────────────────────────────────────

/// A connection's session key (the 0..=15 high nibble used to derive the cipher).
#[derive(Debug, Clone, Copy)]
pub struct SessionKey(pub u8);

/// Encode a plaintext body into the **server** wire format (compress + encrypt).
///
/// `session_key` is the connection's high key (0..=15). The header's `low_key`
/// is chosen randomly per packet, exactly as the original (`rand() & 255`).
/// Returns the full frame: header + encrypted body.
pub fn encode_server(
    plain: &[u8],
    session_key: SessionKey,
    low_key: u8,
    out: &mut Vec<u8>,
) -> Result<(), FrameError> {
    if plain.is_empty() {
        return Err(FrameError::EmptyBody);
    }

    // LZO-compress the plaintext body.
    let compressed = compress::compress(plain)?;

    // Body = [derived_key][4 size bytes][compressed...]
    let body_len = 1 + 4 + compressed.len();
    debug_assert!(
        body_len <= u16::MAX as usize,
        "server body overflows u16 size"
    );

    let cipher = Crypt::new(session_key.0, low_key);

    // Build the plaintext body the cipher will encrypt. The first byte must be
    // the check byte (k1) for the integrity check to pass.
    let mut body_plain = Vec::with_capacity(body_len);
    body_plain.push(cipher.check_byte());
    body_plain.extend_from_slice(&size_codec::encode_size(plain.len() as u32));
    body_plain.extend_from_slice(&compressed);

    // Encrypt the body in place.
    let mut body_cipher = vec![0u8; body_plain.len()];
    cipher.encrypt(&body_plain, &mut body_cipher)?;

    // Header: [low_key][size as u16 LE]. size = body length.
    out.push(low_key);
    out.extend_from_slice(&(body_len as u16).to_le_bytes());
    out.extend_from_slice(&body_cipher);
    Ok(())
}

/// Encode a plaintext body into the **client** wire format (encrypt only).
///
/// `seq` is normally 0. The body is `[derived_key][plain]`, encrypted as one
/// block.
pub fn encode_client(
    plain: &[u8],
    session_key: SessionKey,
    low_key: u8,
    seq: u8,
    out: &mut Vec<u8>,
) -> Result<(), FrameError> {
    if plain.is_empty() {
        return Err(FrameError::EmptyBody);
    }

    let cipher = Crypt::new(session_key.0, low_key);

    // Plaintext body = [check_byte][plain...]; size = plain.len() + 1.
    let mut body_plain = Vec::with_capacity(plain.len() + 1);
    body_plain.push(cipher.check_byte());
    body_plain.extend_from_slice(plain);

    let mut body_cipher = vec![0u8; body_plain.len()];
    cipher.encrypt(&body_plain, &mut body_cipher)?;

    // Header: [low_key][size LE][seq]. size = body length.
    let size = body_plain.len() as u16;
    out.push(low_key);
    out.extend_from_slice(&size.to_le_bytes());
    out.push(seq);
    out.extend_from_slice(&body_cipher);
    Ok(())
}

/// Encode a plaintext body into the **raw** wire format (plaintext, no crypto).
///
/// Used for the very first key-exchange packet. The body is `[0x00 marker][plain]`.
pub fn encode_raw(plain: &[u8], out: &mut Vec<u8>) -> Result<(), FrameError> {
    if plain.is_empty() {
        return Err(FrameError::EmptyBody);
    }
    let size = (plain.len() + 1) as u16;
    // Header: low_key = 0, size LE.
    out.push(0);
    out.extend_from_slice(&size.to_le_bytes());
    out.push(0x00); // raw marker
    out.extend_from_slice(plain);
    Ok(())
}

// ─────────────────────────────────────────────────────────────────────────────
// Decode
// ─────────────────────────────────────────────────────────────────────────────

/// A decoded frame: the decrypted (and, for server format, decompressed)
/// plaintext body, plus the `low_key`/`seq` from the header.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DecodedFrame {
    pub low_key: u8,
    pub seq: Option<u8>,
    pub body: Vec<u8>,
}

/// Decode a **server**-format frame (the full frame including the 3-byte header).
///
/// Detects raw frames (`low_key == 0` and a `0x00` marker byte) and decodes
/// them via the raw path, matching `packet::unMakeFull`.
pub fn decode_server(frame: &[u8], session_key: SessionKey) -> Result<DecodedFrame, FrameError> {
    if frame.len() < SERVER_HEADER_LEN {
        return Err(FrameError::ShortHeader { got: frame.len() });
    }
    let low_key = frame[0];
    let size = u16::from_le_bytes([frame[1], frame[2]]) as usize;
    let header_len = SERVER_HEADER_LEN;

    if frame.len() < header_len + size {
        return Err(FrameError::Truncated {
            declared: size,
            available: frame.len() - header_len,
        });
    }
    let body = &frame[header_len..header_len + size];

    // Raw detection: low_key == 0 and the first body byte is the 0x00 marker.
    if low_key == 0 && !body.is_empty() && body[0] == 0x00 {
        return Ok(DecodedFrame {
            low_key,
            seq: None,
            body: body[1..].to_vec(),
        });
    }

    // Encrypted+compressed path.
    let cipher = Crypt::new(session_key.0, low_key);
    let mut decrypted = vec![0u8; body.len()];
    cipher.decrypt(body, &mut decrypted)?;

    // After decrypt, byte 0 was the check key (skip it). Bytes 1..5 are the
    // encoded decompressed size; the rest is LZO-compressed plaintext.
    let enc_size_bytes: [u8; 4] = decrypted[1..5].try_into().unwrap();
    let decompressed_cap = size_codec::decode_alloc_size(enc_size_bytes);

    let mut plain = vec![0u8; decompressed_cap as usize];
    let n = compress::decompress(&decrypted[5..], &mut plain)?;
    plain.truncate(n);

    Ok(DecodedFrame {
        low_key,
        seq: None,
        body: plain,
    })
}

/// Decode a **client**-format frame (full frame including the 4-byte header).
pub fn decode_client(frame: &[u8], session_key: SessionKey) -> Result<DecodedFrame, FrameError> {
    if frame.len() < CLIENT_HEADER_LEN {
        return Err(FrameError::ShortHeader { got: frame.len() });
    }
    let low_key = frame[0];
    let size = u16::from_le_bytes([frame[1], frame[2]]) as usize;
    let seq = frame[3];
    let header_len = CLIENT_HEADER_LEN;

    if frame.len() < header_len + size {
        return Err(FrameError::Truncated {
            declared: size,
            available: frame.len() - header_len,
        });
    }
    let body = &frame[header_len..header_len + size];

    let cipher = Crypt::new(session_key.0, low_key);
    let mut decrypted = vec![0u8; body.len()];
    cipher.decrypt(body, &mut decrypted)?;

    // Byte 0 was the check key; the rest is plaintext.
    Ok(DecodedFrame {
        low_key,
        seq: Some(seq),
        body: decrypted[1..].to_vec(),
    })
}

/// Decode a **raw** frame (full frame including the 3-byte header).
pub fn decode_raw(frame: &[u8]) -> Result<DecodedFrame, FrameError> {
    if frame.len() < SERVER_HEADER_LEN {
        return Err(FrameError::ShortHeader { got: frame.len() });
    }
    let low_key = frame[0];
    let size = u16::from_le_bytes([frame[1], frame[2]]) as usize;
    let header_len = SERVER_HEADER_LEN;

    if frame.len() < header_len + size {
        return Err(FrameError::Truncated {
            declared: size,
            available: frame.len() - header_len,
        });
    }
    let body = &frame[header_len..header_len + size];
    if body.is_empty() {
        return Err(FrameError::BadRawMarker(0));
    }
    if body[0] != 0x00 {
        return Err(FrameError::BadRawMarker(body[0]));
    }
    Ok(DecodedFrame {
        low_key,
        seq: None,
        body: body[1..].to_vec(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const SK: SessionKey = SessionKey(7);

    #[test]
    fn server_format_round_trip() {
        let plain = b"\x00\x01Hello, Pangya server format!\xff\xfe".to_vec();
        let mut frame = Vec::new();
        encode_server(&plain, SK, 42, &mut frame).unwrap();

        let decoded = decode_server(&frame, SK).unwrap();
        assert_eq!(decoded.body, plain);
        assert_eq!(decoded.low_key, 42);
        assert!(decoded.seq.is_none());
    }

    #[test]
    fn client_format_round_trip() {
        let plain = b"\x00\x01client-format payload\x02\x03".to_vec();
        let mut frame = Vec::new();
        encode_client(&plain, SK, 17, 0, &mut frame).unwrap();

        let decoded = decode_client(&frame, SK).unwrap();
        assert_eq!(decoded.body, plain);
        assert_eq!(decoded.low_key, 17);
        assert_eq!(decoded.seq, Some(0));
    }

    #[test]
    fn raw_format_round_trip() {
        let plain = b"\x02\x00\x1bkey exchange data".to_vec();
        let mut frame = Vec::new();
        encode_raw(&plain, &mut frame).unwrap();

        let decoded = decode_raw(&frame).unwrap();
        assert_eq!(decoded.body, plain);
        assert_eq!(decoded.low_key, 0);

        // unMakeFull-style raw detection also works via decode_server.
        let via_server = decode_server(&frame, SK).unwrap();
        assert_eq!(via_server.body, plain);
    }

    #[test]
    fn wrong_session_key_fails_to_decrypt() {
        let plain = b"secret payload data goes here".to_vec();
        let mut frame = Vec::new();
        encode_server(&plain, SK, 5, &mut frame).unwrap();

        let err = decode_server(&frame, SessionKey(8)).unwrap_err();
        assert!(matches!(err, FrameError::Decrypt(_)), "got {err:?}");
    }

    #[test]
    fn truncated_frame_is_detected() {
        let plain = b"some payload".repeat(10);
        let mut frame = Vec::new();
        encode_server(&plain, SK, 99, &mut frame).unwrap();

        // Truncate the body.
        let truncated = &frame[..frame.len() - 5];
        let err = decode_server(truncated, SK).unwrap_err();
        assert!(matches!(err, FrameError::Truncated { .. }), "got {err:?}");
    }

    #[test]
    fn empty_body_is_rejected_on_encode() {
        let mut frame = Vec::new();
        assert!(matches!(
            encode_server(&[], SK, 1, &mut frame),
            Err(FrameError::EmptyBody)
        ));
    }

    #[test]
    fn large_payload_round_trips() {
        // Exercises multi-block crypto chaining and a sizable LZO payload.
        let plain: Vec<u8> = (0u32..4096).map(|i| (i.wrapping_mul(31)) as u8).collect();
        let mut frame = Vec::new();
        encode_server(&plain, SK, 200, &mut frame).unwrap();
        let decoded = decode_server(&frame, SK).unwrap();
        assert_eq!(decoded.body, plain);
    }
}
