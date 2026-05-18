use axum::{extract::State, Json};
use chrono::{Duration as ChronoDuration, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    auth,
    error::{AppError, AppResult},
    state::AppState,
    types::{Challenge, ChallengeKind, ChallengeStatus},
};

#[derive(Debug, Deserialize)]
pub struct RequestChallengeRequest {
    pub node_id: Uuid,
    pub wallet: String,
    pub signature: String,
    pub nonce: String,
    pub timestamp: chrono::DateTime<Utc>,
}

#[derive(Debug, Serialize)]
pub struct RequestChallengeResponse {
    pub challenge_id: Uuid,
    pub target_height: u64,
    pub expires_at: chrono::DateTime<Utc>,
}

pub async fn request(
    State(state): State<AppState>,
    Json(req): Json<RequestChallengeRequest>,
) -> AppResult<Json<RequestChallengeResponse>> {
    auth::check_nonce(&req.nonce).map_err(AppError::from)?;
    auth::check_timestamp(req.timestamp, state.config().max_clock_skew).map_err(AppError::from)?;

    let node = state.store().get_node(req.node_id).await?.ok_or(AppError::NotFound)?;
    if node.wallet != req.wallet {
        return Err(AppError::bad_request("wallet does not match node"));
    }

    let msg = format!(
        "depinzcash:challenge:request:v1\n{}\n{}\n{}\n{}\n",
        req.wallet,
        req.node_id,
        req.nonce,
        req.timestamp.to_rfc3339()
    );
    auth::verify_solana_signature(&req.wallet, msg.as_bytes(), &req.signature).map_err(AppError::from)?;

    if !state.store().try_use_nonce(&req.nonce, &req.wallet).await? {
        return Err(AppError::conflict("nonce already used"));
    }

    let rpc = state.rpc();
    if !rpc.is_configured() {
        return Err(AppError::Upstream("no trusted rpcs configured".into()));
    }

    let tip = rpc.get_block_count().await.map_err(|e| AppError::Upstream(e.to_string()))?;
    let depth = ChallengeDepth::pick(tip);
    let target = tip.saturating_sub(depth);
    let expected = rpc.get_block_hash(target).await.map_err(|e| AppError::Upstream(e.to_string()))?;

    let issued = Utc::now();
    let challenge = Challenge {
        id: Uuid::new_v4(),
        node_id: node.id,
        kind: ChallengeKind::BlockHash,
        target_height: target,
        expected_hash: expected,
        issued_at: issued,
        expires_at: issued + ChronoDuration::minutes(10),
        status: ChallengeStatus::Open,
        answered_at: None,
        passed: None,
    };
    state.store().insert_challenge(&challenge).await?;

    Ok(Json(RequestChallengeResponse {
        challenge_id: challenge.id,
        target_height: target,
        expires_at: challenge.expires_at,
    }))
}

#[derive(Debug, Deserialize)]
pub struct SubmitChallengeRequest {
    pub challenge_id: Uuid,
    pub wallet: String,
    pub signature: String,
    pub answer_block_hash: String,
    pub nonce: String,
    pub timestamp: chrono::DateTime<Utc>,
}

#[derive(Debug, Serialize)]
pub struct SubmitChallengeResponse {
    pub passed: bool,
    pub expected_hash: String,
}

pub async fn submit(
    State(state): State<AppState>,
    Json(req): Json<SubmitChallengeRequest>,
) -> AppResult<Json<SubmitChallengeResponse>> {
    auth::check_nonce(&req.nonce).map_err(AppError::from)?;
    auth::check_timestamp(req.timestamp, state.config().max_clock_skew).map_err(AppError::from)?;

    let challenge = state
        .store()
        .get_challenge(req.challenge_id)
        .await?
        .ok_or(AppError::NotFound)?;

    if challenge.status != ChallengeStatus::Open {
        return Err(AppError::conflict("challenge already resolved or expired"));
    }
    if challenge.expires_at < Utc::now() {
        state
            .store()
            .mark_challenge_answered(challenge.id, false, Utc::now())
            .await?;
        return Err(AppError::conflict("challenge expired"));
    }

    let node = state
        .store()
        .get_node(challenge.node_id)
        .await?
        .ok_or(AppError::NotFound)?;
    if node.wallet != req.wallet {
        return Err(AppError::bad_request("wallet does not match challenge node"));
    }

    let msg = format!(
        "depinzcash:challenge:answer:v1\n{}\n{}\n{}\n{}\n{}\n",
        req.wallet,
        req.challenge_id,
        req.answer_block_hash,
        req.nonce,
        req.timestamp.to_rfc3339()
    );
    auth::verify_solana_signature(&req.wallet, msg.as_bytes(), &req.signature).map_err(AppError::from)?;

    if !state.store().try_use_nonce(&req.nonce, &req.wallet).await? {
        return Err(AppError::conflict("nonce already used"));
    }

    let expected = challenge.expected_hash.trim().to_lowercase();
    let answer = req.answer_block_hash.trim().trim_start_matches("0x").to_lowercase();
    let passed = expected.trim_start_matches("0x") == answer;

    state
        .store()
        .mark_challenge_answered(challenge.id, passed, Utc::now())
        .await?;

    if passed {
        // Small bonus for surviving an audit.
        state
            .store()
            .add_uptime_and_points(node.id, 0, node.kind.reward_tier() as u64)
            .await?;
    }

    Ok(Json(SubmitChallengeResponse {
        passed,
        expected_hash: challenge.expected_hash,
    }))
}

// Picks a sensible past-block depth to challenge. Recent enough that an honest synced
// node has it, deep enough that a freshly-bootstrapped fake won't have it yet.
struct ChallengeDepth;

impl ChallengeDepth {
    fn pick(tip: u64) -> u64 {
        if tip < 256 {
            return tip / 2;
        }
        // Random-ish depth between 32 and 256 blocks back. Deterministic on tip so
        // a single tip exposes the same challenge surface, but rotation across blocks
        // makes precomputed answers expensive.
        let span = 256 - 32;
        let pick = (tip % span as u64) + 32;
        pick
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pick_depth_in_range() {
        assert!(ChallengeDepth::pick(50) <= 50);
        for tip in [300u64, 1_000, 100_000, 2_000_000] {
            let d = ChallengeDepth::pick(tip);
            assert!(d >= 32 && d <= 256, "depth out of range: {d}");
        }
    }
}
