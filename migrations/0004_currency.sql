-- Player currency: pang and cookie (cash) balances.
--
-- The original server keeps these in the in-memory UserInfo (pang) and PlayerInfo
-- (cookie), loaded from the user-info tables. We persist just the two balances
-- the item shop needs; the full UserInfo stat block can fold in here later.
--
-- pang   -> serialized into the 0x44 principal at UserInfo+79 (so the lobby shows it)
-- cookie -> sent in the separate 0x96 packet
-- Both are deducted on a shop purchase (0x1D).

CREATE TABLE `pangya_player_currency` (
  `UID` int NOT NULL,
  `pang` bigint unsigned NOT NULL DEFAULT 0,
  `cookie` bigint unsigned NOT NULL DEFAULT 0,
  PRIMARY KEY (`UID`)
) ENGINE=InnoDB DEFAULT CHARSET=latin1;

-- Seed the test account (UID 1) with a healthy balance so it can shop.
INSERT INTO `pangya_player_currency` (`UID`, `pang`, `cookie`)
VALUES (1, 10000000, 100000);
