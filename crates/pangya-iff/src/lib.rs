//! Loader for the Pangya IFF static-data archive.
//!
//! `pangya_jp.iff` is a ZIP archive whose entries are tightly-packed binary
//! tables. Each entry begins with an 8-byte [`Head`]:
//!
//! ```text
//! Head { count_element: u16, flag_ligacao: u16, version: u32 }
//! ```
//!
//! followed by `count_element` records of a fixed, table-specific size. The
//! loader validates `version == IFF_VERSION` (0x0D) and that
//! `count * record_size + HEAD_SIZE == entry_size`.
//!
//! This is a bit-exact port of the C++ `MAKE_UNZIP_MAP` / `MAKE_UNZIP_VECTOR`
//! macros in `Projeto IOCP/UTIL/iff.cpp`. Record structs (Character, Part,
//! ClubSet, Ball, Item, Caddie, Mascot, Card, Course, …) are added per system
//! as they are needed; each lives in a `records` submodule and implements
//! [`IffRecord`].

mod error;
mod records;

pub use error::IffError;
pub use records::{BaseRecord, Character, IffRecord};

use std::collections::HashMap;
use std::io::Read;
use std::path::Path;
use zip::ZipArchive;

/// IFF format version. All tables must report this.
pub const IFF_VERSION: u32 = 0x0D;

/// Size of the per-entry [`Head`].
pub const HEAD_SIZE: usize = 8;

/// The 8-byte header prefixing every IFF table entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Head {
    /// Number of records that follow.
    pub count_element: u16,
    /// "Flag de ligação" — cross-table link flag (unused by the loader itself).
    pub flag_ligacao: u16,
    pub version: u32,
}

impl Head {
    /// Parse a [`Head`] from the first [`HEAD_SIZE`] bytes.
    pub fn parse(bytes: &[u8]) -> Result<Self, IffError> {
        if bytes.len() < HEAD_SIZE {
            return Err(IffError::ShortHeader {
                got: bytes.len(),
                need: HEAD_SIZE,
            });
        }
        let count_element = u16::from_le_bytes([bytes[0], bytes[1]]);
        let flag_ligacao = u16::from_le_bytes([bytes[2], bytes[3]]);
        let version = u32::from_le_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]);
        if version != IFF_VERSION {
            return Err(IffError::UnsupportedVersion {
                version,
                expected: IFF_VERSION,
            });
        }
        Ok(Self {
            count_element,
            flag_ligacao,
            version,
        })
    }
}

/// An open IFF archive (a ZIP of named binary tables).
pub struct IffArchive<R: Read + Seek> {
    zip: ZipArchive<R>,
}

// `Seek` is required by ZipArchive but lives in std::io; re-export the bound.
use std::io::Seek;

impl<R: Read + Seek> IffArchive<R> {
    /// Open an archive from any reader.
    pub fn from_reader(reader: R) -> Result<Self, IffError> {
        Ok(Self {
            zip: ZipArchive::new(reader)?,
        })
    }

    /// List the entry names inside the archive (e.g. `"Character.iff"`).
    pub fn entry_names(&mut self) -> Vec<String> {
        (0..self.zip.len())
            .filter_map(|i| self.zip.by_index(i).ok().map(|f| f.name().to_owned()))
            .collect()
    }

    /// Read a raw entry's bytes by name.
    fn read_entry(&mut self, name: &str) -> Result<Vec<u8>, IffError> {
        let mut file = self.zip.by_name(name)?;
        let mut buf = Vec::with_capacity(file.size() as usize);
        file.read_to_end(&mut buf)?;
        Ok(buf)
    }

    /// Read a table as raw records: returns `(count, record_size, body)` where
    /// `body` is the bytes after the 8-byte [`Head`]. Lets callers (e.g. the IFF
    /// importer) read fields at fixed offsets without a typed record struct —
    /// useful because every table shares the same `Base` prefix but has a
    /// different total record size.
    pub fn read_table_raw(&mut self, name: &str) -> Result<(usize, usize, Vec<u8>), IffError> {
        let bytes = self.read_entry(name)?;
        let head = Head::parse(&bytes)?;
        let count = head.count_element as usize;
        let body = bytes[HEAD_SIZE..].to_vec();
        let record_size = if count > 0 { body.len() / count } else { 0 };
        Ok((count, record_size, body))
    }

