-- Character information table.
--
-- Ports `pangya_character_information` from reference-cpp/bk-squema-mysql.sql (47 columns).
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
  -- Per-slot part instance ids (the warehouse item_id of the equipped part, or
  -- 0 for a built-in default part). The client only counts a part toward the
  -- character's displayed stats when its instance id is non-zero AND its typeid
  -- resolves in Part.iff (see CharacterInfo::getSlotOfStatsFromCharEquipedPartItem
  -- in pangya_st.h). `parts_N` holds the part typeid (IFF lookup key); the
  -- matching `parts_id_N` holds the instance id (the stat gate). The C++ schema
  -- folds both into one `parts_N` column resolved via a warehouse JOIN; we store
  -- them explicitly so multi-slot items (e.g. hair occupying two slots from one
  -- warehouse row) round-trip byte-exactly.
  `parts_id_1` int NOT NULL DEFAULT 0,
  `parts_id_2` int NOT NULL DEFAULT 0,
  `parts_id_3` int NOT NULL DEFAULT 0,
  `parts_id_4` int NOT NULL DEFAULT 0,
  `parts_id_5` int NOT NULL DEFAULT 0,
  `parts_id_6` int NOT NULL DEFAULT 0,
  `parts_id_7` int NOT NULL DEFAULT 0,
  `parts_id_8` int NOT NULL DEFAULT 0,
  `parts_id_9` int NOT NULL DEFAULT 0,
  `parts_id_10` int NOT NULL DEFAULT 0,
  `parts_id_11` int NOT NULL DEFAULT 0,
  `parts_id_12` int NOT NULL DEFAULT 0,
  `parts_id_13` int NOT NULL DEFAULT 0,
  `parts_id_14` int NOT NULL DEFAULT 0,
  `parts_id_15` int NOT NULL DEFAULT 0,
  `parts_id_16` int NOT NULL DEFAULT 0,
  `parts_id_17` int NOT NULL DEFAULT 0,
  `parts_id_18` int NOT NULL DEFAULT 0,
  `parts_id_19` int NOT NULL DEFAULT 0,
  `parts_id_20` int NOT NULL DEFAULT 0,
  `parts_id_21` int NOT NULL DEFAULT 0,
  `parts_id_22` int NOT NULL DEFAULT 0,
  `parts_id_23` int NOT NULL DEFAULT 0,
  `parts_id_24` int NOT NULL DEFAULT 0,
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

-- Seed the equipped character for the test account (UID 1): Erika
-- (typeid 0x04000001), the JP beginner female character.
--
-- The displayed power/control/accuracy/spin/curve bars are computed by the
-- client, NOT from PCL (it is ignored): the bulk comes from the equipped
-- CLUBSET's base stats (ClubSet.iff — the beginner Air Knight set is 8/9/8/3/3,
-- equipped in 0003), and each equipped part with a non-zero instance id
-- (parts_id_N) adds its Part.iff stat slots on top. Parts with parts_id = 0 are
-- the character's default cosmetic outfit (zero stats). Layout (parts_N column =
-- slot N-1):
--   slot 0 (parts_1):  0x08040800 id 11349   equipped, spin+1
--   slot 1 (parts_2):  0x08042400            default (cosmetic)
--   slot 2 (parts_3):  0x08044006 id 11350   equipped (head), power+1
--   slot 3 (parts_4):  0x08046800 id 11351   equipped (glove), curve+1
--   slot 4 (parts_5):  0x08048400            default (cosmetic)
--   slot 6 (parts_7):  0x0804C400            default (cosmetic)
--   slot 7 (parts_8):  0x0804E004 id 11352   equipped, accuracy+1
-- The equipped warehouse rows (11349..11352) are seeded in 0003.
INSERT INTO `pangya_character_information`
  (`item_id`, `typeid`, `UID`,
   `parts_1`,  `parts_2`,  `parts_3`,  `parts_4`,  `parts_5`,  `parts_7`,  `parts_8`,
   `parts_id_1`, `parts_id_3`, `parts_id_4`, `parts_id_8`)
VALUES
  (1, 0x04000001, 1,
   0x08040800, 0x08042400, 0x08044006, 0x08046800, 0x08048400, 0x0804C400, 0x0804E004,
   11349, 11350, 11351, 11352);
