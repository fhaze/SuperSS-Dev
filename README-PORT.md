# pangya-server (Rust port)

A from-scratch Rust reimplementation of the SuperSS-Dev Pangya server emulator
(originally C++, ~215k LOC). The goal is a clean, type-safe, memory-safe server
that is easier to extend with new features.

For the original project, see [`reference-cpp/readme.MD`](reference-cpp/readme.MD).

## Why a rewrite

The original server is a Windows-centric C++ codebase (IOCP/WinSock) retrofitted
onto Linux via an epoll shim. Its recurring problems — segfaults fixed by
mutexes, a 6,474-line `void*` packet dispatcher, ~180 DB commands built by SQL
string concatenation (injection-prone), and a hand-rolled memory/threading
model — are exactly what Rust's ownership model and type system remove.

## Stack

- **Runtime:** `tokio` (replaces IOCP/epoll + custom thread pools + timers)
- **SQL:** `sqlx` with **bound parameters everywhere** (closes the injection
  surface), MySQL; logic migrated out of stored procs into typed repos
- **Wire protocol:** bit-exact reimplementation of the Pangya packet framing,
  XOR cipher (8192-byte key dictionary), and LZO1X compression (`lzokay`)
- **Config:** `rust-ini` (replaces GLib `GKeyFile`)
- **Logging:** `tracing` + API-compatible packet hex logging

## Workspace layout

```
crates/
  pangya-config/      typed server.ini
  pangya-net/         crypt + LZO + framing + tokio codec
  pangya-iff/         ZIP-packed IFF static-data loader
  pangya-proto/       typed wire structs + Packet enum dispatcher
  pangya-db/          sqlx repositories (replace 165 stored procs)
  pangya-model/       domain model (PlayerInfo + aggregates)
  pangya-server-core/ Unit/Server framework, accept loop, heartbeat, dispatch
bin/
  auth-server/        the hub (port 7777) — relays inter-server commands
  login-server/       client login + server list (port 10303)
  game-server/        channels, rooms, gameplay (port 20203)
  rank-server/        leaderboards
  message-server/     social: friends, guilds, chat, mail
  gg-auth-server/     NProtect GameGuard challenge/response (port 7788)
migrations/           incremental schema (seeded from bk-squema-mysql.sql)
run/                  runtime configs (server.ini) + start/stop scripts
```

## Topology

Hub-and-spoke centered on the **Auth Server**. Login/Game/Rank/Message all dial
it; cross-server traffic is relayed through it. Game Server also dials the
GG Auth Server for GameGuard.

## Validation against the real C++ server

The port is validated against **real captured packets** from the live C++ server
via the Pangya GM API (`pangya.nozomi.local/api/gm/packets`):

- **Opcode parsing** — byte-exact match across 25+ captured packets (golden
  vectors in `crates/pangya-proto/tests/captured_vectors.rs`)
- **Login request (`0x0002`)** — field order validated, parses 73/73 bytes
- **Channel list (`0x004D`)** — `ChannelInfo` struct layout confirmed and fixed
  based on real capture (was 56 bytes, actual is 85 bytes with `name[64]`)
- **Login flow** — a real Pangya client successfully logs in, sees the server
  list, connects to the Game Server, and enters a channel

Packet hex logging (`[PKT]` lines in server logs) matches the API's `hex` field
format for direct comparison. The API now returns full untruncated hex.

## Current status

| Phase | Scope | Status |
|-------|-------|--------|
| 0 | Workspace, config, Docker, migrations, CI | ✅ done |
| 1 | `pangya-net`: crypt/compress/framing/codec | ✅ done |
| 2 | Supporting crates (iff, proto, db, model, server-core) | ✅ done |
| 3 | server-core runtime + Auth Server hub | ✅ done |
| 4 | Login Server (end-to-end login + DB integration test) | ✅ done |
| 5 | Game Server lobby (connect/login/channel/chat) | ✅ done |
| 6 | Game systems: room registry, GM commands, lobby acks | ✅ done |
| 7 | Rank, Message, GG Auth server binaries | ✅ done |
| 8 | Full player-info packet (`0x0044` principal) + default character | ✅ done |

### What works (tested with a real Pangya client)

1. **Login Server**: client connects → raw greeting → credentials verified
   (MD5 + bound params) → auth key minted → player UID sent (`0x0001`) →
   server list (`0x0002`)
2. **Server select**: client picks a game server (`0x0003`) → game auth key
   minted → client connects to Game Server
3. **Game Server**: raw greeting (`0x3F`) → login verified (`0x0002`) →
   login ack (`0x44 D3`) → player info (`0x44` option 0, 12803 bytes) →
   channel list (`0x4D`) → handshake confirm (`0xFE` → `0x1B1`)
4. **Channel entry**: enter channel (`0x04`) → lobby data sequence
   (`0x95` → `0x4E` → `0x46` → `0x47`) → lobby activity acks
5. **GM commands**: `/notice`, `/say`, `/kick` in lobby chat
6. **Room management**: create (`0x08`), enter (`0x09`), leave (`0x0A`)

### Default character (Erika) — done

The test account now has a real character so the client clears "Loading...". The
`0x0044` principal packet and the `0x004B` (change item, type 4) response both
serialize a full 513-byte `CharacterInfo` for the equipped character.

- **Character:** Erika (`typeid 0x04000001`) — the JP beginner female character,
  decoded from `pangya_jp.iff` → `Character.iff` (the `pangya-iff` loader parses
  the 420-byte record and extracts the PCL stats).
