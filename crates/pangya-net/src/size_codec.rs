//! The Pangya "conversionByte" wire size encoding — a bit-exact port of the C++
//! `packet::conversionByte` helper used to store the decompressed length of a
//! server-format packet in 4 bytes.
//!
//! ## How it is used
//!
//! - **Encode** (in `makeFull`, constructed with the `CB_BASE_256` flag):
//!   `dwConvertido = size`, then `invert()` runs the `CB_BASE_256` branch:
//!   `getNumberBase255` then `getNumberIS`, and the 4 raw bytes are emitted.
//! - **Decode** (in `unMakeFull`, constructed with `CB_BASE_255` from the wire
//!   bytes): `invert()` runs the `CB_BASE_255` branch: `getNumberIS` then
//!   `getNumberBase256`, then `getNumberNS()` returns the recovered size.
//!
//! ## Reduction
//!
//! `getNumberIS` is a byte-swap of a little-endian `u32` (i.e. it reads the
//! value back as big-endian). Working through the constructor flag branches:
//!
//! - `encode(size)` → `getNumberBase255(size).swap_bytes()`, emitted as 4 LE
//!   bytes ⟺ `getNumberBase255(size).to_be_bytes()`.
//! - `decode(bytes)` → `getNumberBase256(u32::from_be_bytes(bytes))`
//!   = `from_be(bytes) * 255 / 256 + 1`.
//!
//! `getNumberBase255(size)` packs size as `(quotient << 8) | remainder` with
//! divisor 255 — a base-255 representation where the high byte holds the
//! quotient and the low byte the remainder (each ≤ 254).

/// Encode a decompressed size into 4 wire bytes.
///
/// Equivalent to `conversionByte(size, CB_BASE_256).putNumberBuffer(buf)`.
pub fn encode_size(size: u32) -> [u8; 4] {
    base255(size).to_be_bytes()
}

/// Decode 4 wire bytes back into the decompressed size.
///
/// Equivalent to constructing `conversionByte(bytes, CB_BASE_255)` then reading
/// `getNumberNS()`. Note the `+1` in `getNumberBase256`: for sizes that are an
/// exact multiple of 255 the result is `size + 1`; the original `compress.cpp`
/// explicitly tolerates this off-by-one when checking the decompressed length.
pub fn decode_size(bytes: [u8; 4]) -> u32 {
    let be = u32::from_be_bytes(bytes);
    base256(be)
}

/// The buffer size the decoder allocates before decompression. Mirrors
/// `cb.getNumberBase255()` in `packet.cpp:319` (run *after* the constructor's
/// `invert()` already restored the value, so this is `base255(recovered)`).
pub fn decode_alloc_size(bytes: [u8; 4]) -> u32 {
    base255(decode_size(bytes))
}

// ── the two primitive transforms (named to match the C++) ─────────────────────

/// `getNumberBase255`: `(dwConvertido / 255) << 8 | dwConvertido % 255`.
fn base255(value: u32) -> u32 {
    ((value / 255) << 8) | (value % 255)
}

/// `getNumberBase256`: `getNumberNS() * 255 / 256 + 1`.
fn base256(value: u32) -> u32 {
    value * 255 / 256 + 1
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base255_round_trips_for_real_sizes() {
        // Trace the C++ path: decode(encode(size)) = base256(base255(size))
        //   = base255(size) * 255 / 256 + 1.
        // For most sizes this returns size exactly; the getNumberBase256 `+1`
        // is an inherent off-by-one in the original that the framing layer
        // tolerates (compress.cpp checks abs(actual - expected) != 1).
        for &size in &[
            1u32, 2, 100, 200, 254, 255, 256, 300, 510, 1000, 4096, 65535,
        ] {
            let encoded = encode_size(size);
            let decoded = decode_size(encoded);
            let diff = decoded.abs_diff(size);
            assert!(
                diff <= 1,
                "size {size}: decoded {decoded}, diff {diff} (C++ tolerance is 1)"
            );
        }
    }

    #[test]
    fn zero_decodes_to_one_matching_cpp_plus_one() {
        // getNumberBase256(0) = 0*255/256 + 1 = 1. A real packet body is never
        // size 0, but document the quirk so it isn't mistaken for a bug.
        assert_eq!(decode_size(encode_size(0)), 1);
    }

    #[test]
    fn known_vector_small_size() {
        // size = 100: base255 = 100 (0x00000064) → big-endian bytes [0,0,0,0x64].
        assert_eq!(encode_size(100), [0, 0, 0, 0x64]);
        assert_eq!(decode_size([0, 0, 0, 0x64]), 100);
    }

    #[test]
    fn known_vector_multibyte_base255() {
        // size = 300: 300 = 255*1 + 45, so base255 = (1<<8)|45 = 301 = 0x012D
        //   → big-endian bytes [0, 0, 0x01, 0x2D].
        assert_eq!(encode_size(300), [0, 0, 0x01, 0x2D]);
        assert_eq!(decode_size([0, 0, 0x01, 0x2D]), 300);
    }

    #[test]
    fn base255_alloc_size_is_identity_for_small_values() {
        // After decode, getNumberBase255 is applied to an already-small value
        // (< 255), which is the identity. Spot-check the helper.
        assert_eq!(decode_alloc_size([0, 0, 0, 0x64]), 100);
    }

    #[test]
    fn primitive_transforms_match_cpp_definitions() {
        // getNumberBase255(size) = (size/255 << 8) | size%255
        assert_eq!(base255(100), 100);
        assert_eq!(base255(300), 301);
        assert_eq!(base255(255), 256); // 255 = 255*1 + 0 → (1<<8)|0 = 256
                                       // getNumberBase256(v) = v*255/256 + 1
        assert_eq!(base256(100), 100 * 255 / 256 + 1);
        assert_eq!(base256(301), 300); // 301*255/256+1 = 299+1 = 300
    }
}
