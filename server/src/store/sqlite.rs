use anyhow::{anyhow, Context};
use chrono::{DateTime, Utc};
use sqlx::{
    sqlite::{SqliteConnectOptions, SqlitePool, SqlitePoolOptions},
    ConnectOptions, Row,
};
use std::str::FromStr;
use std::time::Duration;
use uuid::Uuid;

use crate::types::{
    Challenge, ChallengeKind, ChallengeStatus, NetworkStats, Node, NodeDailyBucket, NodeKind,
    NodeStatus, Proof, ProofVerdict, WalletStats,
};

#[derive(Clone)]
pub struct SqliteStore {
    pool: SqlitePool,
}

impl SqliteStore {
    pub async fn connect(url: &str) -> anyhow::Result<Self> {
        let opts = SqliteConnectOptions::from_str(url)
            .context("parsing sqlite url")?
            .create_if_missing(true)
            .busy_timeout(Duration::from_secs(5))
            .foreign_keys(true)
            .log_statements(tracing::log::LevelFilter::Trace);

        let pool = SqlitePoolOptions::new()
            .max_connections(8)
            .acquire_timeout(Duration::from_secs(10))
            .connect_with(opts)
            .await
            .context("opening sqlite pool")?;
        Ok(Self { pool })
    }

    pub async fn migrate(&self) -> anyhow::Result<()> {
        sqlx::migrate!("./migrations")
            .run(&self.pool)
            .await
            .context("running migrations")?;
        Ok(())
    }

    pub fn pool(&self) -> &SqlitePool {
        &self.pool
    }

    // ---- node management ----------------------------------------------------

