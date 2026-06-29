//! Pangya patch tooling: the XTEA cipher and the `updatelist` manifest format,
//! shared by the server-side generator and the client-side updater.
//!
//! The `updatelist` is an XTEA-encrypted XML manifest the launcher fetches to
//! decide which game files are out of date. The cipher is the Pangya 16-round
//! XTEA variant (delta `0x61C88647`, little-endian words); ported from the
//! `pangbox/pangfiles` reference (and matched against pangya-editor).

pub mod xtea {
    /// Negated textbook delta (`-0x9E3779B9 mod 2^32`).
    const DELTA: u32 = 0x61C8_8647;
    const ROUNDS: usize = 16;

    /// Per-region keys (4 × u32). JP is the one this server targets.
    pub const KEY_JP: [u32; 4] = [0x020A_5FD4, 0x01EE_BDFF, 0x02B3_C6A0, 0x04F6_A3E1];

    fn encipher_block(mut v0: u32, mut v1: u32, key: &[u32; 4]) -> (u32, u32) {
        let mut sum: u32 = 0;
        for _ in 0..ROUNDS {
            let t = ((v1 << 4) ^ (v1 >> 5)).wrapping_add(v1);
            v0 = v0.wrapping_add(t ^ sum.wrapping_add(key[(sum & 3) as usize]));
            sum = sum.wrapping_sub(DELTA);
            let t = ((v0 << 4) ^ (v0 >> 5)).wrapping_add(v0);
            v1 = v1.wrapping_add(t ^ sum.wrapping_add(key[((sum >> 11) & 3) as usize]));
        }
        (v0, v1)
    }

    fn decipher_block(mut v0: u32, mut v1: u32, key: &[u32; 4]) -> (u32, u32) {
        let mut sum: u32 = 0xE377_9B90; // 16 * 0x9E3779B9 mod 2^32
        for _ in 0..ROUNDS {
            let t = ((v0 << 4) ^ (v0 >> 5)).wrapping_add(v0);
            v1 = v1.wrapping_sub(t ^ sum.wrapping_add(key[((sum >> 11) & 3) as usize]));
            sum = sum.wrapping_add(DELTA);
            let t = ((v1 << 4) ^ (v1 >> 5)).wrapping_add(v1);
            v0 = v0.wrapping_sub(t ^ sum.wrapping_add(key[(sum & 3) as usize]));
        }
        (v0, v1)
    }

    fn transform(data: &[u8], key: &[u32; 4], block: fn(u32, u32, &[u32; 4]) -> (u32, u32)) -> Vec<u8> {
        let mut out = Vec::with_capacity(data.len());
        for c in data.chunks_exact(8) {
            let v0 = u32::from_le_bytes([c[0], c[1], c[2], c[3]]);
            let v1 = u32::from_le_bytes([c[4], c[5], c[6], c[7]]);
            let (a, b) = block(v0, v1, key);
            out.extend_from_slice(&a.to_le_bytes());
            out.extend_from_slice(&b.to_le_bytes());
        }
        out
    }

    /// Encipher a buffer (length must be a multiple of 8).
    pub fn encipher(data: &[u8], key: &[u32; 4]) -> Vec<u8> {
        transform(data, key, encipher_block)
    }
    /// Decipher a buffer (length must be a multiple of 8).
    pub fn decipher(data: &[u8], key: &[u32; 4]) -> Vec<u8> {
        transform(data, key, decipher_block)
    }
}

pub mod updatelist {
    /// One file in the manifest. `f*` describe the on-disk file; `pname`/`psize`
    /// are the downloadable (optionally packed) copy.
    pub struct FileInfo {
        pub fname: String,
        pub fdir: String,
        pub fsize: u64,
        pub fcrc: i32,
        pub fdate: String,
        pub ftime: String,
        pub pname: String,
        pub psize: u64,
    }

    /// Build the `updatelist` XML (the plaintext, before XTEA).
    pub fn build_xml(patch_ver: &str, patch_num: u32, list_ver: &str, files: &[FileInfo]) -> String {
        let mut s = String::with_capacity(256 + files.len() * 160);
        s.push_str("<?xml version=\"1.0\" encoding=\"euc-kr\" standalone=\"yes\" ?>\n");
        s.push_str(&format!("<patchVer value=\"{patch_ver}\" />\n"));
        s.push_str(&format!("<patchNum value=\"{patch_num}\" />\n"));
        s.push_str(&format!("<updatelistVer value=\"{list_ver}\" />\n"));
        s.push_str(&format!("<updatefiles count=\"{}\">\n", files.len()));
        for f in files {
            s.push_str(&format!(
                "\t<fileinfo fname=\"{}\" fdir=\"{}\" fsize=\"{}\" fcrc=\"{}\" fdate=\"{}\" ftime=\"{}\" pname=\"{}\" psize=\"{}\" />\n",
                f.fname, f.fdir, f.fsize, f.fcrc, f.fdate, f.ftime, f.pname, f.psize
            ));
        }
        s.push_str("</updatefiles>\n");
        s
    }

    /// Encrypt manifest XML into a `updatelist` file: pad to an 8-byte multiple
    /// (with spaces, as the original does), then XTEA-encipher.
    pub fn encrypt(xml: &str, key: &[u32; 4]) -> Vec<u8> {
        let mut bytes = xml.as_bytes().to_vec();
        while bytes.len() % 8 != 0 {
            bytes.push(b' ');
        }
        crate::xtea::encipher(&bytes, key)
    }

    /// Decrypt a `updatelist` back to its XML text.
    pub fn decrypt(data: &[u8], key: &[u32; 4]) -> String {
        let plain = crate::xtea::decipher(data, key);
        String::from_utf8_lossy(&plain).into_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn xtea_round_trips() {
        let key = xtea::KEY_JP;
        let data = b"the quick brown fox jumps over!!"; // 32 bytes (mult of 8)
        let enc = xtea::encipher(data, &key);
        assert_ne!(enc, data);
        assert_eq!(xtea::decipher(&enc, &key), data);
    }

    #[test]
    fn updatelist_round_trips() {
        let files = vec![updatelist::FileInfo {
            fname: "ProjectG984.pak".into(),
            fdir: String::new(),
            fsize: 96485736,
            fcrc: -102194910,
            fdate: "2025-01-01".into(),
            ftime: "00:00:00".into(),
            pname: "ProjectG984.pak".into(),
            psize: 96485736,
        }];
        let xml = updatelist::build_xml("1.0", 1, "20250101", &files);
        let enc = updatelist::encrypt(&xml, &xtea::KEY_JP);
        assert_eq!(enc.len() % 8, 0);
        let back = updatelist::decrypt(&enc, &xtea::KEY_JP);
        assert!(back.contains("ProjectG984.pak"));
        assert!(back.contains("count=\"1\""));
    }
}
