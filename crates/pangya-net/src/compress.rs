//! LZO1X compression — a thin wrapper over `lzokay` that mirrors the original
//! `Projeto IOCP/COMPRESS/compress.cpp` (which bound minilzo).
//!
//! The Pangya protocol always LZO1X-compresses the body of server-format
//! packets before encryption. The LZO1X byte format is standardized, so
//! `lzokay`'s pure-Rust output is interoperable with the real client (which
//! expects minilzo-compatible bytes). See `compress_data` / `decompress_data`
//! in the original.
//!
//! One behavioural note from the original: `compress.cpp` tolerates a one-byte
//! mismatch between the decompressed size and the value stored in the header
//! (`abs(actual - expected) != 1`); that tolerance lives in the framing layer,
//! not here.

use thiserror::Error;

#[derive(Debug, Error)]
pub enum CompressError {
    #[error("LZO1X compress failed")]
    Compress,
    #[error("LZO1X decompress failed: output buffer too small or corrupt input")]
    Decompress,
}

/// LZO1X-1 compress `src`. Returns the compressed bytes.
///
/// Mirrors `compress::compress_data(src, src_len, dst, &out_len)`.
pub fn compress(src: &[u8]) -> Result<Vec<u8>, CompressError> {
    lzokay::compress::compress(src).map_err(|_| CompressError::Compress)
}

/// LZO1X decompress `src` into the pre-sized `dst`. Returns the number of
/// bytes written.
///
/// Mirrors `compress::decompress_data(src, src_len, dst, &out_len, expected)`.
/// The caller sizes `dst`; the original allocated `cb.getNumberBase255()` bytes
/// (the recovered decompressed size from the header).
pub fn decompress(src: &[u8], dst: &mut [u8]) -> Result<usize, CompressError> {
    lzokay::decompress::decompress(src, dst).map_err(|_| CompressError::Decompress)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_random_data() {
        let data: Vec<u8> = (0..2048).map(|i| (i * 7) as u8).collect();
        let compressed = compress(&data).unwrap();
        let mut back = vec![0u8; data.len()];
        let n = decompress(&compressed, &mut back).unwrap();
        assert_eq!(n, data.len());
        assert_eq!(&back[..n], &data[..]);
    }

    #[test]
    fn round_trip_repetitive_compresses_well() {
        // Highly repetitive data should compress significantly.
        let data = b" Pangya Pangya Pangya Pangya Pangya ".repeat(64);
        let compressed = compress(&data).unwrap();
        assert!(
            compressed.len() < data.len() / 4,
            "expected strong compression, got {} from {}",
            compressed.len(),
            data.len()
        );
        let mut back = vec![0u8; data.len()];
        let n = decompress(&compressed, &mut back).unwrap();
        assert_eq!(&back[..n], &data[..]);
    }

    #[test]
    fn handles_empty_input() {
        let compressed = compress(&[]).unwrap();
        let mut back = vec![0u8; 8];
        // Empty/short input may decompress to zero bytes; just ensure no panic.
        let _ = decompress(&compressed, &mut back);
    }
}
