# SuperSS — PangYa Server (Rust)

A from-scratch **Rust reimplementation of a PangYa** (the golf MMO, *SuperSS*
era) game server. It is a faithful port of an original C++ server — kept in this
repo under [`reference-cpp/`](reference-cpp/) — reworked into a safe, async,
multi-crate Rust workspace.

The server speaks the real PangYa wire protocol and is validated against a live
C++ server byte-for-byte, so an unmodified JP client connects and plays through
the lobby flow.

## Status

Working against the live JP client:

- Auth → Login → Game server handshake (encrypted PangYa framing)
- Channel list + channel enter, lobby chat
- Multiplayer lobby: room list, **create / enter / exit room**, **exit lobby**
- DB-driven equipment: character, clubset, ball, and **equipped parts**
- Correct **character stats** (computed from clubset + parts, as the client does)

Not yet implemented: the in-game golf round (shot/turn loop), shop, guilds,
tournaments, and most progression systems. Contributions welcome.

## Architecture

Each server is its own binary; shared logic lives in library crates.

**Servers** (`bin/`): `auth-server` (7777), `login-server` (10303),
`game-server` (20203), plus `rank-server`, `message-server`, `gg-auth-server`.

**Crates** (`crates/`):

| Crate | Responsibility |
|-------|----------------|
| `pangya-net` | TCP framing, XOR crypto + key dictionary, compression, codec |
| `pangya-proto` | Packet builders/parsers; wire structs mirror the C++ `#pragma pack(1)` layouts byte-for-byte |
| `pangya-model` | Domain types (account, player, character, room, equipment) |
| `pangya-db` | `sqlx` repositories (MySQL), parameterized queries |
| `pangya-iff` | Reads PangYa `.iff` game data (ZIP of fixed-size record tables) |
| `pangya-config` | `server.ini` parsing |
| `pangya-server-core` | Shared server logic: login cascade, dispatch, session, packet logging |

## Quick start

**Prerequisites:** Rust 1.81+, Docker (for MySQL), `sqlx-cli`
(`cargo install sqlx-cli --no-default-features --features mysql`), and a PangYa
JP client (SuperSS R7.x) pointed at `127.0.0.1`.

```sh
# 1. Database
docker compose up -d mysql
export DATABASE_URL='mysql://pangya:pangya@127.0.0.1:3306/pangya'
cargo sqlx migrate run

# 2. Build + run the servers (auth, login, game)
./run/start-servers.sh
#    logs: run/{auth,login,game}.log   stop: ./run/stop-servers.sh

# 3. Connect the JP client to 127.0.0.1 and log in with:
#       id: test   password: test
```

The default seed gives the test account (UID 1) the character **Erika** with a
beginner clubset, a ball, and equipped stat parts.

## Development

```sh
cargo build
cargo test          # unit tests + capture-vector tests (byte-exact wire checks)

# DB integration tests (need MySQL up + migrated):
cargo test -p pangya-server-core --features integration --test login_db -- --ignored
```

Conventions:

- **All code and comments are in English** (the C++ reference is in Portuguese —
  do not copy its identifiers/comments).
- **Wire serializers must match the C++ packed structs byte-for-byte.** New or
  changed packets should be locked in with a capture-vector test (see
  `crates/pangya-proto/tests/fixtures/*.hex`).
- The C++ in [`reference-cpp/`](reference-cpp/) is the authority for packet
  opcodes, struct field order, and behavior. See [`CLAUDE.md`](CLAUDE.md) for a
  deeper map of the codebase and protocol notes.

## License

MIT