- **PCL stats:** `9/11/6/2/2` (power/control/accuracy/spin/curve).
- **DB:** migration `0002` creates `pangya_character_information` and seeds the
  Erika row for UID 1. `repos::characters()` loads it; a hardcoded Erika fallback
  covers dev environments that haven't applied the migration.

The C++ server's male default was `iff::CHARACTER << 26` = `0x04000000` (Ken);
this JP build has no "Nuri", so Erika is seeded for the female test account.

### Next steps (priority order)

1. **Load the full equipment cascade.** After the `0x0044` player-info packet,
   the C++ sends a burst of data packets: `0x70` (characters), `0x71` (caddies),
   `0x73` (warehouse items), `0xE1` (mascots), `0x72` (user equip). These are
   sent from `LoginTask::sendCompleteData` (`login_task.cpp:132-207`). We need
   to send at least the character list (`0x70`) for the client to function.

2. **Implement the room-enter flow (`0x0009`).** When the client clicks a room,
   it sends `0x0009` and expects a detailed room-state response. The C++
   `channel::requestEnterRoom` (`channel.cpp:1675`) sends room info, player list,
   and game state. This is needed before the client can enter a room.

3. **Compare more packets against the C++ capture.** The full-hex API is now
   available — diff every response packet byte-by-byte to catch remaining
   struct layout issues. Key packets to validate:
   - `0x0044` option-0 (player info) — struct field contents
   - `0x004B` (change item response) — CharacterInfo content
   - `0x0046` (lobby players) — PlayerCanalInfo struct
   - `0x0047` (room list) — RoomInfo struct

### Future work (after lobby is fully working)

- Gameplay loop: match start, shot sync (`0x1B`), hole scoring, Smart Calculator
- Per-system shops: papel shop, scratchy, memorial, treasure, gacha, cube
- Inter-server relay: the `0xD`/`0xE` Auth-Server routing delivering packets
- GameGuard challenge/response in the GG Auth Server
- Full IFF data loading at runtime (`pangya_jp.iff`)

## Getting started (dev)

```bash
# 1. Start MySQL
docker compose up -d mysql

# 2. Apply migrations + fix auth plugin + seed a test account
docker exec pangya-mysql mysql -uroot -prootpw \
  -e "ALTER USER 'pangya'@'%' IDENTIFIED WITH mysql_native_password BY 'pangya'; FLUSH PRIVILEGES;"
DATABASE_URL=mysql://pangya:pangya@127.0.0.1:3306/pangya sqlx migrate run
docker exec pangya-mysql mysql -upangya -ppangya pangya \
  -e "INSERT INTO account (ID, PASSWORD, NICK, Logon, capability, Sex, doTutorial) \
      VALUES ('test', MD5('test'), 'Tester', 0, 0, 1, 0);"
docker exec pangya-mysql mysql -upangya -ppangya pangya \
  -e "INSERT INTO pangya_server_list (Name, UID, IP, Port, MaxUser, CurrUser, Type, UpdateTime, State, PangRate, ServerVersion, ClientVersion, property) \
      VALUES ('Rust Game Server', 20203, '127.0.0.1', 20203, 2001, 0, 1, NOW(), 1, 100, 'v', 'SS.R7.995.00', 2048);"

# 3. Build & test everything
cargo test --workspace

# 4. Start all servers for client testing
./run/start-servers.sh
tail -f run/login.log run/game.log   # packet logs: [PKT] dir=... opcode=... hex=...

# 5. Stop
./run/stop-servers.sh
```

## Test account
- **ID:** `test`
- **Password:** `test`
- **UID:** 1
- **Sex:** female (`1`) — matches the default character Erika
- **Default character:** Erika (`typeid 0x04000001`), seeded by migration 0002
  with PCL stats `9/11/6/2/2` (power/control/accuracy/spin/curve) taken from
  `pangya_jp.iff`

## Comparing packets against the C++ server

Both our servers and the C++ server's GM API log packets in the same format:
post-decrypt plaintext, opcode stripped, hex-encoded. The API now returns full
untruncated hex.

```bash
# Pull captured packets from the C++ server
curl -H "Authorization: Bearer pgy_de8ba121ad981acf_76100af82f7e75f5cd557e7bab663e3634250fd0674de97e" \
  "http://pangya.nozomi.local/api/gm/packets?count=500" | python3 -m json.tool | head -50
```

## Key struct sizes (confirmed from live capture)

These sizes were verified by analyzing the full 12,801-byte `0x0044` packet
captured from the C++ server:

| Struct | Size (bytes) | Notes |
|--------|-------------|-------|
| MemberInfo | 297 | Not 292 — has extra bytes vs our C reproduction |
| UserInfo | 245 | Not 265 — smaller than computed |
| TrofelInfo | 90 | Correct |
| UserEquip | 116 | Correct |
| MapStatistics | 43 | Correct, × 22 maps × 12 arrays |
| CharacterInfo | 513 | Correct |
| CaddieInfo | 25 | Correct |
| ClubSetInfo | 28 | Correct |
| MascotInfo | 70 | Correct |
| PlayerPapelShopInfo | 6 | Correct |
| ChannelInfo | 85 | `name[64]` + i16/i16/u8/u32/i32/i32/i32 |
| ServerInfo | 92 | Used in login server-list (`0x0002`) |
