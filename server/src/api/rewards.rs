use axum::{
    extract::{Path, State},
    Json,
};
use serde::Serialize;
use serde_json::Value;

use crate::{
    auth,
    error::{AppError, AppResult},
    state::AppState,
};

#[derive(Debug, Serialize)]
pub struct LatestSnapshot {
    pub cycle: i64,
    pub merkle_root: String,
    pub total_points: u64,
    pub spl_mint: Option<String>,
    pub solana_cluster: String,
}

pub async fn latest_snapshot(State(state): State<AppState>) -> AppResult<Json<LatestSnapshot>> {
    let snap = state.store().latest_snapshot().await?.ok_or(AppError::NotFound)?;
    Ok(Json(LatestSnapshot {
        cycle: snap.1,
        merkle_root: snap.2,
        total_points: snap.3,
        spl_mint: state.config().spl_mint.clone(),
        solana_cluster: state.config().solana_cluster.clone(),
    }))
}

#[derive(Debug, Serialize)]
pub struct ClaimResponse {
    pub wallet: String,
    pub cycle: i64,
    pub merkle_root: String,
    pub points: u64,
    pub leaf_hash: String,
    pub proof: Value,
    pub spl_mint: Option<String>,
    pub solana_cluster: String,
}

pub async fn latest_claim(
    State(state): State<AppState>,
    Path(wallet): Path<String>,
) -> AppResult<Json<ClaimResponse>> {
    auth::decode_solana_pubkey(&wallet).map_err(AppError::from)?;
    let snap = state.store().latest_snapshot().await?.ok_or(AppError::NotFound)?;
    let leaf = state
        .store()
        .snapshot_leaf_for_wallet(snap.0, &wallet)
        .await?
        .ok_or(AppError::NotFound)?;
    let proof: Value = serde_json::from_str(&leaf.2)
        .map_err(|e| AppError::Internal(anyhow::Error::new(e)))?;
    Ok(Json(ClaimResponse {
        wallet,
        cycle: snap.1,
        merkle_root: snap.2,
        points: leaf.0,
        leaf_hash: leaf.1,
        proof,
        spl_mint: state.config().spl_mint.clone(),
        solana_cluster: state.config().solana_cluster.clone(),
    }))
}
