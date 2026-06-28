//! tokio `Decoder` for the Pangya length-delimited wire format.
//!
//! Replaces the C++ `LOOP_TRANSLATE_BUFFER_TO_PACKET_SERVER` /
//! `LOOP_TRANSLATE_BUFFER_TO_PACKET_CLIENT` macros in `threadpool.h`, which
//! sliced complete frames out of the recv buffer using the 2-byte `size` field
//! and stashed any partial tail on the session for the next recv.
//!
//! The codec is parameterised by [`Format`] (server vs client header length) and
//! the connection's [`SessionKey`]. Each call to `decode` peels as many
//! complete frames as the buffer holds; an incomplete tail is left in the
//! buffer for the next read, exactly as the original did.

use bytes::BytesMut;
use tokio_util::codec::Decoder;

use crate::framing::{
    decode_client, decode_raw, decode_server, DecodedFrame, FrameError, SessionKey,
    CLIENT_HEADER_LEN, SERVER_HEADER_LEN,
};

/// Which header shape this connection speaks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    /// 3-byte header, compress + encrypt. Server → client.
    Server,
    /// 4-byte header (extra `seq` byte), encrypt only. Client → server.
    Client,
}

impl Format {
    pub const fn header_len(self) -> usize {
        match self {
            Format::Server => SERVER_HEADER_LEN,
            Format::Client => CLIENT_HEADER_LEN,
        }
    }
}

/// A tokio codec that decodes the Pangya wire format into [`DecodedFrame`]s.
///
/// Raw frames (the initial key-exchange packet, `low_key == 0`) are detected
/// inside `decode_server` and handled transparently.
///
/// **Encoding** is intentionally not provided here: the original build path
/// chooses a random `low_key` per packet, which is better expressed by an
/// explicit `encode_*` call than a stateful `Encoder`. See [`crate::framing`].
pub struct PangyaDecoder {
    format: Format,
    session_key: SessionKey,
}

impl PangyaDecoder {
    pub fn new(format: Format, session_key: SessionKey) -> Self {
        Self {
            format,
            session_key,
        }
    }
}

impl Decoder for PangyaDecoder {
    type Item = DecodedFrame;
    type Error = FrameError;

    fn decode(&mut self, buf: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        let header_len = self.format.header_len();

        // Not enough bytes for a header yet — wait for more.
        if buf.len() < header_len {
            return Ok(None);
        }

        // Read the little-endian `size` field (header bytes 1..3).
        let size = u16::from_le_bytes([buf[1], buf[2]]) as usize;
        let frame_len = header_len + size;

        // The full frame isn't here yet — wait for more bytes. Reserve so the
        // buffer can hold it when the rest arrives.
        if buf.len() < frame_len {
            buf.reserve(frame_len);
            return Ok(None);
        }

        // The complete frame is available. Split it off the front and decode.
        let frame_bytes = buf.split_to(frame_len);

        match self.format {
            Format::Server => decode_server(&frame_bytes, self.session_key).map(Some),
            Format::Client => decode_client(&frame_bytes, self.session_key).map(Some),
        }
    }

    fn decode_eof(&mut self, buf: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        // First try the normal path in case a full frame is buffered.
        if let Some(frame) = self.decode(buf)? {
            return Ok(Some(frame));
        }
        if buf.is_empty() {
            return Ok(None);
        }
        // Trailing bytes that never formed a complete frame — a clean EOF after
        // a partial read. The original logged and dropped the connection; we
        // surface it as a truncation error.
        Err(FrameError::Truncated {
            declared: 0,
            available: buf.len(),
        })
    }
}

