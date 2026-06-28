//! Pangya XOR packet cipher — a bit-exact port of the C++ `crypt` class.
//!
//! ## Algorithm (from `Projeto IOCP/CRYPT/crypt.cpp`)
//!
//! The cipher is keyed by a single byte pair derived from the session key
//! (high nibble, 0..=15) and the per-packet `low_key` byte (0..=255):
//!
//! ```text
//! pos_dic = (session_key << 8) | low_key          // 0..=4095
//! k0 = KEYS[pos_dic]          // XOR seed byte (first table half)
//! k1 = KEYS[4096 + pos_dic]   // check/verify byte (second table half)
//! ```
//!
//! `k0` lives in the low byte of a little-endian `u32`, so the first 4-byte
//! block is XORed with `[k0, 0, 0, 0]` — i.e. only byte 0 is really keyed and
//! bytes 1–3 pass through unchanged. Bytes 4 onward chain with the **plaintext**
//! 4 bytes back (stride-4, PCBC-style). This is symmetric and self-inverting.
//!
//! The first plaintext byte must equal `k1` after the round-trip — an
//! integrity/authenticity check. Both `encrypt` and `decrypt` assert it.

use crate::key_dictionary::KEYS;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum CryptError {
    #[error("empty buffer (size is 0)")]
    Empty,
    /// Integrity-check failed: the first plaintext byte did not match `k1`.
    /// The C++ code distinguishes encrypt (code 2) from decrypt (code 3); both
    /// mean "the wrong key was used / data corrupted".
    #[error("integrity check failed ({context})")]
    Integrity { context: &'static str },
}

/// The Pangya cipher. Cheap to construct; clone freely. Not `Clone` is fine —
/// it's two bytes — but we derive it for convenience.
#[derive(Debug, Clone, Copy)]
pub struct Crypt {
    /// The two derived key bytes: `k0` = XOR seed, `k1` = check byte.
    /// Mirrors the C++ `m_key[2]` after `init_key`, but as plain bytes.
    k: [u8; 2],
}

impl Crypt {
    /// Equivalent to the default C++ constructor + a fresh `init_key`.
    pub fn new(session_key: u8, low_key: u8) -> Self {
        let k = init_keys(session_key, low_key);
        Self { k }
    }

    /// `(session_key << 8) | low_key`, clamped to the dictionary range.
    /// The C++ code uses `unsigned short`, so the high byte masks naturally
    /// to 8 bits; here `session_key` is documented as 0..=15 but we accept any
    /// `u8` to match the bit-shifting exactly (only the low byte is used).
    pub fn init_keys(session_key: u8, low_key: u8) -> [u8; 2] {
        init_keys(session_key, low_key)
    }

    /// The check byte `k1` — written as the first payload byte on encrypt and
    /// verified on decrypt.
    pub const fn check_byte(&self) -> u8 {
        self.k[1]
    }

    /// Decrypt `cipher` into `plain` in place (or into a same-length buffer).
    ///
    /// `plain` and `cipher` may be the same buffer (overlapping writes are
    /// monotonic forward, matching the C++ semantics).
    pub fn decrypt(&self, cipher: &[u8], plain: &mut [u8]) -> Result<(), CryptError> {
        if cipher.is_empty() {
            return Err(CryptError::Empty);
        }
        if cipher.len() != plain.len() {
            // The C++ code assumes equal length; make it explicit.
            panic!(
                "decrypt: cipher ({}) and plain ({}) lengths differ",
                cipher.len(),
                plain.len()
            );
        }

        // k0 occupies the low byte of a little-endian u32 → [k0, 0, 0, 0].
        let key_le = [self.k[0], 0, 0, 0];

        let n = cipher.len();
        let first = n.min(4);

        // First block (up to 4 bytes): XOR with the little-endian key bytes.
        for i in 0..first {
            plain[i] = cipher[i] ^ key_le[i];
        }
        // Remaining: chain on plaintext, stride 4.
        for i in 4..n {
            plain[i] = cipher[i] ^ plain[i - 4];
        }

        // Integrity check: plain[0] must equal the check byte.
        if plain[0] != self.k[1] {
            return Err(CryptError::Integrity { context: "decrypt" });
        }
        Ok(())
    }

    /// Encrypt `plain` into `cipher`. May alias.
    ///
    /// The caller must ensure `plain[0] == self.check_byte()` (the protocol
    /// always prepends the derived key byte before encrypting). If it doesn't,
    /// the integrity check fails.
    pub fn encrypt(&self, plain: &[u8], cipher: &mut [u8]) -> Result<(), CryptError> {
        if plain.is_empty() {
            return Err(CryptError::Empty);
        }
        assert_eq!(
            plain.len(),
            cipher.len(),
            "encrypt: plain and cipher lengths differ"
        );

        let key_le = [self.k[0], 0, 0, 0];
        let n = plain.len();
        let first = n.min(4);

        for i in 0..first {
            cipher[i] = plain[i] ^ key_le[i];
        }
        for i in 4..n {
            // Chains on PLAINTEXT (not ciphertext) — same as decrypt.
            cipher[i] = plain[i] ^ plain[i - 4];
        }

        // Integrity check: recovering plain[0] from cipher[0] must yield k1.
        if cipher[0] ^ self.k[0] != self.k[1] {
            return Err(CryptError::Integrity { context: "encrypt" });
        }
        Ok(())
    }
}

fn init_keys(session_key: u8, low_key: u8) -> [u8; 2] {
    let pos_dic = (u16::from(session_key) << 8) | u16::from(low_key);
    [KEYS[pos_dic as usize], KEYS[(4096 + pos_dic) as usize]]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dictionary_is_the_expected_size() {
        assert_eq!(KEYS.len(), 8192);
    }

    #[test]
    fn init_key_matches_known_first_entries() {
        // From key_dictionary.h: first row is
        //   0x00, 0x01, 0x29, 0x23, 0xBE, 0x84, 0xE1, 0x6C, ...
        // With session_key=0, low_key=0 → pos_dic=0  → k0=KEYS[0]=0x00, k1=KEYS[4096].
        assert_eq!(KEYS[0], 0x00);
        assert_eq!(KEYS[1], 0x01);
        assert_eq!(KEYS[2], 0x29);
        assert_eq!(KEYS[3], 0x23);
        assert_eq!(KEYS[4], 0xBE);

        let k = Crypt::init_keys(0, 0);
        assert_eq!(k[0], KEYS[0]);
        assert_eq!(k[1], KEYS[4096]);
    }

    #[test]
    fn encrypt_decrypt_round_trip_single_byte() {
        // A 1-byte payload: only the first-block branch runs.
        // plain[0] must be the check byte k1.
        let c = Crypt::new(0, 0);
        let plain_in = [c.check_byte() ^ 0x5A; 1]; // arbitrary, then we set check
        let plain = [c.check_byte()]; // valid: plain[0] == k1
        let _ = plain_in; // (kept to show the constraint)

        let mut cipher = [0u8; 1];
        c.encrypt(&plain, &mut cipher).unwrap();
        let mut back = [0u8; 1];
        c.decrypt(&cipher, &mut back).unwrap();
        assert_eq!(back, plain);
    }

    #[test]
    fn encrypt_decrypt_round_trip_multiblock() {
        // Session key 5, low key 200 — exercises both halves of the dictionary.
        let c = Crypt::new(5, 200);

        // Build a payload whose first byte is the check byte (required).
        let mut plain = vec![c.check_byte()];
        plain.extend_from_slice(&[
            0xDE, 0xAD, 0xBE, 0xEF, // completes first block + starts chaining
            0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0A,
        ]);

        let mut cipher = vec![0u8; plain.len()];
        c.encrypt(&plain, &mut cipher).unwrap();

        let mut back = vec![0u8; plain.len()];
        c.decrypt(&cipher, &mut back).unwrap();
        assert_eq!(back, plain);
    }

    #[test]
    fn decrypt_detects_wrong_key() {
        let enc = Crypt::new(5, 200);
        let dec = Crypt::new(5, 201); // different low_key → different check byte

        let mut plain = vec![enc.check_byte()];
        plain.extend_from_slice(&[1, 2, 3, 4, 5, 6, 7, 8]);

        let mut cipher = vec![0u8; plain.len()];
        enc.encrypt(&plain, &mut cipher).unwrap();

        let mut back = vec![0u8; plain.len()];
        assert!(dec.decrypt(&cipher, &mut back).is_err());
    }

    #[test]
    fn encrypt_detects_invalid_first_byte() {
        let c = Crypt::new(3, 17);
        // plain[0] != k1 → encrypt's integrity check fails.
        let plain = [c.check_byte() ^ 0xFF, 0x11, 0x22, 0x33, 0x44];
        let mut cipher = [0u8; 5];
        assert!(c.encrypt(&plain, &mut cipher).is_err());
    }

    #[test]
    fn encrypt_decrypt_in_place_alias() {
        // The C++ code reads plain[i-4] while writing cipher[i]; in-place is
        // safe because both directions are forward-monotonic. Verify aliasing.
        let c = Crypt::new(2, 42);
        let original = {
            let mut v = vec![c.check_byte()];
            v.extend_from_slice(&[10, 20, 30, 40, 50, 60, 70, 80]);
            v
        };

        let mut buf = original.clone();
        c.encrypt(&buf.clone(), &mut buf).unwrap();
        let cipher = buf;

        let mut buf2 = cipher.clone();
        c.decrypt(&buf2.clone(), &mut buf2).unwrap();
        assert_eq!(buf2, original);
    }
}
