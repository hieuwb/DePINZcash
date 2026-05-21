# DePINZcash

**Decentralized Physical Infrastructure Network for Zcash**

Incentive layer for Zcash nodes. Earn rewards for running a Zebra full node — the server verifies your node against a trusted-RPC quorum and pays out in **$ZePIN** (an SPL token on Solana).

> **Rewards on Solana, for now.** Until [NU7](https://zips.z.cash/protocol/nu7) and [ZIP-227](https://zips.z.cash/zip-0227) land custom assets on Zcash, payouts use a custom $ZePIN token. Once native Zcash custom assets ship, the protocol migrates to ZEC-denominated assets without changing the operator flow. This is surfaced in `/api/info` under `rewards_note`.

---

## Architecture

```
   ┌──────────────────┐   sign + POST     ┌─────────────────────┐  RPC quorum  ┌──────────────────┐
   │   depinzcash-    │ ─────────────────▶│  depinzcash-server  │ ───────────▶ │  Trusted Zcash   │
   │   relay (CLI)    │                   │   (Rust / Axum)     │              │  full nodes      │
   └────────▲─────────┘                   └─────────┬───────────┘              └──────────────────┘
            │                                       │
            │ reads Zebra metrics                   │ Merkle snapshot
            ▼                                       ▼
   ┌──────────────────┐                   ┌─────────────────────┐
   │ Local Zebra full │                   │  Solana $ZePIN claim   │
   │ node (RocksDB)   │                   │  (NU7/ZIP-227 ready)│
   └──────────────────┘                   └─────────────────────┘
```

Three components, one repo:

- **[server/](server/)** — Rust / Axum backend. Verifies signed proofs, runs the points/uptime scheduler, builds Merkle snapshots for $ZePIN claim distribution.
- **[prover/](prover/)** — Two binaries:
  - `depinzcash-prover` — Halo 2 ZK proof generator that reads Zebra state (existing).
  - `depinzcash-relay` — operator-side CLI that signs node-state submissions with a Solana keypair and posts them to the server.
- **web/** — React/Vite frontend (planned next).
- **contracts/** — Solana program for $ZePIN claim distribution (planned).

---

## Quick start (local prototype)

Requires Rust 1.70+ and SQLite (bundled via sqlx).

```bash
# 1. Build server + relay
cd server && cargo build --release
cd ../prover && cargo build --release --bin depinzcash-relay

# 2. Run server (writes ./depinzcash.sqlite, listens on :3000)
cd ../server
cp .env.example .env   # edit ADMIN_API_KEY, TRUSTED_RPCS, SPL_MINT
./target/release/depinzcash-server

# 3. From another shell, generate a Solana keypair + register a node
cd /tmp/operator
/path/to/depinzcash-relay keygen --out config/solana-keypair.json
/path/to/depinzcash-relay register \
    --api http://127.0.0.1:3000 \
    --keypair config/solana-keypair.json \
    --kind zebra-full \
    --label primary

# 4. Submit a node-state proof (or use --proof-file with a Halo 2 proof JSON)
/path/to/depinzcash-relay submit \
    --api http://127.0.0.1:3000 \
    --keypair config/solana-keypair.json \
    --height 2500001 \
    --block-hash 0000000000... \
    --uptime-seconds 7200 --peers 12

# 5. Watch points / leaderboard
curl http://127.0.0.1:3000/api/stats/network
curl http://127.0.0.1:3000/api/stats/leaderboard
curl http://127.0.0.1:3000/api/wallet/<your-wallet>/stats
```

For continuous operation, the relay supports `watch`:

```bash
depinzcash-relay watch --interval-secs 300 \
    --api http://127.0.0.1:3000 \
    --keypair config/solana-keypair.json \
    --proof-file proofs/latest.json
```

---

## Verification model

**Permissive mode (dev/early):** if `TRUSTED_RPCS` is empty, the server accepts proofs without cross-checking and tags them `permissive-mode:no-trusted-rpcs`. Useful before you have RPC endpoints lined up.

**Quorum mode (production):** set `TRUSTED_RPCS` to a comma-separated list of JSON-RPC endpoints (Zebra `--rpc-listen-addr`, zcashd, or a managed provider). The server calls `getblockcount` / `getblockhash <h>` on all of them, takes the majority answer, and rejects any submission whose claimed block hash diverges from the quorum at the same height. Configurable height drift via `MAX_HEIGHT_DRIFT` (default 8 blocks).

Anti-cheat layers in place:

- Ed25519 Solana signatures on every submission (registration + each proof).
- Per-wallet nonce table — registration and proof nonces are single-use.
- Clock skew window (`MAX_CLOCK_SKEW`, default 15m) on submitted timestamps.
- Monotonic-height guard — a proof more than 1024 blocks behind a node's last accepted proof is rejected.
- Random-depth block-hash challenges (`POST /api/challenges/request`) — server picks a random recent block height from the trusted quorum and asks the operator to prove they have its hash, which a freshly bootstrapped fake won't.

---

## Rewards

Points accrue per accepted proof and per uptime tick:

```
points = tier * (1 + freshness)  +  min(uptime_hours, 24)  +  min(peers/4, 3)
where: tier        = 10 (zebra-full) | 6 (lightwalletd)
       freshness   = max(0, 5 - height_drift_from_trusted_tip)
```

On a fixed cadence (`SNAPSHOT_INTERVAL`, default `7d`) the server publishes a **Merkle snapshot** of lifetime points per wallet. The snapshot is sorted-pair / sorted-leaf so a Solana program can verify a claim against the root with a single `keccak`-style folding loop. Operators fetch their proof at:

```
GET /api/wallet/<solana-pubkey>/claim/latest
```

The Solana claim program lives in `contracts/` (planned next).

---

## API surface

| Method | Path | Notes |
|--------|------|-------|
| GET | `/healthz`, `/readyz` | Liveness + readiness |
| GET | `/api/info` | Version, network, registration message format, $ZePIN mint |
| POST | `/api/nodes/register` | Signed registration → returns `node_id` + `auth_token` |
| GET | `/api/nodes/:id` | Public node info |
| GET | `/api/wallet/:wallet/nodes` | Nodes owned by wallet |
| GET | `/api/wallet/:wallet/stats` | Aggregate points + uptime |
| GET | `/api/wallet/:wallet/proofs` | Recent proofs |
| GET | `/api/wallet/:wallet/claim/latest` | Latest Merkle claim payload |
| POST | `/api/proofs/submit` | Signed proof submission |
| POST | `/api/challenges/request` | Operator requests a random-depth challenge |
| POST | `/api/challenges/submit` | Operator answers a challenge |
| GET | `/api/stats/network` | Network-wide totals |
| GET | `/api/stats/leaderboard` | Top wallets by points |
| GET | `/api/snapshots/latest` | Latest published snapshot summary |
| POST | `/api/admin/snapshot/publish` | Force-publish (requires `x-admin-key`) |

Full signature/message formats are in `server/src/auth.rs` (`registration_message`, `proof_message`). The relay implements the same formats — both must agree byte-for-byte.

---

## Configuration

Server reads `server/.env` (see `server/.env.example`). Key knobs:

| Var | Default | Purpose |
|-----|---------|---------|
| `BIND_ADDR` | `0.0.0.0:3000` | HTTP listener |
| `DATABASE_URL` | `sqlite://depinzcash.sqlite?mode=rwc` | SQLite DSN |
| `ZCASH_NETWORK` | `mainnet` | `mainnet` or `testnet` |
| `TRUSTED_RPCS` | (empty) | Comma-sep Zcash JSON-RPC quorum |
| `ADMIN_API_KEY` | (empty) | Required for `/api/admin/*` |
| `CORS_ALLOWED_ORIGINS` | (empty) | For the web frontend |
| `MAX_HEIGHT_DRIFT` | `8` | Reject proofs that diverge by more than this |
| `MAX_CLOCK_SKEW` | `15m` | Timestamp window |
| `SNAPSHOT_INTERVAL` | `7d` | Reward snapshot cadence (`0`/`off` disables cron) |
| `SPL_MINT` | (empty) | Surfaced to clients |
| `SOLANA_CLUSTER` | `devnet` | `devnet` / `testnet` / `mainnet-beta` |

---

## Tests

```bash
cd server && cargo test    # 21 tests: unit + e2e router exercise
cd prover && cargo test
```

The server e2e suite (`server/tests/e2e_register_and_proof.rs`) hits the real router with an in-memory SQLite store and exercises register → submit → leaderboard → snapshot → claim end-to-end.

---

## Repo layout

```
DePINZcash/
├── prover/                   # Rust prover crate
│   ├── src/
│   │   ├── main.rs           # depinzcash-prover (Halo 2)
│   │   ├── bin/relay.rs      # depinzcash-relay (sign + submit)
│   │   └── ...               # zebra_reader, proof_gen, halo2_circuit
│   └── Cargo.toml
├── server/                   # Rust / Axum backend
│   ├── src/
│   │   ├── main.rs           # entry
│   │   ├── api/              # axum handlers
│   │   ├── store/            # sqlx + sqlite
│   │   ├── rpc.rs            # trusted-RPC quorum client
│   │   ├── scheduler.rs      # heartbeat / uptime / staleness / snapshot tickers
│   │   ├── merkle.rs         # $ZePIN claim Merkle tree
│   │   ├── auth.rs           # Solana signature verification
│   │   └── ...
│   ├── migrations/
│   ├── tests/                # e2e router tests
│   └── .env.example
├── web/                      # React/Vite frontend (planned)
├── contracts/                # Solana $ZePIN claim program (planned)
├── config/                   # Operator-side config templates
├── proofs/                   # Halo 2 proofs dropped here
├── scripts/
├── docs/
│   ├── TECHNICAL_SPEC.md
│   ├── REWARDS.md
│   ├── ZK_IMPLEMENTATION.md
│   └── FAQ.md
└── README.md
```

---

## Roadmap

### Phase 1 — Prototype (current)
- [x] Rust prover (Halo 2) — generates proofs from Zebra state
- [x] Rust backend — verifies + scores + ranks
- [x] Relay CLI — sign + submit + watch loop
- [x] Trusted-RPC quorum verification
- [x] Merkle snapshot for $ZePIN claim
- [x] End-to-end smoke + 21 tests
- [ ] React/Vite web frontend
- [ ] Solana program for $ZePIN claim distribution

### Phase 2 — Production-ready
- [ ] Replay Halo 2 proofs to the server for stronger anti-cheat
- [ ] Lightwalletd-specific challenges
- [ ] Mobile monitoring app
- [ ] Public testnet launch

### Phase 3 — Native Zcash assets
- [ ] Migrate payout layer to NU7 / ZIP-227 custom assets once available
- [ ] Keep $ZePIN / Solana path open for cross-chain demand

---

## License

MIT — see [LICENSE](LICENSE).
