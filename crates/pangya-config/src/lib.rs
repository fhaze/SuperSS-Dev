//! Typed configuration for Pangya servers.
//!
//! Replaces the C++ `ReaderIni` layer (which on Linux used GLib `GKeyFile`).
//! Each `server.ini` section maps to a typed struct, parsed from a plain INI
//! file via the `rust-ini` crate.
//!
//! The original `server.ini` files come in several variants (Auth, Login, Game,
//! Rank, Message). They share a common `[SERVERINFO]` / `[OPTION]` / `[LOG]` /
//! `[NORMAL_DB]` layout, and the server subclasses add `[AUTHSERVER]`,
//! `[GGAUTHSERVER]` and (Game Server only) `[CHANNELINFO]` + `[CHANNELn]`.

mod error;

pub use error::ConfigError;

use std::path::Path;

use ini::Ini;

// ─────────────────────────────────────────────────────────────────────────────
// Shared structs
// ─────────────────────────────────────────────────────────────────────────────

/// The `[SERVERINFO]` block. `tipo` (server type) is set per-binary at startup,
/// not read from the ini — matches the C++ behaviour where each subclass sets
/// `m_si.tipo` in its own `config_init`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerInfo {
    pub version: String,
    pub version_client: String,
    pub name: String,
    /// Server UID (Auth=80808, Login=10103, Game=20203, Rank=4774, Message=30303,
    /// GG-Auth=90909). Used for addressing in inter-server relay.
    pub guid: u32,
    pub ip: String,
    pub port: u16,
    pub max_user: u32,
    /// Bitfield; meaning varies per server (see server.ini comments in the C++ repo).
    pub property: u32,
    /// Game Server only: ICONINDEX → img_no. `None` when absent.
    pub icon_index: Option<u16>,
    /// Game Server only: FLAG (64-bit capability flag).
    pub flag: Option<u64>,
}

/// The `[OPTION]` block.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Options {
    /// Login Server: allow logging in with the same ID. Other servers ignore this.
    pub same_id_login: bool,
    /// Login Server: allow account creation on the fly.
    pub create_user: bool,
    /// Login Server: 1 = GM + registered IP only, 0 = everyone.
    pub access_flag: bool,
    /// Time-to-live (ms) before dropping an unresponsive client. 0 disables.
    pub ttl_ms: u32,
    /// Game Server: TTL (ms) for anti-bot detection. 0 disables.
    pub anti_bot_ttl_ms: u32,
}

/// The `[LOG]` block.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LogConfig {
    pub dir: String,
}

/// The `[NORMAL_DB]` block. Mirrors the C++ `ctx_db` struct.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DbConfig {
    pub engine: DbEngine,
    pub ip: String,
    pub name: String,
    pub user: String,
    pub pass: String,
    /// ODBC (MSSQL/Postgres) historically ignored the port; default to the
    /// engine's standard port when absent.
    pub port: u16,
}

impl DbConfig {
    /// Build a `mysql://user:pass@ip:port/name` connect URL for sqlx.
    pub fn mysql_url(&self) -> String {
        format!(
            "mysql://{}:{}@{}:{}/{}",
            self.user, self.pass, self.ip, self.port, self.name
        )
    }
}

/// Which backend to use. Selected from the `DBENGINE` ini key
/// (case-insensitive). The C++ default was MSSQL.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DbEngine {
    #[default]
    Mssql,
    Mysql,
    Postgres,
}

impl DbEngine {
    fn parse(raw: &str) -> Self {
        match raw.trim().to_ascii_lowercase().as_str() {
            "mysql" => Self::Mysql,
            "postgresql" | "postgres" => Self::Postgres,
            _ => Self::Mssql,
        }
    }

    pub fn default_port(self) -> u16 {
        match self {
            Self::Mysql => 3306,
            Self::Mssql => 1433,
            Self::Postgres => 5432,
        }
    }
}

/// The `[AUTHSERVER]` block — where the other servers dial the hub.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthLinkConfig {
    pub ip: String,
    pub port: u16,
}

/// The `[GGAUTHSERVER]` block — Game Server only.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GgAuthLinkConfig {
    pub ip: String,
    pub port: u16,
}

/// Game Server rate config (from `[SERVERINFO]`, optionally overridden from DB).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rates {
    pub exp: u32,
    pub pang: u32,
    pub club_mastery: u32,
}

/// One channel, parsed from a `[CHANNELn]` section (Game Server only).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChannelConfig {
    pub name: String,
    pub max_user: u32,
    pub max_level: u32,
    pub low_level: Option<u32>,
    pub flag: u32,
}

