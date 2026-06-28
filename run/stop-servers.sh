#!/usr/bin/env bash
# Stop the Rust Pangya servers started by start-servers.sh.
set -euo pipefail
RUN="$(cd "$(dirname "$0")" && pwd)"

for name in auth login game; do
    pidfile="$RUN/$name.pid"
    if [ -f "$pidfile" ]; then
        pid="$(cat "$pidfile")"
        if kill -0 "$pid" 2>/dev/null; then
            kill "$pid" && echo "$name-server stopped (pid $pid)"
        fi
        rm -f "$pidfile"
    fi
done
