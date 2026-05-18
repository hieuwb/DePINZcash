use axum::{
    extract::{Path, Query, State},
    Json,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    auth::{self},
    error::{AppError, AppResult},
    rpc::RpcError,
    state::AppState,
    types::{Node, NodeStatus, Proof, ProofVerdict},
};

#[derive(Debug, Deserialize)]
pub struct SubmitProofRequest {
    pub wallet: String,
    pub node_id: Uuid,
    pub signature: String,
    pub nonce: String,
    pub claimed_height: u64,
    pub claimed_block_hash: String,
    pub proof_timestamp: DateTime<Utc>,
    #[serde(default)]
    pub binary_hash: Option<String>,
    #[serde(default)]
    pub uptime_seconds: Option<u64>,
    #[serde(default)]
    pub peers: Option<u32>,
}

#[derive(Debug, Serialize)]
pub struct SubmitProofResponse {
    pub proof_id: Uuid,
    pub verdict: String,
    pub reject_reason: Option<String>,
    pub points_awarded: u64,
    pub trusted_tip_height: Option<u64>,
    pub trusted_block_hash: Option<String>,
}

pub async fn submit(
    State(state): State<AppState>,
    Json(req): Json<SubmitProofRequest>,
) -> AppResult<Json<SubmitProofResponse>> {
    // ---- basic shape checks ------------------------------------------------
    if req.claimed_block_hash.is_empty() || req.claimed_block_hash.len() > 128 {
        return Err(AppError::bad_request("claimed_block_hash empty or oversized"));
    }
    auth::check_nonce(&req.nonce).map_err(AppError::from)?;
    auth::check_timestamp(req.proof_timestamp, state.config().max_clock_skew).map_err(AppError::from)?;

    let store = state.store();
    let node = store
        .get_node(req.node_id)
        .await?
        .ok_or(AppError::NotFound)?;
    if node.wallet != req.wallet {
        return Err(AppError::bad_request("wallet does not match node owner"));
    }
    if node.status == NodeStatus::Suspended {
        return Err(AppError::Forbidden);
    }
    if node.network != state.config().network.as_str() {
        return Err(AppError::bad_request(format!(
            "node registered on network '{}', server is configured for '{}'",
            node.network,
            state.config().network.as_str()
        )));
    }

    // ---- signature verification --------------------------------------------
    let msg = auth::proof_message(
        &req.wallet,
        &req.node_id.to_string(),
        req.claimed_height,
        &req.claimed_block_hash,
        &req.proof_timestamp.to_rfc3339(),
        &req.nonce,
    );
    auth::verify_solana_signature(&req.wallet, &msg, &req.signature)
        .map_err(AppError::from)?;

    // ---- replay prevention -------------------------------------------------
    if !store.try_use_nonce(&req.nonce, &req.wallet).await? {
        return Err(AppError::conflict("nonce already used"));
    }
    if store
        .count_proof(req.node_id, req.claimed_height, &req.claimed_block_hash)
        .await?
        > 0
    {
        return Err(AppError::conflict("proof for (height, hash) already submitted by this node"));
    }

    // ---- monotonic height check --------------------------------------------
    if let Some(last) = node.last_height {
        if req.claimed_height + 1024 < last {
            return Err(AppError::bad_request(format!(
                "claimed_height {} is far behind last accepted {}",
                req.claimed_height, last
            )));
        }
    }

    // ---- verify against trusted RPC quorum --------------------------------
    let cfg = state.config();
    let rpc = state.rpc();
    let trusted_tip = state.trusted_tip().await;

    let (verdict, reject_reason, trusted_hash) = if !rpc.is_configured() {
        // Permissive mode for dev / pre-launch — accept but flag for audit.
        tracing::warn!(node_id = %req.node_id, "no trusted RPCs configured — accepting proof in permissive mode");
        (ProofVerdict::Accepted, Some("permissive-mode:no-trusted-rpcs".to_string()), None)
    } else {
        match rpc.get_block_hash(req.claimed_height).await {
            Ok(hash) => {
                let expected = normalize_hash(&hash);
                let claimed = normalize_hash(&req.claimed_block_hash);
                if expected != claimed {
                    (
                        ProofVerdict::Rejected,
                        Some(format!(
                            "block hash mismatch at height {}: expected {} got {}",
                            req.claimed_height, expected, claimed
                        )),
                        Some(hash),
                    )
                } else if let Some(tip) = trusted_tip {
                    if req.claimed_height + cfg.max_height_drift < tip {
                        (
                            ProofVerdict::Rejected,
                            Some(format!(
                                "claimed_height {} too far behind trusted tip {}",
                                req.claimed_height, tip
                            )),
                            Some(hash),
                        )
                    } else if req.claimed_height > tip + cfg.max_height_drift {
                        (
                            ProofVerdict::Rejected,
                            Some(format!(
                                "claimed_height {} ahead of trusted tip {}",
                                req.claimed_height, tip
                            )),
                            Some(hash),
                        )
                    } else {
                        (ProofVerdict::Accepted, None, Some(hash))
                    }
                } else {
                    // Tip not yet cached; height matched hash — accept.
                    (ProofVerdict::Accepted, None, Some(hash))
                }
            }
            Err(RpcError::NoQuorum) => (
                ProofVerdict::Pending,
                Some("trusted-quorum-failed-to-agree".into()),
                None,
            ),
            Err(e) => {
                tracing::warn!(error = ?e, "trusted RPC verification failed");
                (
                    ProofVerdict::Pending,
                    Some(format!("trusted-rpc-error: {e}")),
                    None,
                )
            }
        }
    };

    let points_awarded = if verdict == ProofVerdict::Accepted {
        calculate_points(&node, &req, trusted_tip)
    } else {
        0
    };

    let proof = Proof {
        id: Uuid::new_v4(),
        node_id: req.node_id,
        wallet: req.wallet.clone(),
        claimed_height: req.claimed_height,
        claimed_block_hash: req.claimed_block_hash.clone(),
        proof_timestamp: req.proof_timestamp,
        binary_hash: req.binary_hash.clone(),
        uptime_seconds: req.uptime_seconds,
        peers: req.peers,
        verdict,
        reject_reason: reject_reason.clone(),
        points_awarded,
        received_at: Utc::now(),
    };
    store.insert_proof(&proof).await?;

    if verdict == ProofVerdict::Accepted {
        store
            .apply_proof_acceptance(
                node.id,
                req.claimed_height,
                &req.claimed_block_hash,
                points_awarded,
                req.proof_timestamp,
            )
            .await?;
    }

    Ok(Json(SubmitProofResponse {
        proof_id: proof.id,
        verdict: verdict.as_str().to_string(),
        reject_reason,
        points_awarded,
        trusted_tip_height: trusted_tip,
        trusted_block_hash: trusted_hash,
    }))
}

