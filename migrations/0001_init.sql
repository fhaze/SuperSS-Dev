-- pangya-server initial schema migration.
--
-- Source: reference-cpp/bk-squema-mysql.sql from the original SuperSS-Dev C++ repo.
-- Phase 0 establishes the migration framework plus the core tables that
-- Phases 3–5 (Auth / Login / Game-lobby) require. The remaining ~170 tables
-- (characters, items, caddies, mascots, cards, warehouse, mail, guilds, quests,
-- trophies, shop systems, etc.) are ported incrementally in Phase 2 as each
-- system is touched, with logic migrated out of stored procedures into the
-- Rust repo layer.
--
-- Column names are preserved verbatim from the original schema so existing
-- client data dumps remain compatible.

-- ── account ─────────────────────────────────────────────────────────────────
CREATE TABLE `account` (
  `ID` varchar(25) NOT NULL,
  `UID` bigint NOT NULL AUTO_INCREMENT,
  `PASSWORD` varchar(33) NOT NULL,
  `IDState` bigint NOT NULL DEFAULT 0,
  `LastLogonTime` datetime DEFAULT NULL,
  `BlockTime` int NOT NULL DEFAULT 0,
  `Logon` smallint NOT NULL DEFAULT 0,
  `FIRST_LOGIN` smallint NOT NULL DEFAULT 0,
  `RegDate` datetime DEFAULT NULL,
  `NICK` varchar(50) NOT NULL,
  `FIRST_SET` smallint NOT NULL DEFAULT 0,
  `Guild_UID` int NOT NULL DEFAULT 0,
  `Sex` smallint NOT NULL DEFAULT 0,
  `doTutorial` smallint NOT NULL DEFAULT 0,
  `UserName` varchar(23) DEFAULT NULL,
  `UserIp` varchar(20) DEFAULT NULL,
  `ServerID` varchar(20) DEFAULT NULL,
  `game_server_id` varchar(20) DEFAULT NULL,
  `LastLeaveTime` datetime DEFAULT NULL,
  `LogonCount` bigint NOT NULL DEFAULT 0,
  `BlockRegDate` datetime DEFAULT NULL,
  `School` int NOT NULL DEFAULT 0,
  `capability` int NOT NULL DEFAULT 0,
  `Event` smallint NOT NULL DEFAULT 0,
  `MannerFlag` smallint NOT NULL DEFAULT 0,
  `Event1` smallint NOT NULL DEFAULT 0,
  `Event2` int NOT NULL DEFAULT 0,
  `domainid` int NOT NULL DEFAULT 0,
  `ChannelFlag` smallint NOT NULL DEFAULT 0,
  `change_nick` datetime DEFAULT NULL,
  PRIMARY KEY (`UID`),
  UNIQUE KEY `uk_account_id` (`ID`),
  KEY `idx_account_nick` (`NICK`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4;

-- ── auth keys ───────────────────────────────────────────────────────────────
-- Minted by Login Server on successful login (login) and by Game Server on
-- enter (game). The client presents the login key to the Game Server, which
-- validates against authkey_game.
CREATE TABLE `authkey_login` (
  `UID` int NOT NULL,
  `AuthKey` varchar(8) NOT NULL,
  `valid` smallint NOT NULL DEFAULT 1,
  PRIMARY KEY (`UID`, `AuthKey`)
) ENGINE=InnoDB DEFAULT CHARSET=latin1;

CREATE TABLE `authkey_game` (
  `UID` int NOT NULL,
  `AuthKey` varchar(8) NOT NULL,
  `valid` smallint NOT NULL DEFAULT 1,
  PRIMARY KEY (`UID`, `AuthKey`)
) ENGINE=InnoDB DEFAULT CHARSET=latin1;

-- ── server registry ────────────────────────────────────────────────────────
-- Written by every server's register heartbeat; read by Login Server to build
-- the channel list shown to clients. Mirrors ServerInfoEx in the C++ model.
CREATE TABLE `pangya_server_list` (
  `Name` varchar(50) NOT NULL,
  `UID` int NOT NULL,
  `IP` varchar(20) NOT NULL,
  `Port` int NOT NULL,
  `MaxUser` int NOT NULL,
  `CurrUser` int NOT NULL,
  `Type` smallint NOT NULL,            -- server tipo (Auth=5, Game=1, …)
  `UpdateTime` datetime NOT NULL,
  `State` smallint NOT NULL,
  `PCBangUser` smallint NOT NULL DEFAULT 0,
  `PangRate` int NOT NULL DEFAULT 100,
  `ServerVersion` varchar(40) NOT NULL,
  `ClientVersion` varchar(20) NOT NULL,
  `property` int NOT NULL DEFAULT 0,
  `AngelicWingsNum` int NOT NULL DEFAULT 0,
  `EventFlag` smallint NOT NULL DEFAULT 0,
  `ExpRate` int NOT NULL DEFAULT 100,
  `RareItemRate` int NOT NULL DEFAULT 100,
  `CookieItemRate` int NOT NULL DEFAULT 100,
  `ServiceControl` int NOT NULL DEFAULT 0,
  `ImgNo` smallint NOT NULL DEFAULT 0,
  `AppRate` smallint NOT NULL DEFAULT 0,
  `ScratchRate` smallint NOT NULL DEFAULT 0,
  PRIMARY KEY (`UID`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4;

-- ── scheduled GM commands / notices (Auth Server heartbeat) ────────────────
CREATE TABLE `pangya_command` (
  `id` bigint NOT NULL AUTO_INCREMENT,
  `COMMAND_ID` int NOT NULL,           -- broadcast notice / ticker / shutdown …
  `MSG` varchar(255) DEFAULT NULL,
  `OPTION` int NOT NULL DEFAULT 0,
  `TARGET_UID` int DEFAULT NULL,       -- server UID to deliver to (NULL = all)
  `created_at` datetime NOT NULL DEFAULT CURRENT_TIMESTAMP,
  `processed` tinyint NOT NULL DEFAULT 0,
  PRIMARY KEY (`id`),
  KEY `idx_command_pending` (`processed`, `created_at`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4;

-- ── IP / MAC bans (refreshed by server heartbeat) ──────────────────────────
CREATE TABLE `pangya_block_ip` (
  `ip` varchar(45) NOT NULL,
  PRIMARY KEY (`ip`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4;

CREATE TABLE `pangya_block_mac` (
  `mac` varchar(32) NOT NULL,
  PRIMARY KEY (`mac`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4;
