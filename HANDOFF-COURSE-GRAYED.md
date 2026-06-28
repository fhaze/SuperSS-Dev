# Handoff: Course Selection Grayed + Character Stats Empty

## Status
- **Working:** Login, lobby, player list, Game Play room list, room creation,
  room entry, equipped character/clubset/ball (all DB-driven).
- **Broken:** Course selection button is grayed in the room UI. Character stats
  (power/control/accuracy/spin/curve) show as all zeros in the room.

## The Problem
When a player creates a room and enters it:
1. The **course selection button** is grayed/disabled — the player cannot
   change the course.
2. The **character stats** panel shows all zeros (no power/control/etc.).

On the real C++ server, both work correctly for the same client.

## Root Cause Analysis (what was tried)
Extensive packet comparison against the live C++ server capture
(`pangya.nozomi.local/api/gm/packets`) showed that our `0x0049` (RoomInfo),
`0x004A` (room update), `0x0048` (PlayerRoomInfoEx), `0x0047` (room list),
and `0x0046` (lobby players) packets all match the C++ struct layout
byte-for-byte (verified via full-hex diff). The only per-player differences
were expected (room name, UID, room number).

### What was tried (all unsuccessful):
1. **state_flag bits** — Added master(3), sexo(5), ready(9) bits to match
   the C++ `0x0228`. The C++ always sets `ready=1` for the master
   (`room.cpp:1141`).
2. **tipo_ex** — Set to `0xFF` (~0) for normal rooms (`room.cpp:979`).
3. **Room security key** — Generated `key[17]` via `geraSecurityKey` formula
   (was all-zeros).
4. **Packet ordering** — Swapped to send `0x4A` before `0x49` (matching
   `sendUpdate()` → `sendMake()` in the C++).
5. **Batched TCP writes** — Sent `0x4A`+`0x49`+`0x48` in a single `write_all`.
6. **Missing `0x000C` handler** — `ChangePlayerItemRoom` (same as `0x000B`).
7. **sendCompleteData acks** — Added `0x102`, `0xF1`, `0x144`, `0x135`,
   `0x136`, `0x13F`, `0x96` to the login cascade.
8. **`0x47` full RoomInfo** — Used `write_room_info_full` (221 bytes) instead
   of the truncated `write_room_entry`.
9. **Equipped parts** — Seeded default parts via `initComboDef` formula
   (this caused the "absurd currency" bug — the part typeids were
   misinterpreted by the client as pang/cookie amounts in the 0x0044 packet.
   **Reverted.**)

### The most promising lead: equipped parts
The C++ character in the capture has **8 equipped parts** (`parts_typeid` with
real values like `0x08142400`). Our character has **zero parts**.

The client computes displayed stats from equipped parts (looking them up in its
local Part.iff for their stat contributions). With no parts → stats show 0.
The course button may also check for valid equipped parts.

**Why the parts attempt broke:** The `initComboDef` formula
`part_typeid = (((typeid << 5) | i) << 13) | 0x8000400` (32-bit truncated)
produces 7 valid typeids for Erika that exist in Part.iff:
```
slot 0 = 0x08040400, slot 1 = 0x08042400, slot 2 = 0x08044400,
slot 3 = 0x08046400, slot 4 = 0x08048400, slot 6 = 0x0804C400,
slot 7 = 0x0804E400
```
However, seeding these into `pangya_character_information.parts_N` caused the
client to display absurd currency amounts. The parts_typeid values may be
getting read at the wrong offset in the `0x0044` principal packet or the
`0x0048` PlayerRoomInfoEx — OR the `parts_id` array (which was zero) needs to
have matching warehouse item_ids.

**The C++ capture shows `parts_id` has non-zero values too** (11351, 11352,
11353) — these are warehouse row IDs. The parts may need BOTH `parts_typeid`
AND `parts_id` set, AND corresponding warehouse rows.

## What to investigate next

### 1. The `0x0044` principal packet UserInfo field
The absurd currency suggests the `parts_typeid` values leaked into the wrong
part of the `0x0044` packet. Check the `UserInfo` struct (245 bytes at the
principal) — the `pang`/`cookie` fields might be adjacent to where parts are
being written. The `CharacterInfo` in the principal is at a specific offset;
verify the parts aren't overwriting UserInfo fields.

### 2. Equip parts properly via warehouse + parts_id
Instead of just setting `parts_typeid` in the CharacterInfo, also:
- Create warehouse rows for each part (with matching `parts_id`)
- Set BOTH `parts_typeid[N]` and `parts_id[N]` in the CharacterInfo
- This mirrors what the C++ `item_manager::addItem` does

### 3. Compare the FULL packet flow byte-by-byte
The C++ server sends MANY more packets in `sendCompleteData` that we don't:
`0x0131` (Treasure Hunter), `0x0138` (cards), `0x0137` (card equip),
`0x0181` (item buffs), `0x021D` (counter items), `0x021E` (achievements),
`0x0169` (trophies), `0x00B4` (tourney stats), `0x0158` (Cadie Cauldron),
`0x025D` (grand prix). One of these may be required.

### 4. Check if `skin_typeid[5]` must be zero
The C++ `room.cpp:1137`: `pri.skin[4] = 0;` — "if it's not zero, the character
image doesn't show." Verify our `skin_typeid` array is all zeros.

### 5. Use the pangya-editor repo
The `https://github.com/fhaze/pangya-editor` repo has an IFF viewer/editor that
can decode the Part.iff records and show exactly what stats each part provides.
This would help verify the part typeids are correct.

## Key files
- `bin/game-server/src/main.rs` — MakeRoom handler (0x08), room-entry sequence
- `crates/pangya-proto/src/game_resp.rs` — all packet builders
- `crates/pangya-db/src/repos.rs` — character/equipment loaders
- `crates/pangya-server-core/src/game_login.rs` — login cascade
- `migrations/0002_character_information.sql` — character seed
- `migrations/0003_equipment.sql` — equipment tables + seed
- `reference-cpp/Server Lib/Game Server/` — C++ reference source

## C++ reference for room creation flow
- `channel.cpp:1369` — `requestMakeRoom` (the handler)
- `channel.cpp:1614-1627` — the post-creation sequence:
  `sendUpdate()` → `sendMake()` → `sendCharacter()` →
  `sendUpdateRoomInfo()` → `sendUpdatePlayerInfo()`
- `room.cpp:1106` — `updatePlayerInfo` (builds PlayerRoomInfoEx)
- `room.cpp:1446` — `requestChangeInfoRoom` (0x0023 room settings change)
- `pangya_game_st.h:2189` — `PlayerRoomInfo` struct (348 bytes)
- `pangya_game_st.h:2285` — `PlayerRoomInfoEx` (348 + 513 CharacterInfo)
- `pangya_st.h:761` — `initComboDef` (default parts formula)

## Test setup
- MySQL running via `docker compose up -d mysql`
- Migrations applied via `sqlx migrate run`
- Servers started via `./run/start-servers.sh`
- Test account: `test`/`test`, UID 1, female, Erika (0x04000001)
- Client: JP Pangya client connecting to 127.0.0.1
- C++ capture API: `pangya.nozomi.local/api/gm/packets`