/// The `[CHANNELINFO]` block + the parsed `[CHANNELn]` entries.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChannelInfo {
    pub num_channels: u32,
    pub channels: Vec<ChannelConfig>,
}

// ─────────────────────────────────────────────────────────────────────────────
// The assembled config
// ─────────────────────────────────────────────────────────────────────────────

/// A fully parsed `server.ini`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerConfig {
    pub server: ServerInfo,
    pub options: Options,
    pub log: LogConfig,
    pub db: DbConfig,
    pub auth: Option<AuthLinkConfig>,
    pub gg_auth: Option<GgAuthLinkConfig>,
    /// Game Server only.
    pub rates: Option<Rates>,
    /// Game Server only.
    pub game_guard_auth: bool,
    /// Game Server only.
    pub channels: Option<ChannelInfo>,
}

impl ServerConfig {
    /// Parse a `server.ini` from disk.
    pub fn load<P: AsRef<Path>>(path: P) -> Result<Self, ConfigError> {
        let ini = Ini::load_from_file(path).map_err(ConfigError::IniRead)?;
        Self::from_ini(&ini)
    }

    /// Parse from an already-loaded INI document. Useful for tests.
    pub fn from_ini(ini: &Ini) -> Result<Self, ConfigError> {
        let server = Self::parse_server_info(ini)?;
        let options = Self::parse_options(ini);
        let log = Self::parse_log(ini);
        let db = Self::parse_db(ini)?;
        let auth = Self::parse_auth(ini);
        let gg_auth = Self::parse_gg_auth(ini);
        let rates = Self::parse_rates(ini);
        let game_guard_auth = Self::parse_game_guard_auth(ini);
        let channels = Self::parse_channels(ini)?;

        Ok(Self {
            server,
            options,
            log,
            db,
            auth,
            gg_auth,
            rates,
            game_guard_auth,
            channels,
        })
    }

    // ── section parsers ──────────────────────────────────────────────────────

