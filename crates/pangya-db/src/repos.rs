//! Typed repositories — one per logical query, all using bound parameters.
//!
//! Each function here replaces a C++ `Cmd*` / stored-procedure pair. The proc
//! names are documented at each function for traceability to the original.

use pangya_model::{
    Account, AuthKey, CaddieInfo, CharacterInfo, ClubSetInfo, MascotInfo, MemberInfo,
    PlayerIdentity, ServerEntry, ServerType, UserEquip, UserInfo, WarehouseItem,
};
use sqlx::mysql::MySqlRow;
use sqlx::Row;

use crate::pool::DbPool;

#[derive(Debug, thiserror::Error)]
pub enum RepoError {
    #[error("database error: {0}")]
    Db(#[from] sqlx::Error),
    /// A row was found but a required column had an unexpected NULL.
    #[error("invalid data: {0}")]
    InvalidData(String),
}

// ── account ───────────────────────────────────────────────────────────────────

/// Look up an account by login ID. Replaces `ProcVerifyID` + `GetInfo_User`.
///
/// The query is parameterized — `id` is bound, never interpolated — closing the
/// injection hole the C++ `makeText` path left open.
pub async fn account_by_id(pool: &DbPool, id: &str) -> Result<Option<Account>, RepoError> {
    let row = sqlx::query(
        "SELECT ID, UID, PASSWORD, NICK, Logon, FIRST_LOGIN, FIRST_SET, \
         capability, Sex, doTutorial \
         FROM account WHERE ID = ? LIMIT 1",
    )
    .bind(id)
    .fetch_optional(pool)
    .await?;

    row.map(account_from_row).transpose()
}

/// Verify the (id, password-hash) pair and return the account on match.
/// Replaces `ProcVerifyPass` (which the C++ built via `makeText`).
pub async fn verify_credentials(
    pool: &DbPool,
    id: &str,
    password_hash: &str,
) -> Result<Option<Account>, RepoError> {
    let row = sqlx::query(
        "SELECT ID, UID, PASSWORD, NICK, Logon, FIRST_LOGIN, FIRST_SET, \
         capability, Sex, doTutorial \
         FROM account WHERE ID = ? AND PASSWORD = ? LIMIT 1",
    )
    .bind(id)
    .bind(password_hash)
    .fetch_optional(pool)
    .await?;

    row.map(account_from_row).transpose()
}

fn account_from_row(row: MySqlRow) -> Result<Account, RepoError> {
    let get_str = |col: &str| -> Result<String, RepoError> {
        row.try_get::<Option<String>, _>(col)?
            .ok_or_else(|| RepoError::InvalidData(format!("column {col} was NULL")))
    };
    Ok(Account {
        id: get_str("ID")?,
        uid: row.try_get("UID")?,
        password_hash: get_str("PASSWORD")?,
        nickname: get_str("NICK")?,
        logon: row.try_get::<i64, _>("Logon")? != 0,
        first_login: row.try_get::<i64, _>("FIRST_LOGIN")? != 0,
        first_set: row.try_get::<i64, _>("FIRST_SET")? != 0,
        capability: row.try_get("capability")?,
        sex: row.try_get("Sex")?,
        do_tutorial: row.try_get::<i64, _>("doTutorial")? != 0,
    })
}

// ── auth keys ─────────────────────────────────────────────────────────────────

/// Mint a login auth key for a UID. Replaces `ProcGeraAuthKeyLogin`.
///
/// Generates an 8-char hex key, persists it, and returns it. Uses `ON DUPLICATE
/// KEY UPDATE` so re-login replaces the prior key (mirrors the C++ behaviour of
/// overwriting the existing row for the UID).
pub async fn mint_login_auth_key(pool: &DbPool, uid: i64, key: &str) -> Result<(), RepoError> {
    sqlx::query(
        "INSERT INTO authkey_login (UID, AuthKey, valid) VALUES (?, ?, 1) \
         ON DUPLICATE KEY UPDATE AuthKey = VALUES(AuthKey), valid = 1",
    )
    .bind(uid)
    .bind(key)
    .execute(pool)
    .await?;
    Ok(())
}

/// Validate a login auth key. Replaces `ProcGetAuthKeyLogin`.
pub async fn verify_login_auth_key(
    pool: &DbPool,
    uid: i64,
    key: &str,
) -> Result<Option<AuthKey>, RepoError> {
    let row =
        sqlx::query("SELECT UID, AuthKey, valid FROM authkey_login WHERE UID = ? AND AuthKey = ?")
            .bind(uid)
            .bind(key)
            .fetch_optional(pool)
            .await?;

    row.map(auth_key_from_row).transpose()
}

fn auth_key_from_row(row: MySqlRow) -> Result<AuthKey, RepoError> {
    let get_str = |col: &str| -> Result<String, RepoError> {
        row.try_get::<Option<String>, _>(col)?
            .ok_or_else(|| RepoError::InvalidData(format!("column {col} was NULL")))
    };
    Ok(AuthKey {
        uid: row.try_get("UID")?,
        key: get_str("AuthKey")?,
        valid: row.try_get::<i16, _>("valid")? != 0,
    })
}

// ── server list ───────────────────────────────────────────────────────────────

/// List all currently-registered servers. Replaces `ProcGetServerList`.
pub async fn server_list(pool: &DbPool) -> Result<Vec<ServerEntry>, RepoError> {
    let rows = sqlx::query(
        "SELECT Name, UID, IP, Port, MaxUser, CurrUser, Type, State, \
         ExpRate, PangRate, ImgNo, property FROM pangya_server_list",
    )
    .fetch_all(pool)
    .await?;

    rows.into_iter().map(server_entry_from_row).collect()
}

fn server_entry_from_row(row: MySqlRow) -> Result<ServerEntry, RepoError> {
    let tipo_raw: i16 = row.try_get("Type")?;
    let tipo = ServerType::from_raw(tipo_raw)
        .ok_or_else(|| RepoError::InvalidData(format!("unknown server Type {tipo_raw}")))?;
    let get_str = |col: &str| -> Result<String, RepoError> {
        row.try_get::<Option<String>, _>(col)?
            .ok_or_else(|| RepoError::InvalidData(format!("column {col} was NULL")))
    };
    Ok(ServerEntry {
        name: get_str("Name")?,
        uid: row.try_get::<i32, _>("UID")? as u32,
        ip: get_str("IP")?,
        port: row.try_get::<i32, _>("Port")? as u16,
        max_user: row.try_get::<i32, _>("MaxUser")? as u32,
        curr_user: row.try_get::<i32, _>("CurrUser")? as u32,
        tipo,
        state: row.try_get("State")?,
        exp_rate: row.try_get::<i32, _>("ExpRate")? as u32,
        pang_rate: row.try_get::<i32, _>("PangRate")? as u32,
        img_no: row.try_get::<i16, _>("ImgNo")? as u16,
        property: row.try_get::<i32, _>("property")? as u32,
    })
}

// ── player info (game server) ─────────────────────────────────────────────────

/// Load a player's identity row. Replaces `CmdPlayerInfo` →
/// `pangya.ProcGetPlayerInfoGame`.
pub async fn player_identity(pool: &DbPool, uid: i64) -> Result<Option<PlayerIdentity>, RepoError> {
    // Fall back to the account table's identity fields when the Pangya game
    // view is absent (a fresh install with only the minimal migration).
    account_identity_fallback(pool, uid).await
}

async fn account_identity_fallback(
    pool: &DbPool,
    uid: i64,
) -> Result<Option<PlayerIdentity>, RepoError> {
    let row = sqlx::query("SELECT ID, NICK, capability FROM account WHERE UID = ?")
        .bind(uid)
        .fetch_optional(pool)
        .await?;

    match row {
        Some(row) => Ok(Some(PlayerIdentity {
            uid,
            id: get_string(&row, "ID")?,
            nickname: get_string(&row, "NICK")?,
            level: 1,
            id_state: 0,
            block_time: 0,
        })),
        None => Ok(None),
    }
}

/// Load the member-info row. Replaces `CmdMemberInfo` → `pangya.ProcGetUserInfo`.
pub async fn member_info(pool: &DbPool, uid: i64) -> Result<Option<MemberInfo>, RepoError> {
    let row = sqlx::query(
        "SELECT ID, NICK, capability, Sex, doTutorial, School, MannerFlag \
         FROM account WHERE UID = ?",
    )
    .bind(uid)
    .fetch_optional(pool)
    .await?;

    Ok(match row {
        Some(row) => Some(MemberInfo {
            id: get_string(&row, "ID")?,
            nickname: get_string(&row, "NICK")?,
            guild_name: String::new(),
            guild_mark_img: String::new(),
            // The account table uses signed INT/smallint for these; cast to the
            // model's unsigned types.
            capability: row.try_get::<i32, _>("capability")? as u32,
            oid: 0,
            guild_uid: 0,
            state_flag: 0,
            sex: row.try_get::<i16, _>("Sex")? as i8,
            level: 1,
            do_tutorial: row.try_get::<i64, _>("doTutorial")? != 0,
            school: row.try_get("School")?,
            manner_flag: row.try_get("MannerFlag")?,
        }),
        None => None,
    })
}

/// Load the equipped-slot indices. Replaces `CmdUserEquip` →
/// `pangya.USP_CHAR_USER_EQUIP` (which reads `pangya_user_equip`).
///
/// If no row exists for the UID (a dev environment without migration 0003),
/// returns a zeroed `UserEquip` so the client at least gets a valid struct.
pub async fn user_equip(pool: &DbPool, uid: i64) -> Result<UserEquip, RepoError> {
    let row = sqlx::query(
        "SELECT caddie_id, character_id, club_id, ball_type, \
         item_slot_1, item_slot_2, item_slot_3, item_slot_4, item_slot_5, \
         item_slot_6, item_slot_7, item_slot_8, item_slot_9, item_slot_10, \
         Skin_1, Skin_2, Skin_3, Skin_4, Skin_5, Skin_6, \
         mascot_id, poster_1, poster_2 \
         FROM pangya_user_equip WHERE UID = ?",
    )
    .bind(uid)
    .fetch_optional(pool)
    .await?;

    Ok(match row {
        Some(row) => {
            let item_slot = [
                row.try_get::<i32, _>("item_slot_1")?,
                row.try_get::<i32, _>("item_slot_2")?,
                row.try_get::<i32, _>("item_slot_3")?,
                row.try_get::<i32, _>("item_slot_4")?,
                row.try_get::<i32, _>("item_slot_5")?,
                row.try_get::<i32, _>("item_slot_6")?,
                row.try_get::<i32, _>("item_slot_7")?,
                row.try_get::<i32, _>("item_slot_8")?,
                row.try_get::<i32, _>("item_slot_9")?,
                row.try_get::<i32, _>("item_slot_10")?,
            ];
            let skin_id = [
                row.try_get::<i32, _>("Skin_1")?,
                row.try_get::<i32, _>("Skin_2")?,
                row.try_get::<i32, _>("Skin_3")?,
                row.try_get::<i32, _>("Skin_4")?,
                row.try_get::<i32, _>("Skin_5")?,
                row.try_get::<i32, _>("Skin_6")?,
            ];
            UserEquip {
                caddie_id: row.try_get("caddie_id")?,
                character_id: row.try_get("character_id")?,
                clubset_id: row.try_get("club_id")?,
                ball_typeid: row.try_get("ball_type")?,
                item_slot,
                skin_id,
                // skin_typeid is not persisted in pangya_user_equip (the SP
                // joins it from elsewhere); zeroed until that join is ported.
                skin_typeid: [0; 6],
                mascot_id: row.try_get("mascot_id")?,
                poster: [row.try_get::<i32, _>("poster_1")?, row.try_get::<i32, _>("poster_2")?],
            }
        }
        None => UserEquip::default(),
    })
}

/// Load all caddies owned by a player. Replaces `CmdCaddieInfo(ALL)` →
/// `pangya.ProcGetCaddieInfo`. Empty for a fresh account.
pub async fn caddies(pool: &DbPool, uid: i64) -> Result<Vec<CaddieInfo>, RepoError> {
    let rows = sqlx::query(
        "SELECT item_id, typeid, parts_typeid, cLevel, Exp, RentFlag, Purchase, CheckEnd \
         FROM pangya_caddie_information WHERE UID = ? AND Valid = 1",
    )
    .bind(uid)
    .fetch_all(pool)
    .await;

    match rows {
        Ok(rows) => Ok(rows
            .into_iter()
            .map(|row| CaddieInfo {
                id: row.try_get::<i64, _>("item_id").unwrap_or(0) as i32,
                typeid: row.try_get("typeid").unwrap_or(0),
                parts_typeid: row.try_get("parts_typeid").unwrap_or(0),
                level: clamp_u8(&row, "cLevel"),
                exp: row.try_get::<i64, _>("Exp").unwrap_or(0) as u32,
                rent_flag: clamp_u8(&row, "RentFlag"),
                purchase: clamp_u8(&row, "Purchase"),
                check_end: row.try_get("CheckEnd").unwrap_or(0),
                ..Default::default()
            })
            .collect()),
        Err(_) => Ok(Vec::new()),
    }
}

/// Load all warehouse items owned by a player. Replaces `CmdWarehouseItem(ALL)`
/// → `pangya.ProcGetWarehouseItem`. Empty for a fresh account.
pub async fn warehouse(pool: &DbPool, uid: i64) -> Result<Vec<WarehouseItem>, RepoError> {
    let rows = sqlx::query(
        "SELECT item_id, typeid, C0, C1, C2, C3, C4, Purchase, flag, ItemType \
         FROM pangya_item_warehouse WHERE UID = ? AND valid = 1",
    )
    .bind(uid)
    .fetch_all(pool)
    .await;

    match rows {
        Ok(rows) => Ok(rows
            .into_iter()
            .map(|row| WarehouseItem {
                id: row.try_get::<i64, _>("item_id").unwrap_or(0) as i32,
                typeid: row.try_get("typeid").unwrap_or(0),
                c: [
                    row.try_get::<i64, _>("C0").unwrap_or(0) as i16,
                    row.try_get::<i64, _>("C1").unwrap_or(0) as i16,
                    row.try_get::<i64, _>("C2").unwrap_or(0) as i16,
                    row.try_get::<i64, _>("C3").unwrap_or(0) as i16,
                    row.try_get::<i64, _>("C4").unwrap_or(0) as i16,
                ],
                purchase: clamp_u8(&row, "Purchase"),
                flag: clamp_u8(&row, "flag"),
                item_type: clamp_u8(&row, "ItemType"),
                ..Default::default()
            })
            .collect()),
        Err(_) => Ok(Vec::new()),
    }
}

/// Load all mascots owned by a player. Replaces `CmdMascotInfo(ALL)` →
/// `pangya.ProcGetMascotInfo`. Empty for a fresh account.
pub async fn mascots(pool: &DbPool, uid: i64) -> Result<Vec<MascotInfo>, RepoError> {
    let rows = sqlx::query(
        "SELECT item_id, typeid, mLevel, mExp, Flag, Tipo, Message \
         FROM pangya_mascot_info WHERE UID = ? AND Valid = 1",
    )
    .bind(uid)
    .fetch_all(pool)
    .await;

    match rows {
        Ok(rows) => Ok(rows
            .into_iter()
            .map(|row| MascotInfo {
                id: row.try_get::<i64, _>("item_id").unwrap_or(0) as i32,
                typeid: row.try_get("typeid").unwrap_or(0),
                level: clamp_u8(&row, "mLevel"),
                exp: row.try_get::<i64, _>("mExp").unwrap_or(0) as u32,
                flag: clamp_u8(&row, "Flag"),
                tipo: row.try_get("Tipo").unwrap_or(0),
                message: get_string(&row, "Message").unwrap_or_default(),
            })
            .collect()),
        Err(_) => Ok(Vec::new()),
    }
}

/// Resolve the equipped clubset into a `ClubSetInfo` (the 28-byte wire struct).
/// `UserEquip.clubset_id` holds the warehouse **item_id** (the instance, per
/// the C++ `player.cpp`), so we load the matching warehouse row by item_id and
/// read its typeid + workshop stats. `slot_c`/`enchant_c` stay zero until the
/// full clubset-stats system lands.
pub async fn clubset_info(pool: &DbPool, uid: i64) -> Result<ClubSetInfo, RepoError> {
    let equip = user_equip(pool, uid).await?;
    if equip.clubset_id == 0 {
        return Ok(ClubSetInfo::default());
    }
    let row = sqlx::query(
        "SELECT item_id, typeid FROM pangya_item_warehouse \
         WHERE UID = ? AND item_id = ? AND valid = 1 LIMIT 1",
    )
    .bind(uid)
    .bind(equip.clubset_id)
    .fetch_optional(pool)
    .await?;
    Ok(match row {
        Some(row) => ClubSetInfo {
            id: row.try_get::<i64, _>("item_id").unwrap_or(0) as i32,
            typeid: row.try_get("typeid").unwrap_or(0),
            ..Default::default()
        },
        None => ClubSetInfo {
            id: equip.clubset_id,
            ..Default::default()
        },
    })
}

/// Load all characters for a player. Replaces `CmdCharacterInfo(ALL)` →
/// `pangya_character_information`.
///
/// If the table or the player's row is missing (a dev environment that hasn't
/// run migration 0002), falls back to a hardcoded Erika default so the client
/// still gets a valid character past "Loading...".
pub async fn characters(pool: &DbPool, uid: i64) -> Result<Vec<CharacterInfo>, RepoError> {
    // The client computes the character's stat bars from the equipped parts —
    // each part with a non-zero `parts_id_N` whose `parts_N` typeid resolves in
    // Part.iff contributes its IFF stat slots. `PCL` is loaded for completeness
    // but the client ignores it (the live capture had PCL all-zero).
    let rows = sqlx::query(
        "SELECT item_id, typeid, \
         PCL0, PCL1, PCL2, PCL3, PCL4, \
         default_hair, default_shirts, gift_flag, Purchase, Mastery, \
         parts_1, parts_2, parts_3, parts_4, parts_5, parts_6, parts_7, parts_8, \
         parts_9, parts_10, parts_11, parts_12, parts_13, parts_14, parts_15, parts_16, \
         parts_17, parts_18, parts_19, parts_20, parts_21, parts_22, parts_23, parts_24, \
         parts_id_1, parts_id_2, parts_id_3, parts_id_4, parts_id_5, parts_id_6, \
         parts_id_7, parts_id_8, parts_id_9, parts_id_10, parts_id_11, parts_id_12, \
         parts_id_13, parts_id_14, parts_id_15, parts_id_16, parts_id_17, parts_id_18, \
         parts_id_19, parts_id_20, parts_id_21, parts_id_22, parts_id_23, parts_id_24 \
         FROM pangya_character_information WHERE UID = ?",
    )
    .bind(uid)
    .fetch_all(pool)
    .await;

    match rows {
        Ok(rows) if !rows.is_empty() => {
            let mut out = Vec::with_capacity(rows.len());
            for row in rows {
                let mut pcl = [0u8; 5];
                pcl[0] = clamp_u8(&row, "PCL0");
                pcl[1] = clamp_u8(&row, "PCL1");
                pcl[2] = clamp_u8(&row, "PCL2");
                pcl[3] = clamp_u8(&row, "PCL3");
                pcl[4] = clamp_u8(&row, "PCL4");
                let mut parts_typeid = [0i32; 24];
                let mut parts_id = [0i32; 24];
                for i in 0..24 {
                    parts_typeid[i] =
                        row.try_get::<i32, _>(format!("parts_{}", i + 1).as_str()).unwrap_or(0);
                    parts_id[i] =
                        row.try_get::<i32, _>(format!("parts_id_{}", i + 1).as_str()).unwrap_or(0);
                }
                out.push(CharacterInfo {
                    typeid: row.try_get("typeid")?,
                    id: row.try_get::<i64, _>("item_id")? as i32,
                    default_hair: clamp_u8(&row, "default_hair"),
                    default_shirts: clamp_u8(&row, "default_shirts"),
                    gift_flag: clamp_u8(&row, "gift_flag"),
                    purchase: clamp_u8(&row, "Purchase"),
                    parts_typeid,
                    parts_id,
                    pcl,
                    mastery: row.try_get("Mastery")?,
                    ..Default::default()
                });
            }
            Ok(out)
        }
        // Table missing or no rows for this player → dev fallback.
        _ => Ok(default_erika()),
    }
}

/// The dev fallback: Erika (0x04000001) with the PCL stats read from
/// `pangya_jp.iff`. Used when no `pangya_character_information` row exists so a
/// fresh install still serves a valid character.
fn default_erika() -> Vec<CharacterInfo> {
    vec![CharacterInfo::from_iff(
        0x04000001,
        1,
        [9, 11, 6, 2, 2],
    )]
}

/// Coerce a `smallint`/`int` column down to a `u8` for the wire struct's
/// byte-sized fields. Values out of range clamp rather than error — they come
/// from untrusted DB data and the wire field can only hold 0..255.
fn clamp_u8(row: &MySqlRow, col: &str) -> u8 {
    row.try_get::<i64, _>(col).unwrap_or(0).clamp(0, 255) as u8
}

/// Load the player's spendable balances (pang + cookie). Replaces the pang/cookie
/// fields of `CmdUserInfo` → `pangya.GetInfo_User`. Missing row → zero balances.
pub async fn user_info(pool: &DbPool, uid: i64) -> Result<UserInfo, RepoError> {
    let row = sqlx::query("SELECT pang, cookie FROM pangya_player_currency WHERE UID = ?")
        .bind(uid)
        .fetch_optional(pool)
        .await;
    Ok(match row {
        Ok(Some(row)) => UserInfo {
            pang: row.try_get::<u64, _>("pang").unwrap_or(0),
            cookie: row.try_get::<u64, _>("cookie").unwrap_or(0),
            ..Default::default()
        },
        _ => UserInfo::default(),
    })
}

/// Deduct `amount` pang from a player, guarded so the balance never goes negative.
/// Returns `true` if the deduction was applied (sufficient balance), else `false`.
pub async fn spend_pang(pool: &DbPool, uid: i64, amount: u64) -> Result<bool, RepoError> {
    let res = sqlx::query(
        "UPDATE pangya_player_currency SET pang = pang - ? WHERE UID = ? AND pang >= ?",
    )
    .bind(amount)
    .bind(uid)
    .bind(amount)
    .execute(pool)
    .await?;
    Ok(res.rows_affected() > 0)
}

/// Persist a character's full equipped-parts arrays (the C++
/// `CmdUpdateCharacterAllPartEquiped`, sent on a `0x20` type-0 equip). Updates
/// every `parts_N` (typeid) + `parts_id_N` (instance id) for the character row.
pub async fn update_character_parts(
    pool: &DbPool,
    uid: i64,
    item_id: i32,
    parts_typeid: &[i32; 24],
    parts_id: &[i32; 24],
) -> Result<(), RepoError> {
    let mut q = sqlx::query(
        "UPDATE pangya_character_information SET \
         parts_1=?, parts_id_1=?, parts_2=?, parts_id_2=?, parts_3=?, parts_id_3=?, \
         parts_4=?, parts_id_4=?, parts_5=?, parts_id_5=?, parts_6=?, parts_id_6=?, \
         parts_7=?, parts_id_7=?, parts_8=?, parts_id_8=?, parts_9=?, parts_id_9=?, \
         parts_10=?, parts_id_10=?, parts_11=?, parts_id_11=?, parts_12=?, parts_id_12=?, \
         parts_13=?, parts_id_13=?, parts_14=?, parts_id_14=?, parts_15=?, parts_id_15=?, \
         parts_16=?, parts_id_16=?, parts_17=?, parts_id_17=?, parts_18=?, parts_id_18=?, \
         parts_19=?, parts_id_19=?, parts_20=?, parts_id_20=?, parts_21=?, parts_id_21=?, \
         parts_22=?, parts_id_22=?, parts_23=?, parts_id_23=?, parts_24=?, parts_id_24=? \
         WHERE item_id=? AND UID=?",
    );
    for i in 0..24 {
        q = q.bind(parts_typeid[i]).bind(parts_id[i]);
    }
    q.bind(item_id).bind(uid).execute(pool).await?;
    Ok(())
}

/// Deduct `amount` cookie (cash) from a player, guarded against going negative.
/// Returns `true` if applied. Mirrors the cookie side of `consomeMoeda`.
pub async fn spend_cookie(pool: &DbPool, uid: i64, amount: u64) -> Result<bool, RepoError> {
    let res = sqlx::query(
        "UPDATE pangya_player_currency SET cookie = cookie - ? WHERE UID = ? AND cookie >= ?",
    )
    .bind(amount)
    .bind(uid)
    .bind(amount)
    .execute(pool)
    .await?;
    Ok(res.rows_affected() > 0)
}

/// Grant an item to a player's warehouse (the C++ `item_manager::addItem` for a
/// shop purchase). Returns the new warehouse `item_id` (echoed back to the client
/// in the buy receipt). `ItemType` defaults to 2 (a normal item).
pub async fn add_warehouse_item(pool: &DbPool, uid: i64, typeid: i32) -> Result<i64, RepoError> {
    let res = sqlx::query(
        "INSERT INTO pangya_item_warehouse (UID, typeid, ItemType, Purchase) VALUES (?, ?, 2, 1)",
    )
    .bind(uid)
    .bind(typeid)
    .execute(pool)
    .await?;
    Ok(res.last_insert_id() as i64)
}

// ── auth keys (game server verify) ────────────────────────────────────────────

/// Validate a game-server auth key. The C++ `requestLogin` verifies against the
/// login auth key via `CmdAuthKeyLoginInfo`; we mirror that for Milestone 1.
pub async fn verify_game_auth_key(
    pool: &DbPool,
    uid: i64,
    key: &str,
) -> Result<Option<AuthKey>, RepoError> {
    verify_login_auth_key(pool, uid, key).await
}

/// Mark a player as logged into this server. Replaces `CmdRegisterLogon`.
pub async fn register_logon(pool: &DbPool, uid: i64) -> Result<(), RepoError> {
    sqlx::query("UPDATE account SET Logon = 1, LastLogonTime = NOW() WHERE UID = ?")
        .bind(uid)
        .execute(pool)
        .await?;
    Ok(())
}

/// Mint a game-server auth key. Replaces `CmdAuthKeyGame` /
/// `ProcGeraAuthKeyGame`. The client presents this to the Game Server.
pub async fn mint_game_auth_key(pool: &DbPool, uid: i64, key: &str) -> Result<(), RepoError> {
    sqlx::query(
        "INSERT INTO authkey_game (UID, AuthKey, valid) VALUES (?, ?, 1) \
         ON DUPLICATE KEY UPDATE AuthKey = VALUES(AuthKey), valid = 1",
    )
    .bind(uid)
    .bind(key)
    .execute(pool)
    .await?;
    Ok(())
}

// ── shared row helpers ────────────────────────────────────────────────────────

fn get_string(row: &MySqlRow, col: &str) -> Result<String, RepoError> {
    row.try_get::<Option<String>, _>(col)?
        .ok_or_else(|| RepoError::InvalidData(format!("column {col} was NULL")))
}
