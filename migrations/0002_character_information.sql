-- Character information table.
--
-- Ports `pangya_character_information` from bk-squema-mysql.sql (47 columns).
-- One row per character instance a player owns; the equipped character is the
-- one referenced by UserEquip.character_id. This is what the 0x0044 principal
-- packet and the 0x004B (change item, type 4) response serialize as the
-- 513-byte CharacterInfo struct (pangya_st.h:389).
--
-- Column names are preserved verbatim from the original schema.

CREATE TABLE `pangya_character_information` (
  `item_id` bigint NOT NULL AUTO_INCREMENT,
  `typeid` int NOT NULL,
  `UID` int NOT NULL,
  `parts_1` int NOT NULL DEFAULT 0,
  `parts_2` int NOT NULL DEFAULT 0,
  `parts_3` int NOT NULL DEFAULT 0,
  `parts_4` int NOT NULL DEFAULT 0,
  `parts_5` int NOT NULL DEFAULT 0,
  `parts_6` int NOT NULL DEFAULT 0,
  `parts_7` int NOT NULL DEFAULT 0,
  `parts_8` int NOT NULL DEFAULT 0,
  `parts_9` int NOT NULL DEFAULT 0,
  `parts_10` int NOT NULL DEFAULT 0,
  `parts_11` int NOT NULL DEFAULT 0,
  `parts_12` int NOT NULL DEFAULT 0,
  `parts_13` int NOT NULL DEFAULT 0,
  `parts_14` int NOT NULL DEFAULT 0,
  `parts_15` int NOT NULL DEFAULT 0,
  `parts_16` int NOT NULL DEFAULT 0,
  `parts_17` int NOT NULL DEFAULT 0,
  `parts_18` int NOT NULL DEFAULT 0,
  `parts_19` int NOT NULL DEFAULT 0,
  `parts_20` int NOT NULL DEFAULT 0,
  `parts_21` int NOT NULL DEFAULT 0,
  `parts_22` int NOT NULL DEFAULT 0,
  `parts_23` int NOT NULL DEFAULT 0,
  `parts_24` int NOT NULL DEFAULT 0,
  `default_hair` smallint NOT NULL DEFAULT 0,
  `default_shirts` smallint NOT NULL DEFAULT 0,
  `gift_flag` smallint NOT NULL DEFAULT 0,
  `PCL0` smallint NOT NULL DEFAULT 0,
  `PCL1` smallint NOT NULL DEFAULT 0,
  `PCL2` smallint NOT NULL DEFAULT 0,
  `PCL3` smallint NOT NULL DEFAULT 0,
  `PCL4` smallint NOT NULL DEFAULT 0,
  `Purchase` smallint NOT NULL DEFAULT 0,
  `auxparts_1` int NOT NULL DEFAULT 0,
  `auxparts_2` int NOT NULL DEFAULT 0,
  `auxparts_3` int NOT NULL DEFAULT 0,
  `auxparts_4` int NOT NULL DEFAULT 0,
  `auxparts_5` int NOT NULL DEFAULT 0,
  `CutIn_1` int NOT NULL DEFAULT 0,
  `CutIn_2` int NOT NULL DEFAULT 0,
  `CutIn_3` int NOT NULL DEFAULT 0,
  `CutIn_4` int NOT NULL DEFAULT 0,
  `Mastery` int NOT NULL DEFAULT 0,
  PRIMARY KEY (`item_id`),
  KEY `idx_charinfo_uid` (`UID`)
) ENGINE=InnoDB DEFAULT CHARSET=latin1;

-- Seed the default test character for the test account (UID 1): Erika
-- (typeid 0x04000001), the JP beginner female character. PCL stats
-- (power/control/accuracy/spin/curve = 9/11/6/2/2) are taken verbatim from
-- pangya_jp.iff → Character.iff.
INSERT INTO `pangya_character_information`
  (`typeid`, `UID`, `PCL0`, `PCL1`, `PCL2`, `PCL3`, `PCL4`)
VALUES
  (0x04000001, 1, 9, 11, 6, 2, 2);
