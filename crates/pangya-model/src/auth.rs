//! Authentication helpers: MD5 password hashing and auth-key generation.
//!
//! The Pangya client sends the password in plaintext; the server hashes it
//! before comparing against the `account.PASSWORD` column (which stores the
//! 32-char lowercase hex MD5). See `login_server::requestLogin` →
//! `md5::processData` / `md5::getHash`.
//!
//! Auth keys are 8-char uppercase hex strings (4 random bytes), matching the
//! C++ `CmdGeraAuthKeyLogin` generation.

use md5::Digest;
use md5::Md5;

/// MD5-hash `password` and return the 32-char lowercase hex digest.
///
/// This matches the stored format: the original `md5::getHash` lowercased.
pub fn md5_hex(password: &str) -> String {
    let hash = Md5::digest(password.as_bytes());
    hex::encode(hash)
}

/// Generate an 8-char uppercase hex auth key from 4 random bytes.
pub fn gen_auth_key(rng: &mut impl rand::Rng) -> String {
    let bytes: [u8; 4] = rng.gen();
    hex::encode_upper(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::rngs::StdRng;
    use rand::SeedableRng;

    #[test]
    fn md5_known_vector() {
        // md5("hello") = 5d41402abc4b2a76b9719d911017c592
        assert_eq!(md5_hex("hello"), "5d41402abc4b2a76b9719d911017c592");
        assert_eq!(md5_hex(""), "d41d8cd98f00b204e9800998ecf8427e");
    }

    #[test]
    fn auth_key_is_8_uppercase_hex_chars() {
        let mut rng = StdRng::seed_from_u64(42);
        let key = gen_auth_key(&mut rng);
        assert_eq!(key.len(), 8);
        assert!(key
            .chars()
            .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit()));
    }

    #[test]
    fn auth_key_is_deterministic_for_seed() {
        let mut a = StdRng::seed_from_u64(1);
        let mut b = StdRng::seed_from_u64(1);
        assert_eq!(gen_auth_key(&mut a), gen_auth_key(&mut b));
    }
}