    /// Load a table into a vector of records.
    ///
    /// Mirrors `MAKE_UNZIP_VECTOR`. Validates the header and that the entry
    /// size equals `count * record_size + HEAD_SIZE`.
    pub fn load_vec<T: IffRecord>(&mut self, name: &str) -> Result<Vec<T>, IffError> {
        let bytes = self.read_entry(name)?;
        Self::parse_vec(&bytes, name)
    }

    /// Load a table into a map keyed by each record's `_typeid`.
    ///
    /// Mirrors `MAKE_UNZIP_MAP`. If two records share a typeid the last wins,
    /// matching the C++ `map::operator[]` insertion semantics.
    pub fn load_map<T: BaseRecord>(&mut self, name: &str) -> Result<HashMap<u32, T>, IffError> {
        let bytes = self.read_entry(name)?;
        let vec = Self::parse_vec::<T>(&bytes, name)?;
        Ok(vec.into_iter().map(|r| (r.typeid(), r)).collect())
    }

    /// Look up a single `Character` record by its `_typeid`.
    ///
    /// Convenience wrapper around [`Self::load_map`] for the common case of
    /// resolving an equipped character (e.g. Erika `0x04000001`) from
    /// `Character.iff`.
    pub fn character_by_typeid(&mut self, typeid: u32) -> Option<Character> {
        self.load_map::<Character>("Character.iff")
            .ok()?
            .remove(&typeid)
    }

    fn parse_vec<T: IffRecord>(bytes: &[u8], name: &str) -> Result<Vec<T>, IffError> {
        let head = Head::parse(bytes)?;
        let record_size = T::SIZE;
        let expected_size = head.count_element as usize * record_size + HEAD_SIZE;
        if bytes.len() != expected_size {
            return Err(IffError::SizeMismatch {
                entry: name.to_owned(),
                expected: expected_size,
                actual: bytes.len(),
                count: head.count_element,
                record_size,
            });
        }
        let mut out = Vec::with_capacity(head.count_element as usize);
        let body = &bytes[HEAD_SIZE..];
        for chunk in body.chunks_exact(record_size) {
            out.push(T::from_le_bytes(chunk)?);
        }
        Ok(out)
    }
}

/// Convenience for the common case: an archive backed by a file.
impl IffArchive<std::fs::File> {
    pub fn open<P: AsRef<Path>>(path: P) -> Result<Self, IffError> {
        let file = std::fs::File::open(path)?;
        Self::from_reader(file)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Cursor, Write};

    /// Build a minimal in-memory ZIP containing one synthetic IFF table.
    fn make_zip(entry_name: &str, body: &[u8]) -> Vec<u8> {
        let mut buf = Cursor::new(Vec::new());
        {
            let mut zip = zip::ZipWriter::new(&mut buf);
            let opts = zip::write::SimpleFileOptions::default()
                .compression_method(zip::CompressionMethod::Stored);
            zip.start_file(entry_name, opts).unwrap();
            zip.write_all(body).unwrap();
            zip.finish().unwrap();
        }
        buf.into_inner()
    }

    // A trivial fixed-size record for testing the generic loader.
    #[repr(C, packed)]
    #[derive(Debug, Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
    struct TestRecord {
        typeid: u32,
        value: u16,
        _pad: u16,
    }

    impl IffRecord for TestRecord {
        const SIZE: usize = 8;
        fn from_le_bytes(b: &[u8]) -> Result<Self, IffError> {
            Ok(bytemuck::pod_read_unaligned(b))
        }
    }
    impl BaseRecord for TestRecord {
        fn typeid(&self) -> u32 {
            u32::from_le(self.typeid)
        }
    }

    fn make_table(records: &[TestRecord]) -> Vec<u8> {
        let mut buf = Vec::new();
        let head = (records.len() as u16).to_le_bytes().to_vec();
        // flag_ligacao (u16) + version (u32)
        let mut rest = vec![0u8; 6];
        rest[2..6].copy_from_slice(&IFF_VERSION.to_le_bytes());
        buf.extend_from_slice(&head);
        buf.extend_from_slice(&rest);
        for r in records {
            buf.extend_from_slice(&r.typeid.to_le_bytes());
            buf.extend_from_slice(&r.value.to_le_bytes());
            buf.extend_from_slice(&r._pad.to_le_bytes());
        }
        buf
    }

    #[test]
    fn parses_head() {
        let mut bytes = vec![0u8; HEAD_SIZE];
        bytes[0..2].copy_from_slice(&7u16.to_le_bytes()); // count
        bytes[4..8].copy_from_slice(&IFF_VERSION.to_le_bytes()); // version
        let head = Head::parse(&bytes).unwrap();
        assert_eq!(head.count_element, 7);
        assert_eq!(head.version, IFF_VERSION);
    }

