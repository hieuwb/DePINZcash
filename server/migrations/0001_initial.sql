-- DePINZcash initial schema
-- Wallets are Solana base58 pubkeys (32-byte Ed25519 keys). Stored as text.
-- Heights are u64 in app code; SQLite stores them as INTEGER (signed 64-bit), which
-- is plenty for any realistic block height.

CREATE TABLE IF NOT EXISTS nodes (
    id TEXT PRIMARY KEY,
    wallet TEXT NOT NULL,
    kind TEXT NOT NULL,
    label TEXT,
    rpc_endpoint TEXT,
    network TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'registered',
    last_height INTEGER,
    last_block_hash TEXT,
    last_proof_at TEXT,
    registered_at TEXT NOT NULL,
    points INTEGER NOT NULL DEFAULT 0,
    uptime_seconds INTEGER NOT NULL DEFAULT 0,
    auth_token TEXT NOT NULL,
    UNIQUE (wallet, kind, label)
);

CREATE INDEX IF NOT EXISTS idx_nodes_wallet ON nodes(wallet);
CREATE INDEX IF NOT EXISTS idx_nodes_status ON nodes(status);
CREATE INDEX IF NOT EXISTS idx_nodes_points ON nodes(points DESC);

CREATE TABLE IF NOT EXISTS proofs (
    id TEXT PRIMARY KEY,
    node_id TEXT NOT NULL REFERENCES nodes(id) ON DELETE CASCADE,
    wallet TEXT NOT NULL,
    claimed_height INTEGER NOT NULL,
    claimed_block_hash TEXT NOT NULL,
    proof_timestamp TEXT NOT NULL,
    binary_hash TEXT,
    uptime_seconds INTEGER,
    peers INTEGER,
    verdict TEXT NOT NULL DEFAULT 'pending',
    reject_reason TEXT,
    points_awarded INTEGER NOT NULL DEFAULT 0,
    received_at TEXT NOT NULL,
    UNIQUE (node_id, claimed_height, claimed_block_hash)
);

CREATE INDEX IF NOT EXISTS idx_proofs_node ON proofs(node_id);
CREATE INDEX IF NOT EXISTS idx_proofs_wallet ON proofs(wallet);
CREATE INDEX IF NOT EXISTS idx_proofs_received ON proofs(received_at DESC);

CREATE TABLE IF NOT EXISTS challenges (
    id TEXT PRIMARY KEY,
    node_id TEXT NOT NULL REFERENCES nodes(id) ON DELETE CASCADE,
    kind TEXT NOT NULL,
    target_height INTEGER NOT NULL,
    expected_hash TEXT NOT NULL,
    issued_at TEXT NOT NULL,
    expires_at TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'open',
    answered_at TEXT,
    passed INTEGER
);

CREATE INDEX IF NOT EXISTS idx_challenges_node ON challenges(node_id);
CREATE INDEX IF NOT EXISTS idx_challenges_status ON challenges(status);

CREATE TABLE IF NOT EXISTS snapshots (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    cycle INTEGER NOT NULL UNIQUE,
    merkle_root TEXT NOT NULL,
    total_points INTEGER NOT NULL,
    spl_mint TEXT,
    published_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS snapshot_leaves (
    snapshot_id INTEGER NOT NULL REFERENCES snapshots(id) ON DELETE CASCADE,
    wallet TEXT NOT NULL,
    points INTEGER NOT NULL,
    leaf_hash TEXT NOT NULL,
    proof_json TEXT NOT NULL,
    PRIMARY KEY (snapshot_id, wallet)
);

CREATE INDEX IF NOT EXISTS idx_snapshot_leaves_wallet ON snapshot_leaves(wallet);

-- Registration replay protection.
CREATE TABLE IF NOT EXISTS used_nonces (
    nonce TEXT PRIMARY KEY,
    wallet TEXT NOT NULL,
    used_at TEXT NOT NULL
);
