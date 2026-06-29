//! Import the PangYa IFF item catalog into the `pangya_iff_item` DB table.
//!
//! Reads the ownable-item IFF tables, extracts the shared Base + ShopDados
//! fields (uniform across every item) plus the per-type stat arrays, and writes
//! one editable row per item. Re-runnable (truncates first).
//!
//!   DATABASE_URL=mysql://pangya:pangya@127.0.0.1:3306/pangya \
//!     cargo run -p iff-import -- [path/to/pangya_jp.iff]
//!
//! Field offsets are relative to each record's start. Base occupies bytes 0-191
//! (typeid @4, name[64] @8, ShopDados price @116, …); type-specific fields begin
//! at 192. All IFF structs are `#pragma pack(1)`.

use anyhow::{Context, Result};

#[derive(Clone, Copy)]
enum Elem {
    U8,
    U16,
}

/// Where a table's stat fields live (offsets from record start). All fields are
/// `Option`, so `Default` is "no stats" without `Elem` needing to be `Default`.
#[derive(Clone, Copy, Default)]
struct Stats {
    c: Option<(usize, Elem)>,
    slot: Option<(usize, Elem)>,
    club: Option<usize>,   // u32[4]
    efeito: Option<usize>, // i16[5]: power_drive, drop_rate, power_gauge, pang_rate, exp_rate
}

struct TableSpec {
    entry: &'static str,
    source: &'static str,
    stats: Stats,
}

const fn spec(entry: &'static str, source: &'static str, stats: Stats) -> TableSpec {
    TableSpec { entry, source, stats }
}

fn tables() -> Vec<TableSpec> {
    use Elem::{U16, U8};
    let none = Stats::default();
    vec![
        // Stat-bearing equipment.
        spec("Part.iff", "Part", Stats { c: Some((484, U16)), slot: Some((494, U16)), ..none }),
        spec("ClubSet.iff", "ClubSet", Stats { c: Some((208, U16)), slot: Some((218, U16)), club: Some(192), ..none }),
        spec("Club.iff", "Club", Stats { c: Some((234, U16)), ..none }),
        spec("Ball.iff", "Ball", Stats { c: Some((804, U16)), ..none }),
        spec("AuxPart.iff", "AuxPart", Stats { c: Some((202, U8)), slot: Some((207, U8)), efeito: Some(212), ..none }),
        spec("Character.iff", "Character", Stats { c: Some((352, U16)), ..none }),
        spec("Caddie.iff", "Caddie", Stats { c: Some((236, U16)), ..none }),
        spec("Mascot.iff", "Mascot", Stats { c: Some((277, U8)), efeito: Some(282), ..none }),
        // Cosmetic / functional items (common fields only).
        spec("Item.iff", "Item", none),
        spec("Card.iff", "Card", none),
        spec("Skin.iff", "Skin", none),
        spec("HairStyle.iff", "HairStyle", none),
        spec("AddonPart.iff", "AddonPart", none),
        spec("CounterItem.iff", "CounterItem", none),
        spec("CaddieItem.iff", "CaddieItem", none),
        spec("Furniture.iff", "Furniture", none),
    ]
}

fn u32at(b: &[u8], o: usize) -> u32 {
    u32::from_le_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]])
}
fn u16at(b: &[u8], o: usize) -> u16 {
    u16::from_le_bytes([b[o], b[o + 1]])
}
fn i16at(b: &[u8], o: usize) -> i16 {
    i16::from_le_bytes([b[o], b[o + 1]])
}

/// Decode a 64-byte Shift-JIS name field, trimmed at the first NUL.
fn decode_name(b: &[u8]) -> String {
    let end = b.iter().position(|&c| c == 0).unwrap_or(b.len());
    encoding_rs::SHIFT_JIS.decode(&b[..end]).0.into_owned()
}

fn read_array(body: &[u8], rec_off: usize, what: Option<(usize, Elem)>, rec_size: usize) -> [Option<i16>; 5] {
    let mut out = [None; 5];
    if let Some((off, elem)) = what {
        let span = match elem {
            Elem::U8 => 5,
            Elem::U16 => 10,
        };
        if off + span <= rec_size {
            for (k, slot) in out.iter_mut().enumerate() {
                *slot = Some(match elem {
                    Elem::U8 => body[rec_off + off + k] as i16,
                    Elem::U16 => u16at(body, rec_off + off + k * 2) as i16,
                });
            }
        }
    }
    out
}

