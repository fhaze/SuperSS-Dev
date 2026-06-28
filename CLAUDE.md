# CLAUDE.md

Guidance for working in this repository.

## What this is

A **Rust reimplementation of a PangYa (golf MMO) game server**, ported from the
original C++ server, which is preserved verbatim under `reference-cpp/`. The C++
is the **authority** for packet opcodes, struct layouts, and behavior — when in
doubt about the wire format, read the C++ and match it byte-for-byte.

Multi-process architecture (each bin is a separate server) over shared crates.

## Build / test / run

- **Build:** `cargo build`
- **Test:** `cargo test` (unit + capture-vector tests).
  DB integration tests are feature-gated and ignored by default:
  `DATABASE_URL=mysql://pangya:pangya@127.0.0.1:3306/pangya cargo test -p pangya-server-core --features integration --test login_db -- --ignored`
- **Run servers:** `./run/start-servers.sh` (builds + starts auth/login/game),
  `./run/stop-servers.sh`. Logs at `run/{auth,login,game}.log`. Sent/received
  packets are logged as `[PKT] dir=C2S|S2C srv=GS opcode=0xNNNN size=N hex=...`
  — invaluable for diffing against the live C++ server.
  Ports: auth 7777, login 10303, game 20203.
- **Database:** MySQL via `docker compose up -d mysql`; apply migrations with
  `cargo sqlx migrate run`. Test login: `test` / `test` (UID 1, female).
  - The `account` table is **seeded manually, not by a migration** — do NOT
    `sqlx database drop` or you lose the test login. To re-apply an edited
    migration, drop only its tables + `DELETE FROM _sqlx_migrations WHERE
    version IN (...)`, then re-run.

## Architecture (workspace crates)

- **pangya-net** — TCP framing + crypto (XOR cipher with a key dictionary),
  compression, tokio codec. Raw / server / client frame formats.
- **pangya-proto** — packet builders (`game_resp.rs`, `login_resp.rs`) and
  parsers (`game.rs`). Wire structs mirror the C++ `#pragma pack(1)` packed
  structs **byte-for-byte**.
- **pangya-model** — domain types (`Account`, `PlayerState`, `CharacterInfo`,
  `Room`, `UserEquip`, …).
- **pangya-db** — sqlx repositories (`repos.rs`, one fn per query), MySQL, all
  bound parameters (no string interpolation; sqlx 0.9 requires `&'static str`
  SQL).
- **pangya-iff** — reads PangYa `.iff` data (a plain ZIP; each entry = 8-byte
  `Head` + fixed-size records).
- **pangya-config** — `server.ini` parsing.
- **pangya-server-core** — shared server logic: login cascade (`game_login.rs`),
  dispatch, session, `packet_log.rs`.
- **bins** — auth-server, login-server, game-server (the gameplay loop +
  C2S opcode match in `bin/game-server/src/main.rs`), rank-server,
  message-server, gg-auth-server.

## Ground-truth resources (verify the wire format with these)

- `reference-cpp/Server Lib/` — original C++ source. Authority for opcodes,
  field order, struct sizes. Structs use `#pragma pack(1)`. **The C++ is written
  in Portuguese.**
- C2S opcode → handler dispatch: `reference-cpp/Server Lib/Game Server/Game
  Server/game_server.cpp` (the `addPacketCall(0xNN, packetNN, ...)` table).
  S2C builders: the `pacoteNNN` functions in `PACKET/packet_func_sv.cpp`.
- `reference-cpp/.../Game Server/data/pangya_jp.iff` — JP game data. Decode with
  Python `zipfile`: `Character.iff`, `Part.iff`, `ClubSet.iff`, … each = 8-byte
  header (count_element u16, …) then fixed records (Base = `active u32, _typeid
  u32, name[64]`, …).
- `reference-cpp/bk-squema-mysql.sql` — original DB schema.
- **Live C++ server packet-capture API** for byte-exact ground truth — see the
  Claude memory `pangya-gm-capture-api` (Bearer-auth; ask the user for the key).

## Conventions

- **All code and comments in English.** The C++ reference is Portuguese — do not
  copy Portuguese identifiers or comments. (Some pre-existing wire-struct field
  names — `senha_flag`, `tipo_show`, `numero`, etc. — still mirror the C++ and
  are kept as-is for now.)
- **Wire serializers must match the C++ packed struct byte-for-byte.** When you
  add or change a packet, lock it in with a capture-vector test that asserts the
  serializer reproduces real captured bytes — see
  `crates/pangya-proto/tests/fixtures/*.hex` and the `*_matches_live_capture`
  tests in `game_resp.rs`.
- When a behavior is unclear, prefer reading the C++ handler over guessing; the
  packet-capture API resolves anything the source doesn't.

## Hard-won protocol gotchas (also in Claude memory)

- **`pangya-player-oid`** — the player `oid` is a per-session object handle, NOT
  the uid. It must be identical across the principal (`MemberInfo.oid`), the
  `0x46` lobby packet, and the `0x48` room packet, or the client can't identify
  itself and grays the room UI (course select + stats). Hardcoded to 0 for the
  single test player; multiplayer needs a real per-session counter.
- **`pangya-stat-mechanism`** — the character stat bars come from the equipped
  **clubset** (`ClubSet.iff` base stats) + equipped **parts** (`parts_id != 0`,
  stat-bearing); `pcl` is ignored. The clubset must be serialized in the
  principal (`0x44`), not zeroed.

## Status

Login, lobby, channel enter, room create/enter/exit, lobby exit, chat, and
DB-driven equipment/character/stats all work against the live JP client. The C++
`origin/master` is a separate legacy line; the Rust port lives on the local
`master`.
