use axum::{
    extract::{Path, Query, State},
    Json,
};
use serde::Deserialize;

use crate::{
    auth,
    error::{AppError, AppResult},
    state::AppState,
    types::{NetworkStats, WalletStats},
};

pub async fn network(State(state): State<AppState>) -> AppResult<Json<NetworkStats>> {
    let cfg = state.config();
    let mut s = state.store().network_stats(cfg.network.as_str()).await?;
    s.spl_mint = cfg.spl_mint.clone();
    s.solana_cluster = cfg.solana_cluster.clone();
    s.trusted_tip_height = state.trusted_tip().await;
    Ok(Json(s))
}

#[derive(Debug, Deserialize)]
pub struct LeaderboardQuery {
    #[serde(default = "default_limit")]
    pub limit: i64,
}

fn default_limit() -> i64 {
    100
}

pub async fn leaderboard(
    State(state): State<AppState>,
    Query(q): Query<LeaderboardQuery>,
) -> AppResult<Json<Vec<WalletStats>>> {
    let limit = q.limit.clamp(1, 500);
    let rows = state.store().leaderboard(state.config().network.as_str(), limit).await?;
    Ok(Json(rows))
}

pub async fn wallet_stats(
    State(state): State<AppState>,
    Path(wallet): Path<String>,
) -> AppResult<Json<WalletStats>> {
    auth::decode_solana_pubkey(&wallet).map_err(AppError::from)?;
    let stats = state.store().wallet_stats(&wallet).await?;
    Ok(Json(stats))
}
