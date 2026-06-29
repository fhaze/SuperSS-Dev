//! Generate a Pangya `updatelist` from a directory of patch files.
//!
//! Walks the patch root, computes each file's size + CRC32, builds the XML
//! manifest, and XTEA-encrypts it (JP key) into a `updatelist` the launcher can
//! consume. We control both ends (this generator + our updater), so the checksum
//! is plain CRC32 rather than Pangya's custom one. Files are served raw
//! (`pname == fname`); the updater downloads them directly.
//!
//!   cargo run -p updatelist-gen -- <patch-dir> <out-updatelist> [patch_num]

use anyhow::{Context, Result};
use pangya_patch::updatelist::{self, FileInfo};
use pangya_patch::xtea;
use std::path::{Path, PathBuf};

/// Collect every file under `dir` as `(fdir, path)` where `fdir` is the
/// Windows-style directory relative to `root` ("" at the top level).
fn collect(root: &Path, dir: &Path, out: &mut Vec<(String, PathBuf)>) -> Result<()> {
    for entry in std::fs::read_dir(dir).with_context(|| format!("reading {}", dir.display()))? {
        let path = entry?.path();
        if path.is_dir() {
            collect(root, &path, out)?;
        } else {
            let fdir = path
                .strip_prefix(root)
                .ok()
                .and_then(|r| r.parent())
                .map(|p| p.to_string_lossy().replace('/', "\\"))
                .unwrap_or_default();
            out.push((fdir, path));
        }
    }
    Ok(())
}

fn main() -> Result<()> {
    let mut args = std::env::args().skip(1);
    let root = PathBuf::from(
        args.next()
            .context("usage: updatelist-gen <patch-dir> <out-updatelist> [patch_num]")?,
    );
    let out_path = args.next().context("missing <out-updatelist>")?;
    let patch_num: u32 = args.next().and_then(|s| s.parse().ok()).unwrap_or(1);

    let mut found = Vec::new();
    collect(&root, &root, &mut found)?;
    found.sort();

    let mut files = Vec::with_capacity(found.len());
    for (fdir, path) in &found {
        let data = std::fs::read(path).with_context(|| format!("reading {}", path.display()))?;
        let crc = {
            let mut h = crc32fast::Hasher::new();
            h.update(&data);
            h.finalize()
        };
        let fname = path.file_name().unwrap().to_string_lossy().into_owned();
        println!("  {fdir}\\{fname}  ({} bytes, crc {})", data.len(), crc as i32);
        files.push(FileInfo {
            fname: fname.clone(),
            fdir: fdir.clone(),
            fsize: data.len() as u64,
            fcrc: crc as i32,
            fdate: "2025-01-01".into(),
            ftime: "00:00:00".into(),
            pname: fname,
            psize: data.len() as u64,
        });
    }

    let xml = updatelist::build_xml("1.0", patch_num, "20250101", &files);
    let enc = updatelist::encrypt(&xml, &xtea::KEY_JP);
    std::fs::write(&out_path, &enc).with_context(|| format!("writing {out_path}"))?;
    println!("wrote {} files -> {out_path} ({} bytes)", files.len(), enc.len());
    Ok(())
}
