//! Pangya wire protocol: crypt, LZO compression, packet framing, and the tokio
//! codec.
//!
//! Everything here is a bit-exact port of the C++ `Projeto IOCP` core:
//! - [`crypt`]: the XOR packet cipher (4-byte-stride chaining, single-byte key
//!   from an 8192-entry dictionary).
//! - `compress`: LZO1X (Phase 1, pending).
//! - `framing`/`codec`: the 3 wire formats (server/client/raw) as a tokio
//!   `Encoder`/`Decoder` (Phase 1, pending).

pub mod codec;
pub mod compress;
pub mod crypt;
pub mod framing;
mod key_dictionary;
pub mod size_codec;
