//! Export the `pangya_iff_item` DB registry back into an IFF file.
//!
//! Reads an *original* IFF (to preserve every byte we don't track) and patches
//! each record's common Base + ShopDados fields (name, price, discount,
//! condition, shop flags, rental, active) from the DB, matched by
//! `(source, seq)`. Records with no DB row, and non-item tables, are copied
//! verbatim. Pack the result into a PAK with pangya-editor afterwards.
//!
//!   DATABASE_URL=mysql://… \
//!     cargo run -p iff-export -- [in.iff] [out.iff]
//!
//! v1 writes the common fields (covers shop-enable, prices, renames). Per-type
//! stat write-back (c[]/slot[]/…) is a follow-up; stats keep their original IFF
//! values for now.

use anyhow::{Context, Result};
use sqlx::Row;
use std::collections::{HashMap, HashSet};
use std::io::{Read, Write};

struct Fields {
    name: String,
    price: u64,
    discount: u32,
    cond: u32,
    flag_shop: u32,
    is_cash: i8,
    is_saleable: i8,
    rental_flag: i8,
    rental_days: i32,
    active: i8,
}

fn put_u32(b: &mut [u8], o: usize, v: u32) {
    b[o..o + 4].copy_from_slice(&v.to_le_bytes());
}

/// Patch a table's records in place from the DB rows (matched by `seq`).
fn patch(bytes: &mut [u8], source: &str, rows: &HashMap<(String, i32), Fields>) {
    if bytes.len() < 8 {
        return;
    }
    let count = u16::from_le_bytes([bytes[0], bytes[1]]) as usize;
    if count == 0 {
        return;
    }
    let rec = (bytes.len() - 8) / count;
    if rec < 132 {
        return; // not an item record (no ShopDados)
    }
    for i in 0..count {
        let Some(f) = rows.get(&(source.to_string(), i as i32)) else {
            continue;
        };
        let o = 8 + i * rec;
        // name[64] @8 — UTF-8 -> Shift-JIS, NUL-padded.
        let enc = encoding_rs::SHIFT_JIS.encode(&f.name).0;
        let n = enc.len().min(64);
        bytes[o + 8..o + 8 + n].copy_from_slice(&enc[..n]);
        for b in &mut bytes[o + 8 + n..o + 72] {
            *b = 0;
        }
        put_u32(bytes, o, f.active as u32); // Base.active @0
        put_u32(bytes, o + 116, f.price as u32);
        put_u32(bytes, o + 120, f.discount);
        put_u32(bytes, o + 124, f.cond);
        // flag_shop @128: keep the other bits, set is_cash(0) + is_saleable(5).
        let flag = (f.flag_shop as u16 & !0x0021)
            | (f.is_cash as u16 & 1)
            | (((f.is_saleable as u16) & 1) << 5);
        bytes[o + 128..o + 130].copy_from_slice(&flag.to_le_bytes());
        bytes[o + 130] = f.rental_flag as u8;
        bytes[o + 131] = f.rental_days as u8;
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let mut args = std::env::args().skip(1);
    let input = args.next().unwrap_or_else(|| {
        "reference-cpp/Server Lib/Game Server/Game Server/data/pangya_jp.iff".to_string()
    });
    let output = args.next().unwrap_or_else(|| "pangya_jp.modified.iff".to_string());
    let db_url = std::env::var("DATABASE_URL")
        .context("set DATABASE_URL (mysql://user:pass@host:port/db)")?;

    let pool = pangya_db::connect(&db_url).await.context("connecting to DB")?;
    let recs = sqlx::query(
        "SELECT source, seq, name, price, discount, cond_value, is_cash, is_saleable, \
         flag_shop, rental_flag, rental_days, active FROM pangya_iff_item",
    )
    .fetch_all(&pool)
    .await?;
    let mut rows: HashMap<(String, i32), Fields> = HashMap::with_capacity(recs.len());
    let mut sources: HashSet<String> = HashSet::new();
    for r in recs {
        let source: String = r.try_get("source")?;
        let seq: i32 = r.try_get("seq")?;
        sources.insert(source.clone());
        rows.insert(
            (source, seq),
            Fields {
                name: r.try_get("name")?,
                price: r.try_get("price")?,
                discount: r.try_get("discount")?,
                cond: r.try_get("cond_value")?,
                flag_shop: r.try_get("flag_shop")?,
                is_cash: r.try_get("is_cash")?,
                is_saleable: r.try_get("is_saleable")?,
                rental_flag: r.try_get("rental_flag")?,
                rental_days: r.try_get("rental_days")?,
                active: r.try_get("active")?,
            },
        );
    }

    // Read every entry, patch the tracked tables, write a fresh IFF.
    let mut archive = zip::ZipArchive::new(
        std::fs::File::open(&input).with_context(|| format!("opening {input}"))?,
    )?;
    let mut entries: Vec<(String, Vec<u8>)> = Vec::with_capacity(archive.len());
    for i in 0..archive.len() {
        let mut f = archive.by_index(i)?;
        let name = f.name().to_string();
        let mut bytes = Vec::with_capacity(f.size() as usize);
        f.read_to_end(&mut bytes)?;
        entries.push((name, bytes));
    }

    let out = std::fs::File::create(&output).with_context(|| format!("creating {output}"))?;
    let mut zw = zip::ZipWriter::new(out);
    let opts = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);
    let mut patched = 0usize;
    for (name, mut bytes) in entries {
        let source = name.strip_suffix(".iff").unwrap_or(&name);
        if sources.contains(source) {
            patch(&mut bytes, source, &rows);
            patched += 1;
        }
        zw.start_file(&name, opts)?;
        zw.write_all(&bytes)?;
    }
    zw.finish()?;
    println!("patched {patched} tables -> {output}");
    Ok(())
}
