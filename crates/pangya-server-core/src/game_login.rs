//! Game Server login flow orchestration: verify the client's auth key, load the
//! player state, and produce the response bodies.
//!
//! Mirrors `game_server::requestLogin` (the verification gate) + the early part
//! of `LoginTask::sendCompleteData`. The full player-info serialization
//! (`principal()`) is added incrementally; for Milestone 1 we send the login
//! ack + channel list so a client reaches the lobby.

use anyhow::Result;
use pangya_db::repos;
use pangya_db::DbPool;
use pangya_model::CharacterInfo;
use pangya_proto::game_resp::{self, ChannelInfoWire};
use pangya_proto::GameLoginRequest;
use tracing::{info, warn};

/// The outcome of a game-server login attempt.
#[derive(Debug)]
pub enum GameLoginOutcome {
    /// Login accepted: the ack body + the channel-list body to send, plus the
    /// player's characters so the `0x000B` (change item) handler can answer
    /// type-4 requests with the equipped character.
    Accepted {
        uid: i64,
        bodies: Vec<Vec<u8>>,
        characters: Vec<CharacterInfo>,
    },
    /// Login denied: a single denial body + whether to disconnect.
    Denied { body: Vec<u8>, disconnect: bool },
}

/// Handle a game-server `0x02` login request.
pub async fn handle_game_login(
    pool: &DbPool,
    req: &GameLoginRequest,
    channels: &[pangya_model::Channel],
) -> Result<GameLoginOutcome> {
    // Verify the auth key the client presents (minted by the Login Server).
    let auth = match repos::verify_game_auth_key(pool, req.uid, &req.auth_key).await? {
        Some(a) if a.valid => a,
        _ => {
            warn!(uid = req.uid, id = %req.id, "game login denied: bad auth key");
            return Ok(GameLoginOutcome::Denied {
                body: game_resp::build_login_denied(500020),
                disconnect: true,
            });
        }
    };
    let _ = auth;

    // Load the player identity + member info.
    let identity = match repos::player_identity(pool, req.uid).await? {
        Some(id) if id.id == req.id => id,
        Some(id) => {
            warn!(
                uid = req.uid,
                "game login denied: id mismatch ({} != {})", id.id, req.id
            );
            return Ok(GameLoginOutcome::Denied {
                body: game_resp::build_login_denied(500020),
                disconnect: true,
            });
        }
        None => {
            warn!(uid = req.uid, "game login denied: player not found");
            return Ok(GameLoginOutcome::Denied {
                body: game_resp::build_login_denied(500020),
                disconnect: true,
            });
        }
    };

    // Mark the account logged in.
    let _ = repos::register_logon(pool, req.uid).await;
    info!(uid = identity.uid, id = %identity.id, "game login accepted");

    // Load the player's characters. The equipped character (the first owned,
    // mirroring the C++ `equipDefaultCharacter` picking `mp_ce.begin()`) is
    // serialized into the principal packet and returned for the 0x000B handler.
    let characters = repos::characters(pool, req.uid).await?;
    let equipped = characters.first();
    if let Some(ci) = equipped {
        info!(
            uid = identity.uid,
            typeid = format!("0x{:08X}", ci.typeid),
            "equipped character loaded"
        );
    } else {
        warn!(uid = identity.uid, "player has no characters; sending zeroed CharacterInfo");
    }

    // Build the channel list from the server's registry. The wire entry mirrors
    // the C++ ChannelInfo struct, validated against captured 0x004D packets.
    let channel_wires: Vec<ChannelInfoWire> = channels
        .iter()
        .enumerate()
        .map(|(i, c)| ChannelInfoWire::from_channel(i as u8, c))
        .collect();

    // The equipment cascade (LoginTask::sendCompleteData). After the principal
    // packet the client expects a burst of collection packets; without them it
    // hangs in "Loading...". For a fresh account most lists are empty, but the
    // client still requires each packet. The equipped character_id is set in the
    // user-equip packet so the client knows which character it is using.
    let equipped_char_id = equipped.map(|ci| ci.id).unwrap_or(0);

    Ok(GameLoginOutcome::Accepted {
        uid: identity.uid,
        bodies: vec![
            game_resp::build_login_ack_d3(),
            // Full player info (0x44 option 0) — the client needs this before
            // it can function in the lobby. The equipped CharacterInfo must be
            // present or the client stays in "Loading..." / disconnects.
            game_resp::build_player_info(
                &req.client_version,
                identity.uid,
                &identity.id,
                &identity.nickname,
                2048, // server property
                equipped,
            ),
            // Equipment cascade (mirrors sendCompleteData order).
            game_resp::build_character_list(&characters),
            game_resp::build_caddie_list(0),
            game_resp::build_warehouse_list(0),
            game_resp::build_mascot_list(0),
            game_resp::build_user_equip(equipped_char_id),
            game_resp::build_channel_list(&channel_wires),
        ],
        characters,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn channel_to_wire_preserves_fields() {
        let ch = pangya_model::Channel::new(1, "Beginners".into(), 500, 16, 512);
        let wires: Vec<ChannelInfoWire> = std::iter::once(&ch)
            .enumerate()
            .map(|(i, c)| ChannelInfoWire::from_channel(i as u8, c))
            .collect();
        assert_eq!(wires[0].max_level_allow, 16);
        assert_eq!(wires[0].flag, 512);
    }
}
