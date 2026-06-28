-- Equipment tables: user-equip slots, warehouse (owned items), caddies, mascots.
--
-- Ports `pangya_user_equip`, `pangya_item_warehouse`, `pangya_caddie_information`,
-- and `pangya_mascot_info` from reference-cpp/bk-squema-mysql.sql. These back the
-- equipment cascade (0x70/0x71/0x73/0xE1/0x72) and the UserEquip block in the
-- 0x0044 principal packet, replacing the hardcoded zero/empty placeholders.
--
-- Column names are preserved verbatim from the original schema.

-- ── user equipment slots (what is equipped, keyed by UID) ────────────────────
CREATE TABLE `pangya_user_equip` (
  `UID` int NOT NULL,
  `caddie_id` int NOT NULL DEFAULT 0,
  `character_id` int NOT NULL DEFAULT 0,
  `club_id` int NOT NULL DEFAULT 0,
  `ball_type` int NOT NULL DEFAULT 0,
  `item_slot_1` int NOT NULL DEFAULT 0,
  `item_slot_2` int NOT NULL DEFAULT 0,
  `item_slot_3` int NOT NULL DEFAULT 0,
  `item_slot_4` int NOT NULL DEFAULT 0,
  `item_slot_5` int NOT NULL DEFAULT 0,
  `item_slot_6` int NOT NULL DEFAULT 0,
  `item_slot_7` int NOT NULL DEFAULT 0,
  `item_slot_8` int NOT NULL DEFAULT 0,
  `item_slot_9` int NOT NULL DEFAULT 0,
  `item_slot_10` int NOT NULL DEFAULT 0,
  `Skin_1` int NOT NULL DEFAULT 0,
  `Skin_2` int NOT NULL DEFAULT 0,
  `Skin_3` int NOT NULL DEFAULT 0,
  `Skin_4` int NOT NULL DEFAULT 0,
  `Skin_5` int NOT NULL DEFAULT 0,
  `Skin_6` int NOT NULL DEFAULT 0,
  `mascot_id` int NOT NULL DEFAULT 0,
  `poster_1` int NOT NULL DEFAULT 0,
  `poster_2` int NOT NULL DEFAULT 0,
  PRIMARY KEY (`UID`)
) ENGINE=InnoDB DEFAULT CHARSET=latin1;

-- ── warehouse: owned items (parts, clubsets, balls, consumables, …) ──────────
-- The C++ WarehouseItem wire struct is 196 bytes (id, typeid, ano, c[5],
-- purchase, flag, apply/end dates, type, UCC, Card, ClubsetWorkshop). We persist
-- the core columns; the UCC/card sub-structs are zero-filled on the wire until
-- those features land.
CREATE TABLE `pangya_item_warehouse` (
  `item_id` bigint NOT NULL AUTO_INCREMENT,
  `UID` int NOT NULL,
  `typeid` int NOT NULL,
  `valid` smallint NOT NULL DEFAULT 1,
  `regdate` datetime DEFAULT NULL,
  `Gift_flag` smallint NOT NULL DEFAULT 0,
  `flag` smallint NOT NULL DEFAULT 0,
  `Applytime` datetime DEFAULT CURRENT_TIMESTAMP,
  `EndDate` datetime DEFAULT CURRENT_TIMESTAMP,
  `C0` smallint NOT NULL DEFAULT 0,
  `C1` smallint NOT NULL DEFAULT 0,
  `C2` smallint NOT NULL DEFAULT 0,
  `C3` smallint NOT NULL DEFAULT 0,
  `C4` smallint NOT NULL DEFAULT 0,
  `Purchase` smallint NOT NULL DEFAULT 0,
  `ItemType` smallint NOT NULL DEFAULT 2,
  `ClubSet_WorkShop_Flag` smallint NOT NULL DEFAULT 0,
  `ClubSet_WorkShop_C0` smallint NOT NULL DEFAULT 0,
  `ClubSet_WorkShop_C1` smallint NOT NULL DEFAULT 0,
  `ClubSet_WorkShop_C2` smallint NOT NULL DEFAULT 0,
  `ClubSet_WorkShop_C3` smallint NOT NULL DEFAULT 0,
  `ClubSet_WorkShop_C4` smallint NOT NULL DEFAULT 0,
  `Mastery_Pts` int NOT NULL DEFAULT 0,
  `Recovery_Pts` int NOT NULL DEFAULT 0,
  `Level` int NOT NULL DEFAULT 0,
  `Up` int NOT NULL DEFAULT 0,
  PRIMARY KEY (`item_id`),
  KEY `idx_warehouse_uid` (`UID`)
) ENGINE=InnoDB DEFAULT CHARSET=latin1;

