use axum::{extract::State, Json};
use serde_json::{json, Value};

use crate::state::AppState;

pub async fn healthz() -> Json<Value> {
    Json(json!({ "status": "ok" }))
}

pub async fn readyz(State(state): State<AppState>) -> Json<Value> {
    let db_ok = sqlx::query("SELECT 1").execute(state.store().pool()).await.is_ok();
    Json(json!({
        "status": if db_ok { "ok" } else { "degraded" },
        "db": db_ok,
        "rpc_endpoints": state.rpc().endpoints().len(),
    }))
}

pub async fn info(State(state): State<AppState>) -> Json<Value> {
    let cfg = state.config();
    let tip = state.trusted_tip().await;
    Json(json!({
        "name": "depinzcash-server",
        "version": env!("CARGO_PKG_VERSION"),
        "network": cfg.network.as_str(),
        "rpc_endpoints": state.rpc().endpoints().len(),
        "trusted_tip_height": tip,
        "spl_mint": cfg.spl_mint,
        "solana_cluster": cfg.solana_cluster,
        "scheduler_enabled": cfg.scheduler_enabled,
        // Operators care about this — what message do they need to sign?
        "registration_message_v1": "depinzcash:register:v1\\n<wallet>\\n<nonce>\\n<rfc3339-ts>\\n<kind>\\n<network>\\n<label>\\n",
        // Until NU7 + ZIP-227 ship Zcash custom assets, rewards are SPL-denominated on Solana.
        "rewards_note": "rewards paid in SPL token on Solana — pending NU7 / ZIP-227 for native Zcash custom assets"
    }))
}
