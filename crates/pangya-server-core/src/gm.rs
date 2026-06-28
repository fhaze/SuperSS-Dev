//! GM command system — chat-prefixed admin commands.
//!
//! The Pangya client sends lobby chat as `0x03`; GMs prefix commands with `/`.
//! This is a clean place to add server-management features, mirroring the C++
//! `checkCommand` console + Discord-CMD broadcast path. New commands register
//! here without touching the packet plumbing.
//!
//! Supported commands (Milestone 1 subset):
//! - `/notice <msg>`       — broadcast a server notice (`0x40` option 7) to all.
//! - `/kick <uid>`         — disconnect a player by UID (stub: logs only).
//! - `/say <msg>`          — GM chat message (option 0x80).

use pangya_proto::game_resp;

/// The result of interpreting a chat message as a potential GM command.
#[derive(Debug, Clone)]
pub enum GmResult {
    /// The message was not a command; treat as normal chat.
    NotACommand,
    /// The command produced a broadcast body (e.g. a notice or GM chat).
    Broadcast(Vec<u8>),
    /// The command was handled but produced no broadcast (e.g. kick).
    Handled,
    /// The command was unrecognized or malformed.
    Invalid(String),
}

/// Parse and execute a GM command if `message` starts with `/`.
///
/// `gm_nickname` is the GM's display name (used as the source for notices).
/// Returns [`GmResult::NotACommand`] for normal chat.
pub fn try_gm_command(gm_nickname: &str, message: &str) -> GmResult {
    let Some(rest) = message.strip_prefix('/') else {
        return GmResult::NotACommand;
    };
    let mut parts = rest.splitn(2, char::is_whitespace);
    let cmd = parts.next().unwrap_or("").to_ascii_lowercase();
    let arg = parts.next().unwrap_or("").trim();

    match cmd.as_str() {
        "notice" | "broadcast" => {
            if arg.is_empty() {
                return GmResult::Invalid("usage: /notice <message>".into());
            }
            GmResult::Broadcast(game_resp::build_notice(gm_nickname, arg))
        }
        "say" => {
            if arg.is_empty() {
                return GmResult::Invalid("usage: /say <message>".into());
            }
            GmResult::Broadcast(game_resp::build_chat(0x80, gm_nickname, arg))
        }
        "kick" => {
            if arg.is_empty() {
                return GmResult::Invalid("usage: /kick <uid>".into());
            }
            match arg.parse::<i64>() {
                Ok(_uid) => {
                    // TODO: wire to the session manager to drop the connection.
                    GmResult::Handled
                }
                Err(_) => GmResult::Invalid(format!("'{arg}' is not a valid UID")),
            }
        }
        "help" => GmResult::Broadcast(game_resp::build_notice(
            "Server",
            "Commands: /notice <msg>, /say <msg>, /kick <uid>",
        )),
        other => GmResult::Invalid(format!("unknown command: /{other}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normal_chat_is_not_a_command() {
        assert!(matches!(
            try_gm_command("gm", "hello there"),
            GmResult::NotACommand
        ));
    }

    #[test]
    fn notice_command_produces_broadcast() {
        match try_gm_command("Admin", "/notice Server restarting soon") {
            GmResult::Broadcast(body) => {
                assert_eq!(body[0..2], [0x40, 0x00]);
                assert_eq!(body[2], 7); // notice option
            }
            other => panic!("expected broadcast, got {other:?}"),
        }
    }

    #[test]
    fn say_command_uses_gm_flag() {
        match try_gm_command("Admin", "/say hi") {
            GmResult::Broadcast(body) => assert_eq!(body[2], 0x80),
            other => panic!("expected broadcast, got {other:?}"),
        }
    }

    #[test]
    fn empty_notice_is_invalid() {
        assert!(matches!(
            try_gm_command("Admin", "/notice"),
            GmResult::Invalid(_)
        ));
    }

    #[test]
    fn unknown_command_is_invalid() {
        assert!(matches!(
            try_gm_command("Admin", "/frobnicate x"),
            GmResult::Invalid(_)
        ));
    }

    #[test]
    fn kick_parses_uid() {
        assert!(matches!(
            try_gm_command("Admin", "/kick 12345"),
            GmResult::Handled
        ));
        assert!(matches!(
            try_gm_command("Admin", "/kick abc"),
            GmResult::Invalid(_)
        ));
    }
}