    fn section<'a>(ini: &'a Ini, name: &str) -> Option<&'a ini::Properties> {
        ini.section(Some(name))
    }

    fn get<'a>(props: Option<&'a ini::Properties>, key: &str) -> Option<&'a str> {
        props.and_then(|p| p.get(key))
    }

    /// Strip surrounding double-quotes that the C++ inis sometimes use.
    fn unquote(s: &str) -> &str {
        let t = s.trim();
        if t.len() >= 2 && t.starts_with('"') && t.ends_with('"') {
            &t[1..t.len() - 1]
        } else {
            t
        }
    }

    fn parse_server_info(ini: &Ini) -> Result<ServerInfo, ConfigError> {
        let s = Self::section(ini, "SERVERINFO")
            .ok_or_else(|| ConfigError::MissingSection("SERVERINFO".into()))?;
        let need = |k: &str| -> Result<&str, ConfigError> {
            Self::get(Some(s), k)
                .ok_or_else(|| ConfigError::MissingKey("SERVERINFO".into(), k.into()))
                .map(Self::unquote)
        };

        Ok(ServerInfo {
            version: need("VERSION")?.to_owned(),
            version_client: need("CLIENTVERSION")?.to_owned(),
            name: need("NAME")?.to_owned(),
            guid: need("GUID")?
                .parse()
                .map_err(|_| ConfigError::BadValue("SERVERINFO".into(), "GUID".into()))?,
            ip: need("IP")?.to_owned(),
            port: need("PORT")?
                .parse()
                .map_err(|_| ConfigError::BadValue("SERVERINFO".into(), "PORT".into()))?,
            max_user: Self::get(Some(s), "MAXUSER")
                .ok_or_else(|| ConfigError::MissingKey("SERVERINFO".into(), "MAXUSER".into()))?
                .parse()
                .map_err(|_| ConfigError::BadValue("SERVERINFO".into(), "MAXUSER".into()))?,
            property: Self::get(Some(s), "PROPERTY")
                .unwrap_or("0")
                .parse()
                .unwrap_or(0),
            icon_index: Self::get(Some(s), "ICONINDEX").map(|v| v.parse().unwrap_or(0)),
            flag: Self::get(Some(s), "FLAG").map(|v| v.parse().unwrap_or(0)),
        })
    }

    fn parse_options(ini: &Ini) -> Options {
        let s = match Self::section(ini, "OPTION") {
            Some(s) => s,
            None => return Options::default(),
        };
        let bool_ = |k: &str| {
            Self::get(Some(s), k)
                .map(|v| v.trim() == "1")
                .unwrap_or(false)
        };
        Options {
            same_id_login: bool_("SAME_ID_LOGIN"),
            create_user: bool_("CREATEUSER"),
            access_flag: bool_("ACCESSFLAG"),
            ttl_ms: Self::get(Some(s), "TTL")
                .map(|v| v.trim().parse().unwrap_or(0))
                .unwrap_or(0),
            anti_bot_ttl_ms: Self::get(Some(s), "ANTIBOTTTL")
                .map(|v| v.trim().parse().unwrap_or(0))
                .unwrap_or(0),
        }
    }

    fn parse_log(ini: &Ini) -> LogConfig {
        let dir = Self::section(ini, "LOG")
            .and_then(|s| s.get("DIR"))
            .map(|v| Self::unquote(v).to_owned())
            .unwrap_or_else(|| "Log".to_owned());
        LogConfig { dir }
    }

    fn parse_db(ini: &Ini) -> Result<DbConfig, ConfigError> {
        let s = Self::section(ini, "NORMAL_DB")
            .ok_or_else(|| ConfigError::MissingSection("NORMAL_DB".into()))?;
        let engine_raw = Self::get(Some(s), "DBENGINE").unwrap_or("mssql");
        let engine = DbEngine::parse(engine_raw);
        let port = Self::get(Some(s), "DBPORT")
            .map(|v| {
                Self::unquote(v)
                    .parse()
                    .unwrap_or_else(|_| engine.default_port())
            })
            .unwrap_or_else(|| engine.default_port());
        Ok(DbConfig {
            engine,
            ip: Self::get(Some(s), "DBIP")
                .map(Self::unquote)
                .unwrap_or("localhost")
                .to_owned(),
            name: Self::get(Some(s), "DBNAME")
                .map(Self::unquote)
                .unwrap_or("pangya")
                .to_owned(),
            user: Self::get(Some(s), "DBUSER")
                .map(Self::unquote)
                .unwrap_or("pangya")
                .to_owned(),
            pass: Self::get(Some(s), "DBPASS")
                .map(Self::unquote)
                .unwrap_or("pangya")
                .to_owned(),
            port,
        })
    }

    fn parse_auth(ini: &Ini) -> Option<AuthLinkConfig> {
        let s = Self::section(ini, "AUTHSERVER")?;
        let ip = Self::get(Some(s), "IP").map(Self::unquote)?.to_owned();
        let port = Self::get(Some(s), "PORT")?.trim().parse().ok()?;
        Some(AuthLinkConfig { ip, port })
    }

    fn parse_gg_auth(ini: &Ini) -> Option<GgAuthLinkConfig> {
        let s = Self::section(ini, "GGAUTHSERVER")?;
        let ip = Self::get(Some(s), "IP").map(Self::unquote)?.to_owned();
        let port = Self::get(Some(s), "PORT")?.trim().parse().ok()?;
        Some(GgAuthLinkConfig { ip, port })
    }

    fn parse_rates(ini: &Ini) -> Option<Rates> {
        let s = Self::section(ini, "SERVERINFO")?;
        let exp = Self::get(Some(s), "EXPRATE")?.trim().parse().ok()?;
        let pang = Self::get(Some(s), "PANGRATE")?.trim().parse().ok()?;
        let club_mastery = Self::get(Some(s), "CLUBMASTERYRATE")?.trim().parse().ok()?;
        Some(Rates {
            exp,
            pang,
            club_mastery,
        })
    }

    fn parse_game_guard_auth(ini: &Ini) -> bool {
        Self::section(ini, "SERVERINFO")
            .and_then(|s| s.get("GAMEGUARDAUTH"))
            .map(|v| v.trim() == "1")
            .unwrap_or(false)
    }

    fn parse_channels(ini: &Ini) -> Result<Option<ChannelInfo>, ConfigError> {
        let s = match Self::section(ini, "CHANNELINFO") {
            Some(s) => s,
            None => return Ok(None),
        };
        let num: u32 = Self::get(Some(s), "NUM_CHANNEL")
            .ok_or_else(|| ConfigError::MissingKey("CHANNELINFO".into(), "NUM_CHANNEL".into()))?
            .trim()
            .parse()
            .map_err(|_| ConfigError::BadValue("CHANNELINFO".into(), "NUM_CHANNEL".into()))?;

        let mut channels = Vec::with_capacity(num as usize);
        for i in 1..=num {
            let key = format!("CHANNEL{}", i);
            let c =
                Self::section(ini, &key).ok_or_else(|| ConfigError::MissingSection(key.clone()))?;
            let u32_ = |k: &str| {
                Self::get(Some(c), k)
                    .ok_or_else(|| ConfigError::MissingKey(key.clone(), k.into()))?
                    .trim()
                    .parse::<u32>()
                    .map_err(|_| ConfigError::BadValue(key.clone(), k.into()))
            };
            channels.push(ChannelConfig {
                name: Self::get(Some(c), "NAME")
                    .map(Self::unquote)
                    .unwrap_or("Channel")
                    .to_owned(),
                max_user: u32_("MAXUSER")?,
                max_level: u32_("MAXLEVEL")?,
                low_level: Self::get(Some(c), "LOWLEVEL").map(|v| v.trim().parse().unwrap_or(0)),
                flag: Self::get(Some(c), "FLAG")
                    .map(|v| v.trim().parse().unwrap_or(0))
                    .unwrap_or(0),
            });
        }
        Ok(Some(ChannelInfo {
            num_channels: num,
            channels,
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn auth_ini() -> &'static str {
        include_str!("../tests/fixtures/server-auth.ini")
    }

    fn game_ini() -> &'static str {
        include_str!("../tests/fixtures/server-game.ini")
    }

    #[test]
    fn parses_auth_server_ini() {
        let ini = Ini::load_from_str(auth_ini()).unwrap();
        let cfg = ServerConfig::from_ini(&ini).unwrap();
        assert_eq!(cfg.server.guid, 80808);
        assert_eq!(cfg.server.port, 7777);
        assert_eq!(cfg.server.name, "Auth Server");
        assert_eq!(cfg.db.engine, DbEngine::Mssql);
        assert!(cfg.auth.is_none(), "Auth Server has no [AUTHSERVER] block");
        assert!(cfg.channels.is_none());
        assert_eq!(cfg.options.ttl_ms, 0);
    }

    #[test]
    fn parses_login_server_ini() {
        let ini = Ini::load_from_str(include_str!("../tests/fixtures/server-login.ini")).unwrap();
        let cfg = ServerConfig::from_ini(&ini).unwrap();
        assert_eq!(cfg.server.guid, 10103);
        assert_eq!(cfg.server.port, 10303);
        assert_eq!(cfg.server.name, "Login Server");
        assert!(cfg.options.same_id_login);
        assert!(cfg.options.create_user);
        assert!(!cfg.options.access_flag);
        assert_eq!(cfg.options.ttl_ms, 60000);
        let auth = cfg.auth.expect("login server dials auth");
        assert_eq!(auth.ip, "127.0.0.1");
        assert_eq!(auth.port, 7777);
    }

    #[test]
    fn parses_game_server_ini_with_channels() {
        let ini = Ini::load_from_str(game_ini()).unwrap();
        let cfg = ServerConfig::from_ini(&ini).unwrap();
        assert_eq!(cfg.server.guid, 20203);
        assert_eq!(cfg.server.name, "Tittan Boo");
        let rates = cfg.rates.expect("game server has rates");
        assert_eq!(rates.exp, 100);
        assert_eq!(rates.pang, 100);
        assert!(cfg.game_guard_auth);
        let gg = cfg.gg_auth.expect("game server has gg auth");
        assert_eq!(gg.port, 7788);
        let ch = cfg.channels.expect("game server has channels");
        assert_eq!(ch.num_channels, 4);
        assert_eq!(ch.channels.len(), 4);
        assert_eq!(ch.channels[0].name, "Channel (Beginners)");
        assert_eq!(ch.channels[0].flag, 512);
        assert_eq!(ch.channels[0].max_level, 16);
        assert_eq!(ch.channels[1].name, "Channel (Free 1)");
        assert_eq!(ch.channels[1].max_level, 70);
    }

    #[test]
    fn mysql_url_is_well_formed() {
        let ini = Ini::load_from_str(include_str!("../tests/fixtures/server-game.ini")).unwrap();
        let cfg = ServerConfig::from_ini(&ini).unwrap();
        // The game fixture overrides DBENGINE to mysql.
        assert_eq!(cfg.db.engine, DbEngine::Mysql);
        assert_eq!(
            cfg.db.mysql_url(),
            "mysql://pangya:pangya@localhost:3306/pangya"
        );
    }

    #[test]
    fn missing_section_is_an_error() {
        // A complete [SERVERINFO] so parsing proceeds past it and fails on the
        // absent [NORMAL_DB] section.
        let ini = Ini::load_from_str(
            "[SERVERINFO]\n\
             VERSION=v\nCLIENTVERSION=cv\nNAME=x\nGUID=1\nIP=0.0.0.0\nPORT=2\nMAXUSER=3\n",
        )
        .unwrap();
        let err = ServerConfig::from_ini(&ini).unwrap_err();
        assert!(
            matches!(err, ConfigError::MissingSection(ref s) if s == "NORMAL_DB"),
            "got {err:?}"
        );
    }
}
