-- IFF item registry: the game's item catalog extracted from pangya_jp.iff.
--
-- Populated by the `iff-import` bin (cargo run -p iff-import). One row per
-- ownable item across the IFF tables (Part, Ball, ClubSet, Character, Item,
-- Caddie, Mascot, Club, Card, Skin, AuxPart, HairStyle, AddonPart, CounterItem,
-- CaddieItem, Furniture). The common Base + ShopDados fields are uniform; the
-- per-type stat columns are filled only for the types that have them (NULL
-- otherwise). Editing rows here lets the server tune/disable items; note the
-- client still reads its own local IFF, so price changes also need the client
-- file updated (a DB->IFF export tool is the planned counterpart).

CREATE TABLE `pangya_iff_item` (
  `id`          bigint NOT NULL AUTO_INCREMENT,
  `typeid`      bigint unsigned NOT NULL,        -- IFF _typeid (u32)
  `source`      varchar(24)  NOT NULL,           -- which IFF table
  `name`        varchar(255) NOT NULL DEFAULT '',-- decoded from Shift-JIS
  -- Base + ShopDados (uniform across all items)
  `price`       bigint unsigned NOT NULL DEFAULT 0,
  `discount`    int unsigned NOT NULL DEFAULT 0,
  `cond_value`  int unsigned NOT NULL DEFAULT 0, -- ShopDados.condition
  `is_cash`     tinyint NOT NULL DEFAULT 0,      -- 1 = cookie/CP price, 0 = pang
  `is_saleable` tinyint NOT NULL DEFAULT 0,
  `flag_shop`   int unsigned NOT NULL DEFAULT 0, -- raw FlagShop bitfield
  `rental_flag` tinyint NOT NULL DEFAULT 0,      -- time_shop.active
  `rental_days` int NOT NULL DEFAULT 0,          -- time_shop.dia
  `active`      tinyint NOT NULL DEFAULT 1,      -- Base.active
  -- main stat arrays (control/power/spin/curve/accuracy; per-type meaning)
  `c0` smallint NULL, `c1` smallint NULL, `c2` smallint NULL, `c3` smallint NULL, `c4` smallint NULL,
  `slot0` smallint NULL, `slot1` smallint NULL, `slot2` smallint NULL, `slot3` smallint NULL, `slot4` smallint NULL,
  -- ClubSet member club typeids
  `club0` bigint NULL, `club1` bigint NULL, `club2` bigint NULL, `club3` bigint NULL,
  -- AuxPart / Mascot effect bonuses
  `power_drive` smallint NULL,
  `drop_rate`   smallint NULL,
  `power_gauge` smallint NULL,
  `pang_rate`   smallint NULL,
  `exp_rate`    smallint NULL,
  -- A synthetic PK: typeids are neither globally unique (AddonPart reuses
  -- Character typeids) nor unique within a table (AddonPart has several rows per
  -- character), so they can't key the row. (typeid, source) is the lookup index.
  PRIMARY KEY (`id`),
  KEY `idx_iff_item_typeid` (`typeid`, `source`),
  KEY `idx_iff_item_source` (`source`)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4;
