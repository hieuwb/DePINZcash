pub mod admin;
pub mod challenges;
pub mod health;
pub mod nodes;
pub mod proofs;
pub mod rewards;
pub mod stats;

use axum::{
    http::{HeaderName, HeaderValue, Method},
    routing::{get, post},
    Router,
};
use tower_http::{
    cors::{AllowOrigin, CorsLayer},
    trace::TraceLayer,
};

use crate::state::AppState;

pub fn router(state: AppState) -> Router {
    let cors = build_cors(&state);

    Router::new()
        .route("/healthz", get(health::healthz))
        .route("/readyz", get(health::readyz))
        .route("/api/info", get(health::info))
        .route("/api/nodes/register", post(nodes::register))
        .route("/api/nodes/:id", get(nodes::get_by_id))
        .route("/api/wallet/:wallet/nodes", get(nodes::list_for_wallet))
        .route("/api/wallet/:wallet/stats", get(stats::wallet_stats))
        .route("/api/wallet/:wallet/proofs", get(proofs::list_for_wallet))
        .route("/api/wallet/:wallet/claim/latest", get(rewards::latest_claim))
        .route("/api/proofs/submit", post(proofs::submit))
        .route("/api/challenges/request", post(challenges::request))
        .route("/api/challenges/submit", post(challenges::submit))
        .route("/api/stats/network", get(stats::network))
        .route("/api/stats/leaderboard", get(stats::leaderboard))
        .route("/api/snapshots/latest", get(rewards::latest_snapshot))
        .route("/api/admin/snapshot/publish", post(admin::publish_snapshot))
        .layer(TraceLayer::new_for_http())
        .layer(cors)
        .with_state(state)
}

fn build_cors(state: &AppState) -> CorsLayer {
    let origins = &state.config().cors_allowed_origins;
    let base = CorsLayer::new()
        .allow_methods([Method::GET, Method::POST, Method::OPTIONS])
        .allow_headers([
            HeaderName::from_static("content-type"),
            HeaderName::from_static("authorization"),
            HeaderName::from_static("x-admin-key"),
        ]);
    if origins.is_empty() {
        base
    } else {
        let list: Vec<HeaderValue> = origins
            .iter()
            .filter_map(|o| HeaderValue::from_str(o).ok())
            .collect();
        base.allow_origin(AllowOrigin::list(list))
    }
}
