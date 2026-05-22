use chrono::{Duration as ChronoDuration, Utc};
use serde_json::json;
use std::time::Duration;
use tokio::time::{interval, Instant};
use uuid::Uuid;

use crate::{
    rpc::RpcError,
    state::AppState,
    types::{Node, NodeStatus, Proof, ProofVerdict},
};

pub fn spawn(state: AppState) {
    tokio::spawn(tip_refresh_loop(state.clone()));
    tokio::spawn(uptime_loop(state.clone()));
    tokio::spawn(staleness_loop(state.clone()));
    tokio::spawn(challenge_expiry_loop(state.clone()));
    if state.config().exposed_rpc_poll_interval.is_some() {
        tokio::spawn(exposed_rpc_loop(state.clone()));
    }
    if let Some(_) = state.config().snapshot_interval {
        tokio::spawn(snapshot_loop(state));
    }
}

async fn tip_refresh_loop(state: AppState) {
    let mut tick = interval(state.config().heartbeat_interval.max(Duration::from_secs(15)));
    tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    if !state.rpc().is_configured() {
        tracing::warn!("tip_refresh_loop: no trusted rpcs — skipping");
        return;
    }
    loop {
        tick.tick().await;
        match state.rpc().get_block_count().await {
            Ok(h) => {
                state.set_trusted_tip(h).await;
                tracing::debug!(tip = h, "trusted tip refreshed");
            }
            Err(e) => tracing::warn!(error = ?e, "tip refresh failed"),
        }
    }
}

// Award uptime points for nodes that produced an accepted proof within the
// last `2 * uptime_reward_interval`. Mirrors DePINonBNB's uptime ticker concept.
async fn uptime_loop(state: AppState) {
    let interval_dur = state.config().uptime_reward_interval.max(Duration::from_secs(60));
    let mut tick = interval(interval_dur);
    tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    // Skip the first immediate tick — gives the server a moment to come up before crediting.
    tick.tick().await;
    let _start = Instant::now();

    loop {
        tick.tick().await;
        match state.store().list_all_nodes().await {
            Ok(nodes) => {
                let cutoff =
                    Utc::now() - ChronoDuration::from_std(interval_dur * 2).unwrap_or(ChronoDuration::minutes(10));
                for node in nodes {
                    if node.status == NodeStatus::Suspended {
                        continue;
                    }
                    let Some(last_proof) = node.last_proof_at else { continue };
                    if last_proof < cutoff {
                        continue;
                    }
                    // 1 point per tier-unit per tick.
                    let pts = node.kind.reward_tier() as u64;
                    if let Err(e) = state
                        .store()
                        .add_uptime_and_points(node.id, interval_dur.as_secs(), pts)
                        .await
                    {
                        tracing::warn!(error = ?e, node_id = %node.id, "uptime credit failed");
                    }
                }
            }
            Err(e) => tracing::warn!(error = ?e, "uptime loop list_all_nodes failed"),
        }
    }
}

// Mark nodes Stale if no proof in last `staleness_threshold`. Resurrects to Active
// on next accepted proof (handled in proofs handler).
async fn staleness_loop(state: AppState) {
    let dur = state.config().challenge_check_interval.max(Duration::from_secs(60));
    let mut tick = interval(dur * 2);
    tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    loop {
        tick.tick().await;
        let stale_after = ChronoDuration::from_std(state.config().heartbeat_interval * 6)
            .unwrap_or(ChronoDuration::minutes(30));
        let cutoff = Utc::now() - stale_after;
        let nodes = match state.store().list_all_nodes().await {
            Ok(n) => n,
            Err(e) => {
                tracing::warn!(error = ?e, "staleness list failed");
                continue;
            }
        };
        for node in nodes {
            if node.status != NodeStatus::Active {
                continue;
            }
            let Some(last) = node.last_proof_at else { continue };
            if last < cutoff {
                if let Err(e) = state.store().update_node_status(node.id, NodeStatus::Stale).await {
                    tracing::warn!(error = ?e, node_id = %node.id, "marking stale failed");
                }
            }
        }
    }
}

async fn challenge_expiry_loop(state: AppState) {
    let mut tick = interval(state.config().challenge_check_interval.max(Duration::from_secs(30)));
    tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    loop {
        tick.tick().await;
        match state.store().expire_old_challenges(Utc::now()).await {
            Ok(n) if n > 0 => tracing::info!(expired = n, "challenges expired"),
            Ok(_) => {}
            Err(e) => tracing::warn!(error = ?e, "challenge expiry failed"),
        }
    }
}