#[tokio::main]
async fn main() -> Result<()> {
    let iff_path = std::env::args().nth(1).unwrap_or_else(|| {
        "reference-cpp/Server Lib/Game Server/Game Server/data/pangya_jp.iff".to_string()
    });
    let db_url = std::env::var("DATABASE_URL")
        .context("set DATABASE_URL (mysql://user:pass@host:port/db)")?;

    let mut iff = pangya_iff::IffArchive::open(&iff_path)
        .with_context(|| format!("opening IFF {iff_path}"))?;
    let pool = pangya_db::connect(&db_url).await.context("connecting to DB")?;

    sqlx::query("TRUNCATE TABLE pangya_iff_item").execute(&pool).await?;

    let mut total = 0usize;
    let mut tx = pool.begin().await?;
    for t in tables() {
        let (count, rec, body) = match iff.read_table_raw(t.entry) {
            Ok(v) => v,
            Err(e) => {
                eprintln!("skip {} ({e})", t.entry);
                continue;
            }
        };
        if rec < 132 {
            eprintln!("skip {} (record {rec}B too small for ShopDados)", t.entry);
            continue;
        }
        for i in 0..count {
            let o = i * rec;
            let typeid = u32at(&body, o + 4);
            let name = decode_name(&body[o + 8..o + 72]);
            let price = u32at(&body, o + 116);
            let discount = u32at(&body, o + 120);
            let cond = u32at(&body, o + 124);
            let flag_shop = u16at(&body, o + 128);
            let c = read_array(&body, o, t.stats.c, rec);
            let slot = read_array(&body, o, t.stats.slot, rec);
            let club: [Option<i64>; 4] = match t.stats.club {
                Some(off) if off + 16 <= rec => {
                    std::array::from_fn(|k| Some(u32at(&body, o + off + k * 4) as i64))
                }
                _ => [None; 4],
            };
            let efeito: [Option<i16>; 5] = match t.stats.efeito {
                Some(off) if off + 10 <= rec => {
                    std::array::from_fn(|k| Some(i16at(&body, o + off + k * 2)))
                }
                _ => [None; 5],
            };

            sqlx::query(
                "INSERT INTO pangya_iff_item \
                 (typeid, source, seq, name, price, discount, cond_value, is_cash, is_saleable, \
                  flag_shop, rental_flag, rental_days, active, \
                  c0, c1, c2, c3, c4, slot0, slot1, slot2, slot3, slot4, \
                  club0, club1, club2, club3, \
                  power_drive, drop_rate, power_gauge, pang_rate, exp_rate) \
                 VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?, ?,?,?,?,?, ?,?,?,?,?, ?,?,?,?, ?,?,?,?,?)",
            )
            .bind(typeid as u64)
            .bind(t.source)
            .bind(i as i32)
            .bind(&name)
            .bind(price as u64)
            .bind(discount as i64)
            .bind(cond as i64)
            .bind((flag_shop & 1) as i8)
            .bind(((flag_shop >> 5) & 1) as i8)
            .bind(flag_shop as i64)
            .bind(body[o + 130] as i8)
            .bind(body[o + 131] as i32)
            .bind((u32at(&body, o) != 0) as i8)
            .bind(c[0]).bind(c[1]).bind(c[2]).bind(c[3]).bind(c[4])
            .bind(slot[0]).bind(slot[1]).bind(slot[2]).bind(slot[3]).bind(slot[4])
            .bind(club[0]).bind(club[1]).bind(club[2]).bind(club[3])
            .bind(efeito[0]).bind(efeito[1]).bind(efeito[2]).bind(efeito[3]).bind(efeito[4])
            .execute(&mut *tx)
            .await
            .with_context(|| format!("inserting {} 0x{typeid:08X}", t.source))?;
            total += 1;
        }
        println!("{:>6} {}", count, t.source);
    }
    tx.commit().await?;
    println!("imported {total} items into pangya_iff_item");
    Ok(())
}