/// Convenience for callers that want to decode a raw key-exchange packet from a
/// freshly-accepted connection's first bytes (the original `unMakeRaw` path).
pub fn decode_raw_frame(buf: &[u8]) -> Result<DecodedFrame, FrameError> {
    decode_raw(buf)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::framing::{encode_client, encode_raw, encode_server};
    use bytes::BytesMut;

    const SK: SessionKey = SessionKey(7);

    #[tokio::test]
    async fn decodes_single_server_frame() {
        let plain = b"single server frame payload".to_vec();
        let mut wire = Vec::new();
        encode_server(&plain, SK, 42, &mut wire).unwrap();

        let mut decoder = PangyaDecoder::new(Format::Server, SK);
        let mut buf = BytesMut::from(&wire[..]);
        let frame = decoder.decode(&mut buf).unwrap().expect("a frame");
        assert_eq!(frame.body, plain);
        assert!(buf.is_empty(), "buffer should be drained");
    }

    #[tokio::test]
    async fn decodes_multiple_frames_in_one_buffer() {
        let p1 = b"first payload here".to_vec();
        let p2 = b"second payload, a bit longer".to_vec();
        let p3 = b"third".to_vec();
        let mut wire = Vec::new();
        encode_server(&p1, SK, 1, &mut wire).unwrap();
        encode_server(&p2, SK, 2, &mut wire).unwrap();
        encode_server(&p3, SK, 3, &mut wire).unwrap();

        let mut decoder = PangyaDecoder::new(Format::Server, SK);
        let mut buf = BytesMut::from(&wire[..]);

        let f1 = decoder.decode(&mut buf).unwrap().unwrap();
        let f2 = decoder.decode(&mut buf).unwrap().unwrap();
        let f3 = decoder.decode(&mut buf).unwrap().unwrap();
        assert_eq!(f1.body, p1);
        assert_eq!(f2.body, p2);
        assert_eq!(f3.body, p3);
        assert_eq!(decoder.decode(&mut buf).unwrap(), None);
    }

    #[tokio::test]
    async fn waits_for_partial_frame() {
        let plain = b"split-across-reads payload data".to_vec();
        let mut wire = Vec::new();
        encode_server(&plain, SK, 9, &mut wire).unwrap();

        let split = wire.len() / 2;
        let mut decoder = PangyaDecoder::new(Format::Server, SK);
        let mut buf = BytesMut::new();

        // First half: not a complete frame.
        buf.extend_from_slice(&wire[..split]);
        assert_eq!(decoder.decode(&mut buf).unwrap(), None);

        // Rest arrives.
        buf.extend_from_slice(&wire[split..]);
        let frame = decoder.decode(&mut buf).unwrap().unwrap();
        assert_eq!(frame.body, plain);
    }

    #[tokio::test]
    async fn waits_for_header_then_body() {
        // Feed the buffer one byte at a time to stress the length-check logic.
        let plain = b"byte-by-byte feed test payload!!".to_vec();
        let mut wire = Vec::new();
        encode_client(&plain, SK, 5, 0, &mut wire).unwrap();

        let mut decoder = PangyaDecoder::new(Format::Client, SK);
        let mut buf = BytesMut::new();
        let mut got_frame: Option<DecodedFrame> = None;
        for i in 0..wire.len() {
            buf.extend_from_slice(&wire[i..i + 1]);
            let result = decoder.decode(&mut buf).unwrap();
            if i + 1 < wire.len() {
                assert!(result.is_none(), "should wait at byte {}", i);
            } else {
                got_frame = result; // complete on the final byte
            }
        }
        let frame = got_frame.expect("frame decoded on final byte");
        assert_eq!(frame.body, plain);
        assert_eq!(frame.seq, Some(0));
        // Buffer is now drained.
        assert_eq!(decoder.decode(&mut buf).unwrap(), None);
    }

    #[tokio::test]
    async fn handles_raw_frame_on_server_decoder() {
        // The first packet on a connection is raw; decode_server detects it.
        let plain = b"\x02\x00raw handshake".to_vec();
        let mut wire = Vec::new();
        encode_raw(&plain, &mut wire).unwrap();

        let mut decoder = PangyaDecoder::new(Format::Server, SK);
        let mut buf = BytesMut::from(&wire[..]);
        let frame = decoder.decode(&mut buf).unwrap().unwrap();
        assert_eq!(frame.body, plain);
    }

    #[tokio::test]
    async fn eof_with_trailing_bytes_is_an_error() {
        let plain = b"payload".to_vec();
        let mut wire = Vec::new();
        encode_server(&plain, SK, 1, &mut wire).unwrap();
        // Truncate so we have a header but an incomplete body.
        let truncated = &wire[..wire.len() - 3];

        let mut decoder = PangyaDecoder::new(Format::Server, SK);
        let mut buf = BytesMut::from(truncated);
        let err = decoder.decode_eof(&mut buf).unwrap_err();
        assert!(matches!(err, FrameError::Truncated { .. }), "got {err:?}");
    }
}
