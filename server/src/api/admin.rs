use axum::{extract::State, http::HeaderMap, Json};
use serde::Serialize;

use crate::{error::AppError, merkle, state::AppState};

#[derive(Debug, Serialize)]
pub struct PublishSnapshotResponse {
    pub cycle: i64,
    pub merkle_root: String,
    pub leaves: usize,
    pub total_points: u64,
}

pub async fn publish_snapshot(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<PublishSnapshotResponse>, AppError> {
    require_admin(&state, &headers)?;

    let resp = merkle::publish_snapshot(&state).await?;
    Ok(Json(PublishSnapshotResponse {
        cycle: resp.cycle,
        merkle_root: resp.merkle_root,
        leaves: resp.leaves,
        total_points: resp.total_points,
    }))
}

fn require_admin(state: &AppState, headers: &HeaderMap) -> Result<(), AppError> {
    let configured = state
        .config()
        .admin_api_key
        .as_deref()
        .ok_or(AppError::Forbidden)?;
    let provided = headers
        .get("x-admin-key")
        .and_then(|v| v.to_str().ok())
        .ok_or(AppError::Unauthorized)?;
    if !constant_time_eq(configured.as_bytes(), provided.as_bytes()) {
        return Err(AppError::Unauthorized);
    }
    Ok(())
}

fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut acc = 0u8;
    for (x, y) in a.iter().zip(b.iter()) {
        acc |= x ^ y;
    }
    acc == 0
}