    #[test]
    fn rejects_bad_version() {
        let bytes = [0, 0, 0, 0, 0xFE, 0xFF, 0xFF, 0xFF];
        assert!(Head::parse(&bytes).is_err());
    }

    #[test]
    fn loads_table_as_vec_and_map() {
        let records = vec![
            TestRecord {
                typeid: 100,
                value: 1,
                _pad: 0,
            },
            TestRecord {
                typeid: 200,
                value: 2,
                _pad: 0,
            },
            TestRecord {
                typeid: 300,
                value: 3,
                _pad: 0,
            },
        ];
        let table = make_table(&records);
        let zip_bytes = make_zip("Test.iff", &table);

        let mut archive = IffArchive::from_reader(Cursor::new(zip_bytes)).unwrap();
        assert!(archive.entry_names().iter().any(|n| n == "Test.iff"));

        let vec: Vec<TestRecord> = archive.load_vec("Test.iff").unwrap();
        assert_eq!(vec.len(), 3);
        assert_eq!(u32::from_le(vec[1].typeid), 200);

        let map = archive.load_map::<TestRecord>("Test.iff").unwrap();
        assert_eq!(map.len(), 3);
        assert_eq!(u16::from_le(map.get(&300).unwrap().value), 3);
    }

    #[test]
    fn rejects_size_mismatch() {
        // Truncate the body by one record.
        let mut table = make_table(&[
            TestRecord {
                typeid: 1,
                value: 0,
                _pad: 0,
            },
            TestRecord {
                typeid: 2,
                value: 0,
                _pad: 0,
            },
        ]);
        table.truncate(table.len() - 4);
        let zip_bytes = make_zip("Bad.iff", &table);
        let mut archive = IffArchive::from_reader(Cursor::new(zip_bytes)).unwrap();
        let err = archive.load_vec::<TestRecord>("Bad.iff").unwrap_err();
        assert!(matches!(err, IffError::SizeMismatch { .. }), "got {err:?}");
    }

    // ── integration: the real pangya_jp.iff ─────────────────────────────────
    //
    // The repo root ships `pangya_jp.iff`. These tests pin the Character table
    // layout against the live file: Erika (0x04000001) must parse with the
    // known PCL stats, and the table must hold all 14 characters.

    /// Locate the shipped `pangya_jp.iff`, searching from the crate root up.
    fn real_iff_path() -> Option<std::path::PathBuf> {
        for ancestor in std::path::Path::new(env!("CARGO_MANIFEST_DIR")).ancestors() {
            let candidate = ancestor.join("pangya_jp.iff");
            if candidate.exists() {
                return Some(candidate);
            }
        }
        None
    }

    #[test]
    fn parses_erika_from_real_iff() {
        let path = match real_iff_path() {
            Some(p) => p,
            None => {
                // The IFF isn't checked out in every environment (e.g. CI); skip
                // gracefully rather than fail.
                eprintln!("pangya_jp.iff not found; skipping live IFF test");
                return;
            }
        };
        let mut archive = IffArchive::open(&path).unwrap();

        let chars: Vec<Character> = archive.load_vec("Character.iff").unwrap();
        assert_eq!(chars.len(), 14, "JP Character.iff has 14 records");

        let mut map = archive.load_map::<Character>("Character.iff").unwrap();
        let erika = map.remove(&0x04000001).expect("Erika (0x04000001) must exist");
        // The name is Shift-JIS in the JP build (not used on the wire); just
        // confirm the record parsed past the name field with non-zero bytes.
        assert!(erika.name.iter().any(|&b| b != 0), "name parsed non-empty");
        assert_eq!(erika.c_stat, [9, 11, 6, 2, 2]); // P C A S C
    }

    #[test]
    fn character_by_typeid_lookup_works() {
        let path = match real_iff_path() {
            Some(p) => p,
            None => {
                eprintln!("pangya_jp.iff not found; skipping live IFF test");
                return;
            }
        };
        let mut archive = IffArchive::open(&path).unwrap();
        let erika = archive
            .character_by_typeid(0x04000001)
            .expect("Erika lookup");
        assert_eq!(erika._typeid, 0x04000001);
        assert_eq!(erika.c_stat, [9, 11, 6, 2, 2]);
    }
}
