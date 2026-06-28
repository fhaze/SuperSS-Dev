//! IFF record traits and the concrete table record structs.
//!
//! Each Pangya IFF table holds fixed-size records. A record implements
//! [`IffRecord`] (fixed size + little-endian parse); tables that are keyed by
//! `_typeid` (the common case) additionally implement [`BaseRecord`].
//!
//! Record structs mirror the C++ `IFF::Base`-derived structs in
//! `Projeto IOCP/TYPE/data_iff.h`. They use `#[repr(C, packed)]` to match the
//! on-disk layout exactly. The C bitfields (`level : 7`, `is_max : 1`, etc.)
//! are represented as raw `u8`/`u16` fields here and decoded via accessor
//! methods, since stable Rust has no stable C-bitfield layout.

use crate::error::IffError;

/// A fixed-size IFF record that can be parsed from a little-endian byte slice.
///
/// `SIZE` must equal the C++ `sizeof(T)` for the corresponding struct.
pub trait IffRecord: Sized {
    /// The on-disk size of this record in bytes (the C++ `sizeof`).
    const SIZE: usize;

    /// Parse one record from a `SIZE`-length little-endian byte slice.
    fn from_le_bytes(bytes: &[u8]) -> Result<Self, IffError>;
}

/// A record that has a `_typeid` key — the common Pangya case, used by
/// `MAKE_UNZIP_MAP` to build `map<typeid, record>`.
pub trait BaseRecord: IffRecord {
    fn typeid(&self) -> u32;
}

/// The shared prefix of most IFF records: `Base` from `data_iff.h:144`.
///
/// Not all fields of the C++ `Base` are ported here yet (the `ShopDados`,
/// `TikiShopDados`, and `DateDados` sub-structs use C bitfields and SYSTEMTIME
/// and are added per-system as needed). The core identity fields — `active`,
/// `_typeid`, and `name` — are present so the loader and lookups work.
#[allow(dead_code)] // fields filled in incrementally as systems are ported
#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
pub struct Base {
    pub active: u32,
    pub _typeid: u32,
    pub name: [u8; 64],
    // The full C++ Base continues with: stLevel level (bitfields), icon[43],
    // ShopDados, TikiShopDados, DateDados. Ported incrementally per system.
}

impl Base {
    /// The record's name as a `&str`, trimmed at the first NUL byte.
    #[allow(dead_code)] // used once systems consume IFF records
    pub fn name_str(&self) -> &str {
        let end = self
            .name
            .iter()
            .position(|&b| b == 0)
            .unwrap_or(self.name.len());
        std::str::from_utf8(&self.name[..end]).unwrap_or("")
    }
}

/// `Character.iff` record — mirrors `IFF::Character : public Base`
/// (`data_iff.h:667`). The full on-disk record is 420 bytes; the middle Base
/// sub-structs (shop/tiki/date) and the per-character textures are not decoded
/// yet — only the identity fields and the character stats (`c_stat[5]`, the
/// PCL: power/control/accuracy/spin/curve) that the lobby/`0x0044` flow needs.
///
/// Layout verified byte-by-byte against `pangya_jp.iff`:
/// `active:u32` @0, `_typeid:u32` @4, `name:[u8;64]` @8, …, `c_stat:[u8;5]` @372.
#[derive(Debug, Clone)]
pub struct Character {
    pub active: u32,
    pub _typeid: u32,
    pub name: [u8; 64],
    /// Character stats — PCL order is power/control/accuracy/spin/curve
    /// (`Stats` enum in `pangya_st.h:390`). At on-disk offset 372.
    pub c_stat: [u8; 5],
}

impl Character {
    /// The record's name, trimmed at the first NUL byte.
    pub fn name_str(&self) -> &str {
        let end = self
            .name
            .iter()
            .position(|&b| b == 0)
            .unwrap_or(self.name.len());
        std::str::from_utf8(&self.name[..end]).unwrap_or("")
    }
}

impl IffRecord for Character {
    const SIZE: usize = 420;
    fn from_le_bytes(bytes: &[u8]) -> Result<Self, IffError> {
        debug_assert_eq!(bytes.len(), Self::SIZE);
        let active = u32::from_le_bytes(bytes[0..4].try_into().unwrap());
        let _typeid = u32::from_le_bytes(bytes[4..8].try_into().unwrap());
        let mut name = [0u8; 64];
        name.copy_from_slice(&bytes[8..72]);
        let mut c_stat = [0u8; 5];
        c_stat.copy_from_slice(&bytes[372..377]);
        Ok(Self {
            active,
            _typeid,
            name,
            c_stat,
        })
    }
}

impl BaseRecord for Character {
    fn typeid(&self) -> u32 {
        self._typeid
    }
}

// NOTE: Base itself does not implement IffRecord because its *full* on-disk
// size depends on the still-unported sub-structs. Once those land, give Base a
// const SIZE and implement IffRecord/BaseRecord; the per-record structs
// (Item, Character, ...) embed or overlay Base and add their own SIZE.