// Exposed RPC verification mode.
//
// Operators register with a public `rpc_endpoint`. This loop polls each one
// every `EXPOSED_RPC_POLL_INTERVAL`, asks for its current tip, cross-checks
// the block hash against the trusted quorum, and — if it matches — credits
// the node as if the operator had submitted a signed proof.
//
// This is the "zero-install" path: no relay binary on the node side. The
// operator's reachability (we can fetch the URL) and their RPC's truthfulness
// (hash matches the quorum) substitute for the signed proof.
async fn exposed_rpc_loop(state: AppState) {
    let Some(poll_interval) = state.config().exposed_rpc_poll_interval else {
        return;
    };
    if !state.rpc().is_configured() {
        tracing::warn!("exposed_rpc_loop: no trusted rpcs — disabling (cannot verify without quorum)");
        return;
    }
    let interval_dur = poll_interval.max(Duration::from_secs(60));
    let mut tick = interval(interval_dur);
    tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    tick.tick().await; // skip immediate fire

    loop {
        tick.tick().await;
        let nodes = match state
            .store()
            .list_nodes_with_rpc(state.config().network.as_str())
            .await
        {
            Ok(n) => n,
            Err(e) => {
                tracing::warn!(error = ?e, "exposed_rpc_loop: list_nodes_with_rpc failed");
                continue;
            }
        };
        if nodes.is_empty() {
            continue;
        }
        let trusted_tip = state.trusted_tip().await;
        for node in nodes {
            if let Err(e) = poll_one_node(&state, &node, trusted_tip).await {
                tracing::warn!(error = ?e, node_id = %node.id, "exposed_rpc poll failed");
            }
        }
    }
}

async fn poll_one_node(state: &AppState, node: &Node, trusted_tip: Option<u64>) -> anyhow::Result<()> {
    let Some(endpoint) = node.rpc_endpoint.as_deref() else {
        return Ok(());
    };

    // 1) Ask the operator's RPC for its tip + hash at that tip.
    let height_v = state.rpc().call_single(endpoint, "getblockcount", json!([])).await?;
    let height = height_v
        .as_u64()
        .ok_or_else(|| anyhow::anyhow!("getblockcount returned non-u64: {height_v}"))?;
    let hash_v = state.rpc().call_single(endpoint, "getblockhash", json!([height])).await?;
    let claimed_hash = hash_v
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("getblockhash returned non-string: {hash_v}"))?
        .to_string();

    // 2) Drift check against the trusted tip.
    let cfg = state.config();
    if let Some(tip) = trusted_tip {
        if height + cfg.max_height_drift < tip || height > tip + cfg.max_height_drift {
            tracing::debug!(node_id = %node.id, height, tip, "exposed_rpc: out of drift, skipping");
            return Ok(());
        }
    }

    // 3) Cross-check the hash against the trusted quorum at the same height.
    let trusted_hash = match state.rpc().get_block_hash(height).await {
        Ok(h) => h,
        Err(RpcError::NoQuorum) => {
            tracing::debug!(node_id = %node.id, height, "exposed_rpc: trusted quorum has no answer yet");
            return Ok(());
        }
        Err(e) => return Err(anyhow::anyhow!("quorum get_block_hash: {e}")),
    };
    let claimed = normalize_hash(&claimed_hash);
    let expected = normalize_hash(&trusted_hash);
    let verdict = if claimed == expected {
        ProofVerdict::Accepted
    } else {
        tracing::warn!(
            node_id = %node.id,
            height,
            claimed = %claimed,
            expected = %expected,
            "exposed_rpc: hash mismatch — not crediting"
        );
        ProofVerdict::Rejected
    };

    // 4) Build a synthetic proof row. The "wallet signature" check is replaced by
    //    the operator-controlled rpc_endpoint they signed during registration.
    let drift = trusted_tip
        .map(|t| t.saturating_sub(height))
        .unwrap_or(0);
    let points = if verdict == ProofVerdict::Accepted {
        // No uptime/peers signal from the operator here — use the freshness +
        // tier components only. Matches the lower bound of a relay-mode proof.
        let freshness = 5u64.saturating_sub(drift);
        (node.kind.reward_tier() as u64).saturating_mul(1 + freshness)
    } else {
        0
    };

    let now = Utc::now();
    let proof = Proof {
        id: Uuid::new_v4(),
        node_id: node.id,
        wallet: node.wallet.clone(),
        claimed_height: height,
        claimed_block_hash: claimed_hash.clone(),
        proof_timestamp: now,
        binary_hash: Some("exposed-rpc-poll".to_string()),
        uptime_seconds: None,
        peers: None,
        verdict,
        reject_reason: if verdict == ProofVerdict::Accepted {
            None
        } else {
            Some("exposed-rpc: hash mismatch with trusted quorum".to_string())
        },
        points_awarded: points,
        received_at: now,
    };

    let inserted = state.store().try_insert_proof(&proof).await?;
    if !inserted {
        // Same (node, height, hash) already on file — operator's tip hasn't moved
        // since our last poll. No-op, no double credit.
        return Ok(());
    }

    if verdict == ProofVerdict::Accepted {
        state
            .store()
            .apply_proof_acceptance(node.id, height, &claimed_hash, points, now)
            .await?;
        tracing::info!(
            node_id = %node.id,
            height,
            points,
            "exposed_rpc: accepted proof and credited"
        );
    }
    Ok(())
}

fn normalize_hash(s: &str) -> String {
    s.trim().trim_start_matches("0x").to_lowercase()
}

async fn snapshot_loop(state: AppState) {
    let Some(snap_interval) = state.config().snapshot_interval else {
        return;
    };
    let mut tick = interval(snap_interval);
    tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    // Skip immediate tick — give operators a chance to earn before first snapshot.
    tick.tick().await;
    loop {
        tick.tick().await;
        match crate::merkle::publish_snapshot(&state).await {
            Ok(res) => tracing::info!(cycle = res.cycle, leaves = res.leaves, root = %res.merkle_root, "scheduled snapshot published"),
            Err(e) => tracing::warn!(error = ?e, "scheduled snapshot failed"),
        }
    }
}