-- ── caddies owned by the player ─────────────────────────────────────────────
CREATE TABLE `pangya_caddie_information` (
  `item_id` bigint NOT NULL AUTO_INCREMENT,
  `UID` int NOT NULL,
  `typeid` int NOT NULL,
  `parts_typeid` int NOT NULL DEFAULT 0,
  `gift_flag` smallint NOT NULL DEFAULT 0,
  `cLevel` smallint NOT NULL DEFAULT 0,
  `Exp` int NOT NULL DEFAULT 0,
  `RegDate` datetime NOT NULL DEFAULT CURRENT_TIMESTAMP,
  `Period` smallint NOT NULL DEFAULT 0,
  `EndDate` datetime DEFAULT NULL,
  `RentFlag` smallint NOT NULL DEFAULT 1,
  `Purchase` smallint NOT NULL DEFAULT 0,
  `parts_EndDate` datetime DEFAULT NULL,
  `CheckEnd` smallint NOT NULL DEFAULT 1,
  `Valid` smallint NOT NULL DEFAULT 1,
  PRIMARY KEY (`item_id`),
  KEY `idx_caddie_uid` (`UID`)
) ENGINE=InnoDB DEFAULT CHARSET=latin1;

-- ── mascots owned by the player ─────────────────────────────────────────────
CREATE TABLE `pangya_mascot_info` (
  `item_id` bigint NOT NULL AUTO_INCREMENT,
  `UID` int NOT NULL,
  `typeid` int NOT NULL,
  `mLevel` smallint NOT NULL DEFAULT 0,
  `mExp` int NOT NULL DEFAULT 0,
  `Flag` smallint NOT NULL DEFAULT 0,
  `Tipo` smallint NOT NULL DEFAULT 0,
  `RegDate` datetime NOT NULL DEFAULT CURRENT_TIMESTAMP,
  `Period` smallint NOT NULL DEFAULT 0,
  `EndDate` datetime DEFAULT NULL,
  `Message` varchar(30) NOT NULL DEFAULT 'PangYa SuperSS',
  `IsCash` smallint NOT NULL DEFAULT 0,
  `Price` int NOT NULL DEFAULT 0,
  `Valid` smallint NOT NULL DEFAULT 1,
  PRIMARY KEY (`item_id`),
  KEY `idx_mascot_uid` (`UID`)
) ENGINE=InnoDB DEFAULT CHARSET=latin1;

-- ── starter equipment for the test account (UID 1) ──────────────────────────
-- Give Erika a beginner clubset and a Comet ball so the player can enter a room
-- and start a game. Typeids are the official beginner defaults, defined in the
-- C++ as AIR_KNIGHT_SET (0x10000000) and DEFAULT_COMET_TYPEID (0x14000000), and
-- verified against pangya_jp.iff → ClubSet.iff / Ball.iff.

-- Own the beginner clubset + a stack of Comets in the warehouse.
INSERT INTO `pangya_item_warehouse` (`UID`, `typeid`, `ItemType`, `C0`)
VALUES
  (1, 0x10000000, 2, 1),   -- Air Knight Set (beginner ClubSet, 1 owned)
  (1, 0x14000000, 2, 100); -- Aztec / default Comet ball (stack of 100)

-- Equip Erika (character_id 1) + the starter clubset + ball.
-- Per the C++ UserEquip semantics (player.cpp): clubset_id holds the warehouse
-- item_id (the instance), character_id holds the character's item_id, and
-- ball_typeid holds the ball's typeid directly. item_slot holds equippable-item
-- typeids (potions/boosters) — empty for a beginner.
INSERT INTO `pangya_user_equip`
  (`UID`, `character_id`, `club_id`, `ball_type`)
VALUES
  (1, 1, 1, 0x14000000);  -- club_id=1 = the Air Knight warehouse row above
