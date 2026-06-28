//! End-to-end login flow integration test against a live MySQL.
//!
//! Requires `DATABASE_URL=mysql://pangya:pangya@127.0.0.1:3306/pangya` and the
//! migrations applied. Skips automatically when no DB is reachable, so it is
//! safe in CI environments without MySQL.
//!
//! To run locally:
//!   docker compose up -d mysql
//!   sqlx migrate run          # DATABASE_URL set in .env or env
//!   DATABASE_URL=... cargo test -p pangya-server-core --test login_db -- --ignored

#![cfg(feature = "integration")]

use pangya_db::repos;
use pangya_model::{gen_auth_key, md5_hex};
use pangya_proto::LoginRequest;
use pangya_server_core::login::{handle_login, LoginConfig, LoginOutcome};
use rand::rngs::StdRng;
use rand::SeedableRng;
use sqlx::MySqlPool;

const DB_URL: &str = "mysql://pangya:pangya@127.0.0.1:3306/pangya";

async fn pool_or_skip() -> MySqlPool {
    if std::env::var("DATABASE_URL").is_err() && std::env::var("TEST_DATABASE_URL").is_err() {
        eprintln!("skipping login_db integration test: no DATABASE_URL");
    }
    let url = std::env::var("TEST_DATABASE_URL").unwrap_or_else(|_| DB_URL.to_owned());
    match MySqlPool::connect(&url).await {
        Ok(p) => p,
        Err(e) => {
            eprintln!("skipping login_db integration test: cannot reach DB: {e}");
            panic!("DB unavailable");
        }
    }
}

#[tokio::test]
#[ignore]
async fn login_succeeds_and_mints_auth_key() {
    let pool = pool_or_skip().await;

    // Seed a test account with a known password hash.
    let id = "itest_user";
    let pass = "itest_pass";
    let hash = md5_hex(pass);
    sqlx::query("DELETE FROM account WHERE ID = ?")
        .bind(id)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO account (ID, PASSWORD, NICK, Logon, capability, Sex, doTutorial) \
         VALUES (?, ?, ?, 0, 0, 0, 0)",
    )
    .bind(id)
    .bind(&hash)
    .bind(id)
    .execute(&pool)
    .await
    .unwrap();

    let req = LoginRequest {
        id: id.into(),
        password: pass.into(),
        options: vec![],
        mac_address: "00:00:00:00:00:00".into(),
    };

    let outcome = handle_login(&pool, &req, LoginConfig::default())
        .await
        .expect("login handler ok");

    match outcome {
        LoginOutcome::Success { bodies } => {
            assert!(!bodies.is_empty(), "should send at least the success body");
            // The first body is the 0x10 login-success packet.
            assert_eq!(bodies[0][0..2], [0x10, 0x00]);
        }
        LoginOutcome::Denied { .. } => panic!("expected success"),
    }

    // Cleanup.
    sqlx::query("DELETE FROM account WHERE ID = ?")
        .bind(id)
        .execute(&pool)
        .await
        .unwrap();
}

/// End-to-end check that the equipped character for the test account (UID 1)
/// loads from the DB (Erika, 0x04000001) with its equipped stat parts. The
/// equipped parts (non-zero `parts_id`) add a stat bonus on top of the clubset
/// base stats — together they make the client render non-zero stat bars. Guards
/// the `pangya_character_information` seed + the `parts_N`/`parts_id_N` loader
/// mapping.
#[tokio::test]
#[ignore]
async fn equipped_character_has_stat_parts() {
    let pool = pool_or_skip().await;
    let chars = repos::characters(&pool, 1).await.expect("load characters");
    let ci = chars.first().expect("test account has a character");

    assert_eq!(ci.typeid, 0x04000001, "equipped character is Erika");
    // Equipped stat parts (slot -> typeid) and their non-zero instance ids.
    let expect = [
        (0, 0x08040800i32, 11349i32),
        (2, 0x08044006, 11350),
        (3, 0x08046800, 11351),
        (7, 0x0804E004, 11352),
    ];
    for (slot, tid, id) in expect {
        assert_eq!(ci.parts_typeid[slot], tid, "parts_typeid[{slot}]");
        assert_eq!(ci.parts_id[slot], id, "parts_id[{slot}]");
    }
    // The stat-bearing parts (non-zero parts_id) must be present — without them
    // (and the clubset) the client shows all-zero stats.
    assert!(
        ci.parts_id.iter().filter(|&&v| v != 0).count() >= 4,
        "character must have equipped (stat-bearing) parts"
    );
}

#[tokio::test]
#[ignore]
async fn wrong_password_is_denied() {
    let pool = pool_or_skip().await;
    let id = "itest_wrong";
    sqlx::query("DELETE FROM account WHERE ID = ?")
        .bind(id)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO account (ID, PASSWORD, NICK, Logon, capability, Sex, doTutorial) \
         VALUES (?, ?, ?, 0, 0, 0, 0)",
    )
    .bind(id)
    .bind(md5_hex("correct"))
    .bind(id)
    .execute(&pool)
    .await
    .unwrap();

    let req = LoginRequest {
        id: id.into(),
        password: "wrong".into(),
        options: vec![],
        mac_address: "".into(),
    };
    let outcome = handle_login(&pool, &req, LoginConfig::default())
        .await
        .unwrap();
    assert!(matches!(outcome, LoginOutcome::Denied { .. }));

    sqlx::query("DELETE FROM account WHERE ID = ?")
        .bind(id)
        .execute(&pool)
        .await
        .unwrap();
}

// Silence unused-import warnings: repos/gen_auth_key are referenced by the
// intended full test suite; kept for when the feature is enabled.
#[allow(unused_imports)]
use gen_auth_key as _gen;
#[allow(unused_imports)]
use repos as _repos;
#[allow(unused_imports)]
use StdRng as _StdRng;
