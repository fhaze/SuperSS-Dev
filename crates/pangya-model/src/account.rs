//! Account and authentication-key domain types.
//!
//! Mirror the `account`, `authkey_login`, and `authkey_game` tables. Field names
//! match the original schema for clarity.

/// A player account row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Account {
    pub id: String,
    pub uid: i64,
    /// MD5 hash of the password (the client sends the plaintext, the server
    /// hashes it before comparison — see `login_server::requestLogin`).
    pub password_hash: String,
    pub nickname: String,
    pub logon: bool,
    pub first_login: bool,
    pub first_set: bool,
    /// Bitfield capability flags (`capability` column).
    pub capability: i32,
    pub sex: i16,
    pub do_tutorial: bool,
}

/// An 8-character authentication key minted on login / game-enter.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthKey {
    pub uid: i64,
    pub key: String,
    pub valid: bool,
}
