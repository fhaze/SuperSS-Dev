#!/usr/bin/env bash
# Launch the Rust Pangya servers (auth, login, game) for client testing.
#
# Usage:  ./run/start-servers.sh
#
# Each server runs in its own directory with its own server.ini, logging to
# run/<srv>.log. Packet hex dumps (API-comparable format) appear at INFO.
#
# Stop everything:  ./run/stop-servers.sh   (or kill the PIDs in run/*.pid)

set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
BIN="$ROOT/target/debug"
RUN="$ROOT/run"

mkdir -p "$RUN"

# Make sure the binaries are built.
echo "building servers..."
(cd "$ROOT" && cargo build -p auth-server -p login-server -p game-server) >/dev/null

start_server() {
    local name="$1" ini="$2" pidfile="$3" logfile="$4"
    if [ -f "$pidfile" ] && kill -0 "$(cat "$pidfile")" 2>/dev/null; then
        echo "$name already running (pid $(cat "$pidfile"))"
        return
    fi
    local workdir="$RUN/$name"
    mkdir -p "$workdir"
    cp "$RUN/$ini" "$workdir/server.ini"
    (
        cd "$workdir"
        RUST_LOG=info "$BIN/$name" >"$logfile" 2>&1 &
        echo $! > "$pidfile"
    )
    echo "$name started (pid $(cat "$pidfile")) — log: $logfile"
}

echo
start_server auth-server  auth.ini  "$RUN/auth.pid"  "$RUN/auth.log"
sleep 1
start_server login-server login.ini "$RUN/login.pid" "$RUN/login.log"
sleep 1
start_server game-server  game.ini  "$RUN/game.pid"  "$RUN/game.log"

echo
echo "All servers up. Logs:"
echo "  tail -f $RUN/auth.log   (port 7777)"
echo "  tail -f $RUN/login.log  (port 10303)"
echo "  tail -f $RUN/game.log   (port 20203)"
echo
echo "Packet logs appear as: [PKT] dir=C2S|S2C srv=LS|GS opcode=0xNNNN size=N hex=..."
echo "Compare those hex values against http://pangya.nozomi.local/api/gm/packets"