#[derive(Debug, Deserialize)]
pub struct ListQuery {
    #[serde(default = "default_limit")]
    pub limit: i64,
}

fn default_limit() -> i64 {
    50
}

pub async fn list_for_wallet(
    State(state): State<AppState>,
    Path(wallet): Path<String>,
    Query(q): Query<ListQuery>,
) -> AppResult<Json<Vec<Proof>>> {
    auth::decode_solana_pubkey(&wallet).map_err(AppError::from)?;
    let limit = q.limit.clamp(1, 500);
    let proofs = state.store().list_proofs_by_wallet(&wallet, limit).await?;
    Ok(Json(proofs))
}

// Reward formula:
//   base = node.kind.reward_tier()                       (10 for zebra-full, 6 for lwd)
//   freshness = max(0, 5 - drift_from_tip)               (0..=5)
//   uptime_bonus = min(uptime_hours, 24)                 (0..=24)
//   peers_bonus = min(peers / 4, 3)                      (0..=3)
//   points = base * (1 + freshness) + uptime_bonus + peers_bonus
fn calculate_points(node: &Node, req: &SubmitProofRequest, trusted_tip: Option<u64>) -> u64 {
    let base = node.kind.reward_tier() as u64;
    let drift = match trusted_tip {
        Some(tip) if tip >= req.claimed_height => tip - req.claimed_height,
        _ => 0,
    };
    let freshness = 5u64.saturating_sub(drift);
    let uptime_hours = req.uptime_seconds.unwrap_or(0) / 3600;
    let uptime_bonus = uptime_hours.min(24);
    let peers_bonus = (req.peers.unwrap_or(0) as u64 / 4).min(3);
    base.saturating_mul(1 + freshness) + uptime_bonus + peers_bonus
}

fn normalize_hash(s: &str) -> String {
    let stripped = s.trim().trim_start_matches("0x").to_lowercase();
    stripped
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::NodeKind;

    fn dummy_node(kind: NodeKind) -> Node {
        Node {
            id: Uuid::new_v4(),
            wallet: "Wallet".into(),
            kind,
            label: None,
            rpc_endpoint: None,
            network: "mainnet".into(),
            status: NodeStatus::Active,
            last_height: None,
            last_block_hash: None,
            last_proof_at: None,
            registered_at: Utc::now(),
            points: 0,
            uptime_seconds: 0,
        }
    }

    fn dummy_req(height: u64, uptime: u64, peers: u32) -> SubmitProofRequest {
        SubmitProofRequest {
            wallet: "w".into(),
            node_id: Uuid::nil(),
            signature: "s".into(),
            nonce: "n".into(),
            claimed_height: height,
            claimed_block_hash: "h".into(),
            proof_timestamp: Utc::now(),
            binary_hash: None,
            uptime_seconds: Some(uptime),
            peers: Some(peers),
        }
    }

    #[test]
    fn points_full_credit_when_at_tip() {
        let node = dummy_node(NodeKind::ZebraFull);
        let req = dummy_req(100, 3600 * 12, 16);
        let pts = calculate_points(&node, &req, Some(100));
        // base 10 * (1 + 5) = 60, +12 uptime, +3 peers = 75
        assert_eq!(pts, 75);
    }

    #[test]
    fn points_penalise_drift() {
        let node = dummy_node(NodeKind::ZebraFull);
        let req = dummy_req(95, 0, 0);
        let pts = calculate_points(&node, &req, Some(100));
        // drift=5 → freshness=0 → base*(1+0)=10
        assert_eq!(pts, 10);
    }

    #[test]
    fn points_lwd_tier_lower() {
        let node = dummy_node(NodeKind::Lightwalletd);
        let req = dummy_req(100, 0, 0);
        let pts = calculate_points(&node, &req, Some(100));
        // base 6 * 6 = 36
        assert_eq!(pts, 36);
    }

    #[test]
    fn normalize_hash_accepts_0x() {
        assert_eq!(normalize_hash("0xAB"), "ab");
        assert_eq!(normalize_hash("CD"), "cd");
    }
}
