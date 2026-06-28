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
use pangya_model::{CaddieInfo, CharacterInfo, ClubSetInfo, MascotInfo, UserEquip, WarehouseItem};
use pangya_proto::game_resp::{self, ChannelInfoWire};
use pangya_proto::GameLoginRequest;
use tracing::{info, warn};

/// The loaded player equipment, threaded from login to the `0x000B` handler so
/// change-item requests for any type can be answered from real state.
#[derive(Debug, Default, Clone)]
pub struct PlayerEquipment {
    pub equip: UserEquip,
    pub characters: Vec<CharacterInfo>,
    pub caddies: Vec<CaddieInfo>,
    pub warehouse: Vec<WarehouseItem>,
    pub mascots: Vec<MascotInfo>,
    pub clubset_info: ClubSetInfo,
    /// Account sex (0=male, 1=female). Used for the `state_flag` gender bit in
    /// `0x48` PlayerRoomInfo.
    pub sex: i16,
}

/// The outcome of a game-server login attempt.
#[derive(Debug)]
pub enum GameLoginOutcome {
    /// Login accepted: the ack body + the channel-list body to send, plus the
    /// loaded player equipment so the `0x000B` (change item) handler can answer
    /// any item-type request from real state.
    Accepted {
        uid: i64,
        nickname: String,
        bodies: Vec<Vec<u8>>,
        // Boxed: PlayerEquipment holds several Vecs and is ~272 bytes; boxing
        // keeps this enum's footprint close to the small `Denied` variant.
        equipment: Box<PlayerEquipment>,
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

    // Load the player's full equipment from the DB. All loaders fall back to
    // empty/default values when their table is absent (dev safety), so a missing
    // migration never breaks login.
    let characters = repos::characters(pool, req.uid).await?;
    let equip = repos::user_equip(pool, req.uid).await?;
    let caddies = repos::caddies(pool, req.uid).await?;
    let warehouse = repos::warehouse(pool, req.uid).await?;
    let mascots = repos::mascots(pool, req.uid).await?;
    let clubset_info = repos::clubset_info(pool, req.uid).await?;
    // Spendable balances (pang + cookie) for the principal + the shop.
    let user_info = repos::user_info(pool, req.uid).await?;
    // Load member info for the account sex (used for the 0x48 state_flag gender
    // bit). Falls back to 0 (male) if unavailable.
    let sex = repos::member_info(pool, req.uid)
        .await?
        .map(|m| m.sex)
        .unwrap_or(0);

    // The equipped character is the one referenced by UserEquip.character_id,
    // falling back to the first owned (mirrors C++ equipDefaultCharacter).
    let equipped: Option<&CharacterInfo> = (equip.character_id != 0)
        .then(|| characters.iter().find(|c| c.id == equip.character_id))
        .flatten()
        .or_else(|| characters.first());
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
    // hangs in "Loading...". All builders now serialize real DB-loaded data.
    Ok(GameLoginOutcome::Accepted {
        uid: identity.uid,
        nickname: identity.nickname.clone(),
        bodies: vec![
            game_resp::build_login_ack_d3(),
            // Full player info (0x44 option 0) — the client needs this before
            // it can function in the lobby. Both the equipped CharacterInfo and
            // the UserEquip are now serialized from real DB data.
            game_resp::build_player_info(
                &req.client_version,
                identity.uid,
                &identity.id,
                &identity.nickname,
                2048, // server property
                equipped,
                Some(&equip),
                Some(&clubset_info),
                user_info.pang,
            ),
            // Equipment cascade (mirrors sendCompleteData order).
            game_resp::build_character_list(&characters),
            game_resp::build_caddie_list(&caddies),
            game_resp::build_warehouse_list(&warehouse),
            game_resp::build_mascot_list(&mascots),
            game_resp::build_user_equip(&equip),
            // Cookie (cash) balance — sent separately from the principal.
            game_resp::build_cookie(user_info.cookie),
            game_resp::build_channel_list(&channel_wires),
        ],
        equipment: Box::new(PlayerEquipment {
            equip,
            characters,
            caddies,
            warehouse,
            mascots,
            clubset_info,
            sex: sex.into(),
        }),
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
