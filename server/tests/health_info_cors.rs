// Tests for the small but important surface: /healthz, /readyz, /api/info,
// and the CORS layer that gates browser access.

use axum::{
    body::Body,
    http::{Method, Request, StatusCode},
};
use depinzcash_server::{
    api,
    config::{Config, ZcashNetwork},
    rpc::ZcashRpcQuorum,
    state::AppState,
    store::SqliteStore,
};
use http_body_util::BodyExt;
use serde_json::Value;
use std::time::Duration;
use tower::ServiceExt;

fn cfg(cors: Vec<String>, mint: Option<String>) -> Config {
    Config {
        bind_addr: "127.0.0.1:0".into(),
        database_url: "sqlite::memory:".into(),
        trusted_rpcs: vec![],
        rpc_timeout: Duration::from_secs(1),
        admin_api_key: Some("admin-key".into()),
        cors_allowed_origins: cors,
        scheduler_enabled: false,
        heartbeat_interval: Duration::from_secs(60),
        challenge_check_interval: Duration::from_secs(60),
        uptime_reward_interval: Duration::from_secs(60),
        snapshot_interval: None,
        max_height_drift: 8,
        max_clock_skew: Duration::from_secs(15 * 60),
        rate_limit_enabled: false,
        rate_limit_per_second: 1000,
        rate_limit_burst: 5000,
        spl_mint: mint,
        solana_cluster: "mainnet-beta".into(),
        network: ZcashNetwork::Mainnet,
    }
}

async fn state_with(cors: Vec<String>, mint: Option<String>) -> AppState {
    let store = SqliteStore::connect("sqlite::memory:").await.unwrap();
    store.migrate().await.unwrap();
    AppState::new(
        cfg(cors, mint),
        store,
        ZcashRpcQuorum::new(vec![], Duration::from_secs(1)),
    )
}

async fn get(app: axum::Router, path: &str) -> (StatusCode, Value) {
    let req = Request::builder()
        .method(Method::GET)
        .uri(path)
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    let s = resp.status();
    let b = resp.into_body().collect().await.unwrap().to_bytes();
    (s, serde_json::from_slice(&b).unwrap_or(Value::Null))
}

// ---- /healthz + /readyz ------------------------------------------------

#[tokio::test]
async fn healthz_returns_ok() {
    let app = api::router(state_with(vec![], None).await);
    let (s, body) = get(app, "/healthz").await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(body["status"], "ok");
}

#[tokio::test]
async fn readyz_reports_db_ok_and_zero_rpcs() {
    let app = api::router(state_with(vec![], None).await);
    let (s, body) = get(app, "/readyz").await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(body["status"], "ok");
    assert_eq!(body["db"], true);
    assert_eq!(body["rpc_endpoints"], 0);
}

// ---- /api/info ----------------------------------------------------------

#[tokio::test]
async fn api_info_carries_config_fields() {
    let mint = "So11111111111111111111111111111111111111112".to_string();
    let app = api::router(state_with(vec![], Some(mint.clone())).await);
    let (s, body) = get(app, "/api/info").await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(body["name"], "depinzcash-server");
    assert_eq!(body["network"], "mainnet");
    assert_eq!(body["solana_cluster"], "mainnet-beta");
    assert_eq!(body["spl_mint"], mint);
    assert_eq!(body["scheduler_enabled"], false);
    assert!(body["version"].as_str().is_some());
    assert!(body["rewards_note"].as_str().unwrap().contains("$ZePIN"));
    assert!(body["registration_message_v1"]
        .as_str()
        .unwrap()
        .starts_with("depinzcash:register:v1"));
}

#[tokio::test]
async fn api_info_handles_null_mint() {
    let app = api::router(state_with(vec![], None).await);
    let (s, body) = get(app, "/api/info").await;
    assert_eq!(s, StatusCode::OK);
    assert!(body["spl_mint"].is_null());
}

// ---- CORS preflight + actual request ----------------------------------

#[tokio::test]
async fn cors_allows_listed_origin() {
    let app = api::router(state_with(vec!["http://localhost:3002".into()], None).await);
    let req = Request::builder()
        .method(Method::OPTIONS)
        .uri("/api/info")
        .header("origin", "http://localhost:3002")
        .header("access-control-request-method", "GET")
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    let ok_origin = resp
        .headers()
        .get("access-control-allow-origin")
        .and_then(|v| v.to_str().ok())
        .map(|v| v.to_string())
        .unwrap_or_default();
    assert_eq!(ok_origin, "http://localhost:3002");
}

#[tokio::test]
async fn cors_blocks_unlisted_origin() {
    let app = api::router(state_with(vec!["http://localhost:3002".into()], None).await);
    let req = Request::builder()
        .method(Method::OPTIONS)
        .uri("/api/info")
        .header("origin", "http://evil.example.com")
        .header("access-control-request-method", "GET")
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    let ok_origin = resp
        .headers()
        .get("access-control-allow-origin")
        .and_then(|v| v.to_str().ok())
        .map(|v| v.to_string())
        .unwrap_or_default();
    assert_ne!(ok_origin, "http://evil.example.com");
}

#[tokio::test]
async fn cors_with_no_configured_origins_omits_header() {
    let app = api::router(state_with(vec![], None).await);
    let req = Request::builder()
        .method(Method::OPTIONS)
        .uri("/api/info")
        .header("origin", "http://localhost:3002")
        .header("access-control-request-method", "GET")
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert!(
        resp.headers().get("access-control-allow-origin").is_none(),
        "no allow-origin header expected when CORS_ALLOWED_ORIGINS is empty",
    );
}

// ---- 404 ----------------------------------------------------------------

#[tokio::test]
async fn unknown_route_404s() {
    let app = api::router(state_with(vec![], None).await);
    let req = Request::builder()
        .method(Method::GET)
        .uri("/this/does/not/exist")
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}
