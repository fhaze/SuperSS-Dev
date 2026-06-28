# Handoff: Course Selection Grayed + Character Stats Empty

## Status — RESOLVED (verified on the live JP client)
Root causes found by diffing our packets against the **live C++ server capture**
(`http://pangya.nozomi.local/api/gm/packets`, `Authorization: Bearer <key>`) and
decoding the JP IFF. All fixes applied, unit/integration tested byte-for-byte
against the capture, and **confirmed working on the client** (stats show, course
is editable).

## The actual blocker — player `oid` mismatch (the "grayed" cause)
Both the course button **and** the stat panel were *grayed* (disabled), not just
zero. Cause: the player **object id (`oid`) is not the uid** — it's a per-session
handle (the captured player had `oid = 0`). The client matches the oid it learned
at login (principal `MemberInfo.oid`, which we leave 0) against the `oid` in the
`0x46` lobby and `0x48` room packets to find **itself**. We were sending
`oid = uid` (1) there → mismatch → the client couldn't identify itself in the
room and disabled all "you"-specific UI. Fix: send a consistent `oid = 0`
everywhere (`bin/game-server/src/main.rs`). See Claude memory `pangya-player-oid`.
Multiplayer will need a real per-session oid counter conveyed per player.

## Root cause #1 — Character stats show zero (FIXED)
The client computes the character's stat bars **entirely from its equipped
parts**, looking each `parts_typeid` up in `Part.iff` and summing the part's
`c[]`/`slot[]` stats. Verified from the capture: `pcl` was `[0,0,0,0,0]` yet the
character showed stats, so **`pcl` is ignored**.

The prior attempt seeded **default parts** (`initComboDef`, `parts_id = 0`).
Decoding `Part.iff` shows **default parts have all-zero stats** — they're
cosmetic. Only **equipped gear** (`parts_id != 0`, a real warehouse `item_id`)
carries non-zero stats. So that attempt could never produce stats, and the
"absurd currency" was a side effect of the malformed currency acks (`0x96`,
`0x102`) bundled in the same WIP commit — not the parts.

Critically, **the bulk of the stat bars comes from the equipped CLUBSET**, not the
parts. `ClubSet.iff` gives the Air Knight beginner set base stats `(8,9,8,3,3)`;
parts add a small bonus. The client reads the clubset from the **principal's
`ClubSetInfo` block (`0x44`)**, which `build_player_info` was **zeroing** — so
stats read ~0 even fully equipped.

**Fix:** (a) seed the test character **Erika `0x04000001`** with equipped stat
parts (`migrations/0002`) + backing warehouse rows (`migrations/0003`); the repo
loads both `parts_N` (typeid) and `parts_id_N` (instance id). (b) Serialize the
real equipped `ClubSetInfo` in the principal (`build_player_info`). Verified:
- `character_info_matches_live_capture` / `player_room_info_header_matches_live_capture`
  — the serializers reproduce the live-capture (Kooh) bytes byte-for-byte.
- `equipped_character_has_stat_parts` (integration) — the repo loads Erika's
  equipped parts from the DB correctly.

## Course selection grayed — THE CAUSE was the `oid` mismatch (above)
The course gray was **not** a room-packet content bug — every create-response
packet (`0x49`/`0x4A`/`0x48`/`0x47`) was already byte-correct vs the capture, and
the order doesn't matter (same-millisecond sends). It was the **`oid` mismatch**
(see the section above): the client couldn't identify itself in the room.

Supporting fixes made along the way (correct, but not the blocker):
- **`0x48` field bug:** `flag_item_boost` (u16) precedes `mascot_typeid` (u32) —
  they were swapped (guarded by `player_room_info_header_matches_live_capture`).
- **`state_flag`** matches the captured master `0x0228` (master+sexo+**ready**;
  previously `azinha`).
- **`0x0A` was mis-handled as "Leave Room" — it is actually "Change Room Info"**
  (`game_server.cpp:302`), the master's course/holes/mode change. The old handler
  *removed the player from the room*. Now it parses the `INFO_CHANGE` fields,
  applies them, and rebroadcasts `0x4A`+`0x47` (`room.cpp:1446`) — this is what
  makes changing the course actually work once the button is enabled.

## How to verify
1. Servers are running (`./run/start-servers.sh`). DB already migrated.
2. Connect the JP client, log in `test`/`test`.
3. Expect: equipped character **Erika** with **non-zero stat bars**.
4. Create a room → stats visible in the room; **course button enabled**;
   changing the course updates the room (sends `0x0A`, server replies `0x4A`).
5. If course is still grayed: capture the GS packet log
   (`run/game.log`, `[PKT]` lines) around room creation and diff field-by-field
   against the live capture for the same flow.

## Key references
- Stats mechanism + capture API: see Claude memory
  (`pangya-stat-mechanism`, `pangya-gm-capture-api`).
- `migrations/0002` / `0003` — Erika seed + equipped parts + warehouse.
- `crates/pangya-db/src/repos.rs::characters` — loads `parts_N` + `parts_id_N`.
- `crates/pangya-proto/src/game_resp.rs` — `write_character_info`,
  `write_player_room_info` (+ capture-vector tests).
- `bin/game-server/src/main.rs` — `0x08` create-room flow, `0x0A`
  change-room-info (`parse_change_room_info`).
- `reference-cpp/Server Lib/Game Server/` — C++ reference.
