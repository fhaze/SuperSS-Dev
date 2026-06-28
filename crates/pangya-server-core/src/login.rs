//! Login flow orchestration: verify credentials, mint an auth key, and build
//! the response packet bodies.
//!
//! This is the typed, testable core of `login_server::requestLogin` +
//! `packet_func::succes_login`. The DB calls go through the bound-parameter
//! repos in `pangya-db`; the response bodies come from `pangya-proto`.

use anyhow::Result;
use pangya_db::repos;
use pangya_db::DbPool;
use pangya_model::{gen_auth_key, md5_hex};
use pangya_proto::login_resp::{self, ServerInfoWire};
use rand::rngs::StdRng;
use rand::SeedableRng;
use tracing::info;

/// The outcome of a login attempt: either the success response bodies to send,
/// or an error body + whether to disconnect.
#[derive(Debug)]
pub enum LoginOutcome {
    Success {
        /// Bodies in send order: login success, server list, message list.
        bodies: Vec<Vec<u8>>,
    },
    Denied {
        body: Vec<u8>,
        disconnect: bool,
    },
}

/// Configuration knobs the login flow consults (from `[OPTION]`).
#[derive(Debug, Clone, Copy, Default)]
pub struct LoginConfig {
    pub create_user: bool,
    pub access_flag: bool,
    pub same_id_login: bool,
}

/// Handle a `0x01` Login request end-to-end.
///
/// Mirrors the C++ success/error branches:
/// - missing account → wrong-ID/PW error (code 6), disconnect.
/// - bad password → wrong-ID/PW error (code 6), disconnect.
/// - otherwise → mint auth key, send login-success + server list + message list.
pub async fn handle_login(
    pool: &DbPool,
    req: &pangya_proto::LoginRequest,
    cfg: LoginConfig,
) -> Result<LoginOutcome> {
    let _ = cfg; // create_user / access_flag paths added incrementally

    // The client sends plaintext; hash before lookup/compare.
    let pass_hash = md5_hex(&req.password);

    // verify_credentials does an (ID, PASSWORD) match in one parameterized
    // query, closing the C++ `makeText` injection hole.
    let account = match repos::verify_credentials(pool, &req.id, &pass_hash).await? {
        Some(acc) => acc,
        None => {
            info!(id = %req.id, "login denied: bad id or password");
            return Ok(LoginOutcome::Denied {
                body: login_resp::build_login_error(6, None),
                disconnect: true,
            });
        }
    };

    // Mint an auth key and persist it.
    let mut rng = StdRng::from_entropy();
    let auth_key = gen_auth_key(&mut rng);
    repos::mint_login_auth_key(pool, account.uid, &auth_key).await?;
    info!(uid = account.uid, id = %account.id, "login success, minted auth key");

    // Build the server/message lists for the client.
    let game_servers = repos::server_list(pool).await?;
    let game_wires: Vec<ServerInfoWire> = game_servers
        .iter()
        .filter(|s| s.tipo == pangya_model::ServerType::Game)
        .map(server_to_wire)
        .collect();
    // Message servers aren't separately typed yet; reuse the same list filter
    // until the message-server distinction is wired (Phase 7).
    let msg_wires: Vec<ServerInfoWire> = Vec::new();

    Ok(LoginOutcome::Success {
        bodies: vec![
            login_resp::build_login_success(&auth_key),
            // Player info (0x01) — sends the UID so the client knows who it is.
            login_resp::build_login_player_info(
                &account.id,
                account.uid,
                account.capability,
                1, // level — minimal until PlayerInfo is fully loaded
                &account.nickname,
            ),
            login_resp::build_server_list(&game_wires),
            login_resp::build_message_server_list(&msg_wires),
        ],
    })
}

fn server_to_wire(s: &pangya_model::ServerEntry) -> ServerInfoWire {
    ServerInfoWire {
        name: s.name.clone(),
        uid: s.uid as i32,
        max_user: s.max_user as i32,
        curr_user: s.curr_user as i32,
        ip: s.ip.clone(),
        port: s.port as i32,
        property: s.property,
        angelic_wings_num: 0,
        event_flag: 0,
        event_map: 0,
        app_rate: 0,
        unknown: 0,
        img_no: s.img_no as i16,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn denied_body_uses_error_code_6() {
        let body = login_resp::build_login_error(6, None);
        assert_eq!(body, vec![0x01, 0x00, 0x06]);
    }

    #[test]
    fn server_to_wire_preserves_fields() {
        let s = pangya_model::ServerEntry {
            name: "Tittan Boo".into(),
            uid: 20203,
            ip: "127.0.0.1".into(),
            port: 20203,
            max_user: 2001,
            curr_user: 5,
            tipo: pangya_model::ServerType::Game,
            state: 0,
            exp_rate: 100,
            pang_rate: 100,
            img_no: 2,
            property: 2048,
        };
        let w = server_to_wire(&s);
        assert_eq!(w.uid, 20203);
        assert_eq!(w.port, 20203);
        assert_eq!(w.img_no, 2);
    }
}
