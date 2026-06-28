//! Rank Server — leaderboards and rankings. Dials the Auth Server.
//! Built out in Phase 7.

use anyhow::Result;
use pangya_config::ServerConfig;
use tracing::{info, Level};

const LOG_PREFIX: &str = "RS";

fn main() -> Result<()> {
    tracing_subscriber::fmt().with_max_level(Level::INFO).init();

    let cfg = ServerConfig::load("server.ini")
        .map_err(|e| anyhow::anyhow!("failed to load server.ini: {e}"))?;

    info!(
        "[{}] Rank Server starting up — guid={}, port={}",
        LOG_PREFIX, cfg.server.guid, cfg.server.port
    );
    info!(
        "[{}] Phase 0 scaffolding: config parsed. Logic lands in Phase 7.",
        LOG_PREFIX
    );
    Ok(())
}
