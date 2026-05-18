#!/usr/bin/env bash
# Local dev launcher: builds + runs depinzcash-server with sane defaults.
# Override anything via environment.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT/server"

if [ ! -f .env ]; then
    cp .env.example .env
    echo "wrote $ROOT/server/.env from .env.example — edit ADMIN_API_KEY and TRUSTED_RPCS"
fi

cargo build --release --bin depinzcash-server

export BIND_ADDR="${BIND_ADDR:-127.0.0.1:3000}"
export DATABASE_URL="${DATABASE_URL:-sqlite://$ROOT/server/depinzcash.sqlite?mode=rwc}"
export RUST_LOG="${RUST_LOG:-info,sqlx=warn,hyper=warn}"

echo "listening on $BIND_ADDR (db: $DATABASE_URL)"
exec ./target/release/depinzcash-server