    #[allow(clippy::too_many_arguments)]
    pub async fn insert_node(
        &self,
        node: &Node,
        auth_token: &str,
    ) -> anyhow::Result<()> {
        sqlx::query(
            r#"INSERT INTO nodes (id, wallet, kind, label, rpc_endpoint, network, status,
                last_height, last_block_hash, last_proof_at, registered_at, points, uptime_seconds, auth_token)
                VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)"#,
        )
        .bind(node.id.to_string())
        .bind(&node.wallet)
        .bind(node.kind.as_str())
        .bind(&node.label)
        .bind(&node.rpc_endpoint)
        .bind(&node.network)
        .bind(node.status.as_str())
        .bind(node.last_height.map(|h| h as i64))
        .bind(&node.last_block_hash)
        .bind(node.last_proof_at.map(|t| t.to_rfc3339()))
        .bind(node.registered_at.to_rfc3339())
        .bind(node.points as i64)
        .bind(node.uptime_seconds as i64)
        .bind(auth_token)
        .execute(&self.pool)
        .await
        .context("inserting node")?;
        Ok(())
    }

    pub async fn get_node(&self, id: Uuid) -> anyhow::Result<Option<Node>> {
        let row = sqlx::query(
            r#"SELECT id, wallet, kind, label, rpc_endpoint, network, status,
                last_height, last_block_hash, last_proof_at, registered_at, points, uptime_seconds
                FROM nodes WHERE id = ?1"#,
        )
        .bind(id.to_string())
        .fetch_optional(&self.pool)
        .await?;
        row.map(node_from_row).transpose()
    }

    pub async fn get_node_by_auth_token(&self, token: &str) -> anyhow::Result<Option<Node>> {
        let row = sqlx::query(
            r#"SELECT id, wallet, kind, label, rpc_endpoint, network, status,
                last_height, last_block_hash, last_proof_at, registered_at, points, uptime_seconds
                FROM nodes WHERE auth_token = ?1"#,
        )
        .bind(token)
        .fetch_optional(&self.pool)
        .await?;
        row.map(node_from_row).transpose()
    }

    pub async fn list_nodes_by_wallet(&self, wallet: &str) -> anyhow::Result<Vec<Node>> {
        let rows = sqlx::query(
            r#"SELECT id, wallet, kind, label, rpc_endpoint, network, status,
                last_height, last_block_hash, last_proof_at, registered_at, points, uptime_seconds
                FROM nodes WHERE wallet = ?1 ORDER BY registered_at ASC"#,
        )
        .bind(wallet)
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter().map(node_from_row).collect()
    }

    // Nodes that registered with an exposed JSON-RPC URL — the targets for the
    // exposed_rpc_loop poller.
    pub async fn list_nodes_with_rpc(&self, network: &str) -> anyhow::Result<Vec<Node>> {
        let rows = sqlx::query(
            r#"SELECT id, wallet, kind, label, rpc_endpoint, network, status,
                last_height, last_block_hash, last_proof_at, registered_at, points, uptime_seconds
                FROM nodes
                WHERE network = ?1
                  AND rpc_endpoint IS NOT NULL
                  AND status != 'suspended'
                ORDER BY registered_at ASC"#,
        )
        .bind(network)
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter().map(node_from_row).collect()
    }

    pub async fn list_all_nodes(&self) -> anyhow::Result<Vec<Node>> {
        let rows = sqlx::query(
            r#"SELECT id, wallet, kind, label, rpc_endpoint, network, status,
                last_height, last_block_hash, last_proof_at, registered_at, points, uptime_seconds
                FROM nodes ORDER BY registered_at ASC"#,
        )
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter().map(node_from_row).collect()
    }

    // Public explorer: nodes that have at least one accepted proof, ordered by
    // most recently active. Filters out registration-only spam.
    pub async fn list_active_nodes(&self, network: &str, limit: i64) -> anyhow::Result<Vec<Node>> {
        let rows = sqlx::query(
            r#"SELECT id, wallet, kind, label, rpc_endpoint, network, status,
                last_height, last_block_hash, last_proof_at, registered_at, points, uptime_seconds
                FROM nodes
                WHERE network = ?1 AND last_proof_at IS NOT NULL
                ORDER BY last_proof_at DESC
                LIMIT ?2"#,
        )
        .bind(network)
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter().map(node_from_row).collect()
    }

    // Global recent proofs feed for the explorer page.
    pub async fn list_recent_proofs(&self, network: &str, limit: i64) -> anyhow::Result<Vec<Proof>> {
        let rows = sqlx::query(
            r#"SELECT p.id, p.node_id, p.wallet, p.claimed_height, p.claimed_block_hash,
                      p.proof_timestamp, p.binary_hash, p.uptime_seconds, p.peers,
                      p.verdict, p.reject_reason, p.points_awarded, p.received_at
               FROM proofs p
               JOIN nodes n ON n.id = p.node_id
               WHERE n.network = ?1
               ORDER BY p.received_at DESC
               LIMIT ?2"#,
        )
        .bind(network)
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter().map(proof_from_row).collect()
    }

    pub async fn update_node_status(&self, id: Uuid, status: NodeStatus) -> anyhow::Result<()> {
        sqlx::query("UPDATE nodes SET status = ?1 WHERE id = ?2")
            .bind(status.as_str())
            .bind(id.to_string())
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn add_uptime_and_points(
        &self,
        id: Uuid,
        uptime_delta_secs: u64,
        points_delta: u64,
    ) -> anyhow::Result<()> {
        sqlx::query(
            "UPDATE nodes SET uptime_seconds = uptime_seconds + ?1, points = points + ?2 WHERE id = ?3",
        )
        .bind(uptime_delta_secs as i64)
        .bind(points_delta as i64)
        .bind(id.to_string())
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn apply_proof_acceptance(
        &self,
        node_id: Uuid,
        height: u64,
        block_hash: &str,
        points_awarded: u64,
        proof_at: DateTime<Utc>,
    ) -> anyhow::Result<()> {
        sqlx::query(
            r#"UPDATE nodes
               SET last_height = ?1,
                   last_block_hash = ?2,
                   last_proof_at = ?3,
                   points = points + ?4,
                   status = 'active'
               WHERE id = ?5"#,
        )
        .bind(height as i64)
        .bind(block_hash)
        .bind(proof_at.to_rfc3339())
        .bind(points_awarded as i64)
        .bind(node_id.to_string())
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    // ---- proofs -------------------------------------------------------------

    pub async fn insert_proof(&self, proof: &Proof) -> anyhow::Result<()> {
        sqlx::query(
            r#"INSERT INTO proofs (id, node_id, wallet, claimed_height, claimed_block_hash,
                proof_timestamp, binary_hash, uptime_seconds, peers, verdict, reject_reason,
                points_awarded, received_at)
                VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)"#,
        )
        .bind(proof.id.to_string())
        .bind(proof.node_id.to_string())
        .bind(&proof.wallet)
        .bind(proof.claimed_height as i64)
        .bind(&proof.claimed_block_hash)
        .bind(proof.proof_timestamp.to_rfc3339())
        .bind(&proof.binary_hash)
        .bind(proof.uptime_seconds.map(|u| u as i64))
        .bind(proof.peers.map(|p| p as i64))
        .bind(proof.verdict.as_str())
        .bind(&proof.reject_reason)
        .bind(proof.points_awarded as i64)
        .bind(proof.received_at.to_rfc3339())
        .execute(&self.pool)
        .await
        .context("inserting proof")?;
        Ok(())
    }

    // Race-safe insert. Returns true if the row was inserted, false if a row
    // with the same (node_id, claimed_height, claimed_block_hash) already
    // existed — concurrent preflight + insert no longer can produce a 500.
    pub async fn try_insert_proof(&self, proof: &Proof) -> anyhow::Result<bool> {
        let res = sqlx::query(
            r#"INSERT OR IGNORE INTO proofs (id, node_id, wallet, claimed_height, claimed_block_hash,
                proof_timestamp, binary_hash, uptime_seconds, peers, verdict, reject_reason,
                points_awarded, received_at)
                VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)"#,
        )
        .bind(proof.id.to_string())
        .bind(proof.node_id.to_string())
        .bind(&proof.wallet)
        .bind(proof.claimed_height as i64)
        .bind(&proof.claimed_block_hash)
        .bind(proof.proof_timestamp.to_rfc3339())
        .bind(&proof.binary_hash)
        .bind(proof.uptime_seconds.map(|u| u as i64))
        .bind(proof.peers.map(|p| p as i64))
        .bind(proof.verdict.as_str())
        .bind(&proof.reject_reason)
        .bind(proof.points_awarded as i64)
        .bind(proof.received_at.to_rfc3339())
        .execute(&self.pool)
        .await
        .context("try-inserting proof")?;
        Ok(res.rows_affected() == 1)
    }

    pub async fn list_proofs_by_wallet(&self, wallet: &str, limit: i64) -> anyhow::Result<Vec<Proof>> {
        let rows = sqlx::query(
            r#"SELECT id, node_id, wallet, claimed_height, claimed_block_hash, proof_timestamp,
                binary_hash, uptime_seconds, peers, verdict, reject_reason, points_awarded, received_at
                FROM proofs WHERE wallet = ?1 ORDER BY received_at DESC LIMIT ?2"#,
        )
        .bind(wallet)
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter().map(proof_from_row).collect()
    }

    pub async fn list_proofs_by_node(&self, node_id: Uuid, limit: i64) -> anyhow::Result<Vec<Proof>> {
        let rows = sqlx::query(
            r#"SELECT id, node_id, wallet, claimed_height, claimed_block_hash, proof_timestamp,
                binary_hash, uptime_seconds, peers, verdict, reject_reason, points_awarded, received_at
                FROM proofs WHERE node_id = ?1 ORDER BY received_at DESC LIMIT ?2"#,
        )
        .bind(node_id.to_string())
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter().map(proof_from_row).collect()
    }

    pub async fn count_proof(&self, node_id: Uuid, height: u64, block_hash: &str) -> anyhow::Result<i64> {
        let count: i64 = sqlx::query_scalar(
            "SELECT COUNT(1) FROM proofs WHERE node_id = ?1 AND claimed_height = ?2 AND claimed_block_hash = ?3",
        )
        .bind(node_id.to_string())
        .bind(height as i64)
        .bind(block_hash)
        .fetch_one(&self.pool)
        .await?;
        Ok(count)
    }

    // Daily aggregates for a node, oldest first. Used by the per-node dashboard's
    // bar chart — date strings are ISO yyyy-mm-dd derived from received_at.
    pub async fn node_daily_series(
        &self,
        node_id: Uuid,
        days: i64,
    ) -> anyhow::Result<Vec<NodeDailyBucket>> {
        let days = days.clamp(1, 90);
        let rows = sqlx::query(
            r#"SELECT substr(received_at, 1, 10) AS day,
                      COUNT(1) AS proofs,
                      SUM(CASE WHEN verdict = 'accepted' THEN 1 ELSE 0 END) AS accepted,
                      COALESCE(SUM(points_awarded), 0) AS points
               FROM proofs
               WHERE node_id = ?1
                 AND received_at >= datetime('now', '-' || ?2 || ' days')
               GROUP BY day
               ORDER BY day ASC"#,
        )
        .bind(node_id.to_string())
        .bind(days)
        .fetch_all(&self.pool)
        .await?;

        rows.into_iter()
            .map(|r| {
                Ok(NodeDailyBucket {
                    day: r.try_get("day")?,
                    proofs: r.try_get::<i64, _>("proofs")? as u64,
                    accepted: r.try_get::<i64, _>("accepted")? as u64,
                    points: r.try_get::<i64, _>("points")? as u64,
                })
            })
            .collect()
    }

    pub async fn last_accepted_proof_for_node(&self, node_id: Uuid) -> anyhow::Result<Option<Proof>> {
        let row = sqlx::query(
            r#"SELECT id, node_id, wallet, claimed_height, claimed_block_hash, proof_timestamp,
                binary_hash, uptime_seconds, peers, verdict, reject_reason, points_awarded, received_at
                FROM proofs WHERE node_id = ?1 AND verdict = 'accepted'
                ORDER BY received_at DESC LIMIT 1"#,
        )
        .bind(node_id.to_string())
        .fetch_optional(&self.pool)
        .await?;
        row.map(proof_from_row).transpose()
    }

    // ---- challenges ---------------------------------------------------------

    pub async fn insert_challenge(&self, ch: &Challenge) -> anyhow::Result<()> {
        sqlx::query(
            r#"INSERT INTO challenges (id, node_id, kind, target_height, expected_hash, issued_at,
                expires_at, status, answered_at, passed)
                VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)"#,
        )
        .bind(ch.id.to_string())
        .bind(ch.node_id.to_string())
        .bind(challenge_kind_str(ch.kind))
        .bind(ch.target_height as i64)
        .bind(&ch.expected_hash)
        .bind(ch.issued_at.to_rfc3339())
        .bind(ch.expires_at.to_rfc3339())
        .bind(ch.status.as_str())
        .bind(ch.answered_at.map(|t| t.to_rfc3339()))
        .bind(ch.passed.map(|p| if p { 1 } else { 0 }))
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn get_open_challenge_for_node(&self, node_id: Uuid) -> anyhow::Result<Option<Challenge>> {
        let row = sqlx::query(
            r#"SELECT id, node_id, kind, target_height, expected_hash, issued_at, expires_at, status,
                answered_at, passed FROM challenges
                WHERE node_id = ?1 AND status = 'open'
                ORDER BY issued_at DESC LIMIT 1"#,
        )
        .bind(node_id.to_string())
        .fetch_optional(&self.pool)
        .await?;
        row.map(challenge_from_row).transpose()
    }

    pub async fn get_challenge(&self, id: Uuid) -> anyhow::Result<Option<Challenge>> {
        let row = sqlx::query(
            r#"SELECT id, node_id, kind, target_height, expected_hash, issued_at, expires_at, status,
                answered_at, passed FROM challenges WHERE id = ?1"#,
        )
        .bind(id.to_string())
        .fetch_optional(&self.pool)
        .await?;
        row.map(challenge_from_row).transpose()
    }

    pub async fn mark_challenge_answered(
        &self,
        id: Uuid,
        passed: bool,
        when: DateTime<Utc>,
    ) -> anyhow::Result<()> {
        sqlx::query(
            r#"UPDATE challenges
               SET status = 'answered', answered_at = ?1, passed = ?2
               WHERE id = ?3"#,
        )
        .bind(when.to_rfc3339())
        .bind(if passed { 1 } else { 0 })
        .bind(id.to_string())
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn expire_old_challenges(&self, now: DateTime<Utc>) -> anyhow::Result<u64> {
        let res = sqlx::query(
            "UPDATE challenges SET status = 'expired' WHERE status = 'open' AND expires_at < ?1",
        )
        .bind(now.to_rfc3339())
        .execute(&self.pool)
        .await?;
        Ok(res.rows_affected())
    }

    // ---- stats --------------------------------------------------------------

    pub async fn network_stats(&self, network: &str) -> anyhow::Result<NetworkStats> {
        // Only count nodes that have produced at least one accepted proof.
        // Sign-up-only bots never get last_proof_at set, so they don't inflate
        // the public counter.
        let total_nodes: i64 = sqlx::query_scalar(
            "SELECT COUNT(1) FROM nodes WHERE network = ?1 AND last_proof_at IS NOT NULL",
        )
        .bind(network)
        .fetch_one(&self.pool)
        .await?;
        let active_nodes: i64 = sqlx::query_scalar(
            "SELECT COUNT(1) FROM nodes WHERE network = ?1 AND status = 'active' AND last_proof_at IS NOT NULL",
        )
        .bind(network)
        .fetch_one(&self.pool)
        .await?;
        let total_proofs: i64 = sqlx::query_scalar(
            "SELECT COUNT(1) FROM proofs p JOIN nodes n ON n.id = p.node_id WHERE n.network = ?1",
        )
        .bind(network)
        .fetch_one(&self.pool)
        .await?;
        let accepted_proofs: i64 = sqlx::query_scalar(
            "SELECT COUNT(1) FROM proofs p JOIN nodes n ON n.id = p.node_id WHERE n.network = ?1 AND p.verdict = 'accepted'",
        )
        .bind(network)
        .fetch_one(&self.pool)
        .await?;
        let total_points: i64 = sqlx::query_scalar(
            "SELECT COALESCE(SUM(points), 0) FROM nodes WHERE network = ?1",
        )
        .bind(network)
        .fetch_one(&self.pool)
        .await?;

        Ok(NetworkStats {
            total_nodes: total_nodes as u32,
            active_nodes: active_nodes as u32,
            total_proofs: total_proofs as u64,
            accepted_proofs: accepted_proofs as u64,
            total_points: total_points as u64,
            network: network.to_string(),
            spl_mint: None,
            solana_cluster: String::new(),
            trusted_tip_height: None,
        })
    }

    pub async fn leaderboard(&self, network: &str, limit: i64) -> anyhow::Result<Vec<WalletStats>> {
        let rows = sqlx::query(
            r#"SELECT wallet,
                COUNT(1) AS nodes,
                COALESCE(SUM(points), 0) AS total_points,
                COALESCE(SUM(uptime_seconds), 0) AS total_uptime,
                MAX(last_proof_at) AS last_seen
                FROM nodes WHERE network = ?1
                GROUP BY wallet
                ORDER BY total_points DESC
                LIMIT ?2"#,
        )
        .bind(network)
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;

        rows.into_iter()
            .map(|row| {
                let last_seen: Option<String> = row.try_get("last_seen")?;
                let last_seen = last_seen
                    .as_deref()
                    .map(parse_dt)
                    .transpose()?
                    .map(Some)
                    .unwrap_or(None);
                Ok(WalletStats {
                    wallet: row.try_get("wallet")?,
                    nodes: row.try_get::<i64, _>("nodes")? as u32,
                    total_points: row.try_get::<i64, _>("total_points")? as u64,
                    total_uptime_seconds: row.try_get::<i64, _>("total_uptime")? as u64,
                    last_seen,
                })
            })
            .collect()
    }

    pub async fn wallet_stats(&self, wallet: &str) -> anyhow::Result<WalletStats> {
        let row = sqlx::query(
            r#"SELECT
                COUNT(1) AS nodes,
                COALESCE(SUM(points), 0) AS total_points,
                COALESCE(SUM(uptime_seconds), 0) AS total_uptime,
                MAX(last_proof_at) AS last_seen
                FROM nodes WHERE wallet = ?1"#,
        )
        .bind(wallet)
        .fetch_one(&self.pool)
        .await?;
        let last_seen: Option<String> = row.try_get("last_seen")?;
        let last_seen = last_seen.as_deref().map(parse_dt).transpose()?;
        Ok(WalletStats {
            wallet: wallet.to_string(),
            nodes: row.try_get::<i64, _>("nodes")? as u32,
            total_points: row.try_get::<i64, _>("total_points")? as u64,
            total_uptime_seconds: row.try_get::<i64, _>("total_uptime")? as u64,
            last_seen,
        })
    }

    // ---- replay protection --------------------------------------------------

    pub async fn try_use_nonce(&self, nonce: &str, wallet: &str) -> anyhow::Result<bool> {
        let res = sqlx::query(
            "INSERT OR IGNORE INTO used_nonces (nonce, wallet, used_at) VALUES (?1, ?2, ?3)",
        )
        .bind(nonce)
        .bind(wallet)
        .bind(Utc::now().to_rfc3339())
        .execute(&self.pool)
        .await?;
        Ok(res.rows_affected() == 1)
    }

    // ---- snapshots ----------------------------------------------------------

    pub async fn insert_snapshot(
        &self,
        cycle: i64,
        merkle_root: &str,
        total_points: u64,
        spl_mint: Option<&str>,
    ) -> anyhow::Result<i64> {
        let row = sqlx::query(
            r#"INSERT INTO snapshots (cycle, merkle_root, total_points, spl_mint, published_at)
                VALUES (?1, ?2, ?3, ?4, ?5)
                RETURNING id"#,
        )
        .bind(cycle)
        .bind(merkle_root)
        .bind(total_points as i64)
        .bind(spl_mint)
        .bind(Utc::now().to_rfc3339())
        .fetch_one(&self.pool)
        .await?;
        Ok(row.try_get::<i64, _>("id")?)
    }

    pub async fn insert_snapshot_leaf(
        &self,
        snapshot_id: i64,
        wallet: &str,
        points: u64,
        leaf_hash: &str,
        proof_json: &str,
    ) -> anyhow::Result<()> {
        sqlx::query(
            r#"INSERT INTO snapshot_leaves (snapshot_id, wallet, points, leaf_hash, proof_json)
                VALUES (?1, ?2, ?3, ?4, ?5)"#,
        )
        .bind(snapshot_id)
        .bind(wallet)
        .bind(points as i64)
        .bind(leaf_hash)
        .bind(proof_json)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn latest_snapshot(&self) -> anyhow::Result<Option<(i64, i64, String, u64)>> {
        let row = sqlx::query(
            "SELECT id, cycle, merkle_root, total_points FROM snapshots ORDER BY cycle DESC LIMIT 1",
        )
        .fetch_optional(&self.pool)
        .await?;
        match row {
            None => Ok(None),
            Some(row) => Ok(Some((
                row.try_get::<i64, _>("id")?,
                row.try_get::<i64, _>("cycle")?,
                row.try_get::<String, _>("merkle_root")?,
                row.try_get::<i64, _>("total_points")? as u64,
            ))),
        }
    }

    pub async fn snapshot_leaf_for_wallet(
        &self,
        snapshot_id: i64,
        wallet: &str,
    ) -> anyhow::Result<Option<(u64, String, String)>> {
        let row = sqlx::query(
            r#"SELECT points, leaf_hash, proof_json FROM snapshot_leaves
                WHERE snapshot_id = ?1 AND wallet = ?2"#,
        )
        .bind(snapshot_id)
        .bind(wallet)
        .fetch_optional(&self.pool)
        .await?;
        match row {
            None => Ok(None),
            Some(row) => Ok(Some((
                row.try_get::<i64, _>("points")? as u64,
                row.try_get::<String, _>("leaf_hash")?,
                row.try_get::<String, _>("proof_json")?,
            ))),
        }
    }

    pub async fn total_points_per_wallet(&self, network: &str) -> anyhow::Result<Vec<(String, u64)>> {
        let rows = sqlx::query(
            r#"SELECT wallet, COALESCE(SUM(points), 0) AS pts
                FROM nodes WHERE network = ?1
                GROUP BY wallet HAVING pts > 0
                ORDER BY wallet ASC"#,
        )
        .bind(network)
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter()
            .map(|r| {
                Ok((
                    r.try_get::<String, _>("wallet")?,
                    r.try_get::<i64, _>("pts")? as u64,
                ))
            })
            .collect()
    }
}

// ---- row mappers ------------------------------------------------------------

fn node_from_row(row: sqlx::sqlite::SqliteRow) -> anyhow::Result<Node> {
    let id_str: String = row.try_get("id")?;
    let kind_str: String = row.try_get("kind")?;
    let status_str: String = row.try_get("status")?;
    let last_height: Option<i64> = row.try_get("last_height")?;
    let last_proof_at: Option<String> = row.try_get("last_proof_at")?;
    let registered_at: String = row.try_get("registered_at")?;
    Ok(Node {
        id: Uuid::parse_str(&id_str)?,
        wallet: row.try_get("wallet")?,
        kind: NodeKind::parse(&kind_str).ok_or_else(|| anyhow!("unknown node kind: {}", kind_str))?,
        label: row.try_get("label")?,
        rpc_endpoint: row.try_get("rpc_endpoint")?,
        network: row.try_get("network")?,
        status: NodeStatus::parse(&status_str)
            .ok_or_else(|| anyhow!("unknown node status: {}", status_str))?,
        last_height: last_height.map(|h| h as u64),
        last_block_hash: row.try_get("last_block_hash")?,
        last_proof_at: last_proof_at.as_deref().map(parse_dt).transpose()?,
        registered_at: parse_dt(&registered_at)?,
        points: row.try_get::<i64, _>("points")? as u64,
        uptime_seconds: row.try_get::<i64, _>("uptime_seconds")? as u64,
    })
}

fn proof_from_row(row: sqlx::sqlite::SqliteRow) -> anyhow::Result<Proof> {
    let id_str: String = row.try_get("id")?;
    let node_id_str: String = row.try_get("node_id")?;
    let verdict_str: String = row.try_get("verdict")?;
    let proof_timestamp: String = row.try_get("proof_timestamp")?;
    let received_at: String = row.try_get("received_at")?;
    let uptime: Option<i64> = row.try_get("uptime_seconds")?;
    let peers: Option<i64> = row.try_get("peers")?;
    Ok(Proof {
        id: Uuid::parse_str(&id_str)?,
        node_id: Uuid::parse_str(&node_id_str)?,
        wallet: row.try_get("wallet")?,
        claimed_height: row.try_get::<i64, _>("claimed_height")? as u64,
        claimed_block_hash: row.try_get("claimed_block_hash")?,
        proof_timestamp: parse_dt(&proof_timestamp)?,
        binary_hash: row.try_get("binary_hash")?,
        uptime_seconds: uptime.map(|u| u as u64),
        peers: peers.map(|p| p as u32),
        verdict: ProofVerdict::parse(&verdict_str)
            .ok_or_else(|| anyhow!("unknown verdict: {}", verdict_str))?,
        reject_reason: row.try_get("reject_reason")?,
        points_awarded: row.try_get::<i64, _>("points_awarded")? as u64,
        received_at: parse_dt(&received_at)?,
    })
}

fn challenge_from_row(row: sqlx::sqlite::SqliteRow) -> anyhow::Result<Challenge> {
    let id_str: String = row.try_get("id")?;
    let node_id_str: String = row.try_get("node_id")?;
    let kind_str: String = row.try_get("kind")?;
    let status_str: String = row.try_get("status")?;
    let issued_at: String = row.try_get("issued_at")?;
    let expires_at: String = row.try_get("expires_at")?;
    let answered_at: Option<String> = row.try_get("answered_at")?;
    let passed: Option<i64> = row.try_get("passed")?;
    Ok(Challenge {
        id: Uuid::parse_str(&id_str)?,
        node_id: Uuid::parse_str(&node_id_str)?,
        kind: parse_challenge_kind(&kind_str)?,
        target_height: row.try_get::<i64, _>("target_height")? as u64,
        expected_hash: row.try_get("expected_hash")?,
        issued_at: parse_dt(&issued_at)?,
        expires_at: parse_dt(&expires_at)?,
        status: ChallengeStatus::parse(&status_str)
            .ok_or_else(|| anyhow!("unknown challenge status: {}", status_str))?,
        answered_at: answered_at.as_deref().map(parse_dt).transpose()?,
        passed: passed.map(|p| p != 0),
    })
}

fn challenge_kind_str(k: ChallengeKind) -> &'static str {
    match k {
        ChallengeKind::BlockHash => "block_hash",
    }
}

fn parse_challenge_kind(s: &str) -> anyhow::Result<ChallengeKind> {
    match s {
        "block_hash" => Ok(ChallengeKind::BlockHash),
        other => Err(anyhow!("unknown challenge kind: {}", other)),
    }
}

fn parse_dt(s: &str) -> anyhow::Result<DateTime<Utc>> {
    Ok(DateTime::parse_from_rfc3339(s)
        .with_context(|| format!("parsing rfc3339 timestamp {:?}", s))?
        .with_timezone(&Utc))
}
