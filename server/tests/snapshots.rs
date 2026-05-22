// Edge cases around the Merkle snapshot publishing + claim lookup.

use axum::{
    body::Body,
    http::{Method, Request, StatusCode},
};
use chrono::Utc;
use depinzcash_server::{
    api,
    auth::{proof_message, registration_message},
    config::{Config, ZcashNetwork},
    rpc::ZcashRpcQuorum,
    state::AppState,
    store::SqliteStore,
};
use ed25519_dalek::{Signer, SigningKey};
use http_body_util::BodyExt;
use rand::RngCore;
use serde_json::{json, Value};
use std::time::Duration;
use tower::ServiceExt;

fn test_config() -> Config {
    Config {
        bind_addr: "127.0.0.1:0".into(),
        database_url: "sqlite::memory:".into(),
        trusted_rpcs: vec![],
        rpc_timeout: Duration::from_secs(1),
        admin_api_key: Some("admin-key".into()),
        cors_allowed_origins: vec![],
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
        spl_mint: Some("So11111111111111111111111111111111111111112".into()),
        solana_cluster: "devnet".into(),
        network: ZcashNetwork::Mainnet,
    }
}

async fn build_state() -> AppState {
    let store = SqliteStore::connect("sqlite::memory:").await.unwrap();
    store.migrate().await.unwrap();
    AppState::new(
        test_config(),
        store,
        ZcashRpcQuorum::new(vec![], Duration::from_secs(1)),
    )
}

fn fresh_kp() -> (String, SigningKey) {
    let mut s = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut s);
    let sk = SigningKey::from_bytes(&s);
    (bs58::encode(sk.verifying_key().to_bytes()).into_string(), sk)
}

async fn post_json(app: axum::Router, path: &str, body: Value) -> (StatusCode, Value) {
    let req = Request::builder()
        .method(Method::POST)
        .uri(path)
        .header("content-type", "application/json")
        .body(Body::from(body.to_string()))
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    let s = resp.status();
    let b = resp.into_body().collect().await.unwrap().to_bytes();
    (s, serde_json::from_slice(&b).unwrap_or(Value::Null))
}

async fn get_json(app: axum::Router, path: &str) -> (StatusCode, Value) {
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

async fn register_and_submit(state: AppState, height: u64) -> String {
    let (wallet, sk) = fresh_kp();
    // register
    let r_nonce = format!("snap-reg-{:x}", rand::random::<u128>());
    let ts = Utc::now();
    let r_msg = registration_message(&wallet, &r_nonce, &ts.to_rfc3339(), "zebra-full", "mainnet", "");
    let r_sig = bs58::encode(sk.sign(&r_msg).to_bytes()).into_string();
    let (rs, rb) = post_json(
        api::router(state.clone()),
        "/api/nodes/register",
        json!({
            "wallet": &wallet, "signature": r_sig, "nonce": r_nonce,
            "timestamp": ts.to_rfc3339(), "kind": "zebra-full",
        }),
    )
    .await;
    assert_eq!(rs, StatusCode::OK, "register: {rb}");
    let node_id = rb["node"]["id"].as_str().unwrap().to_string();

    // submit accepted proof
    let p_nonce = format!("snap-proof-{:x}", rand::random::<u128>());
    let p_ts = Utc::now();
    let p_msg = proof_message(&wallet, &node_id, height, "h", &p_ts.to_rfc3339(), &p_nonce);
    let p_sig = bs58::encode(sk.sign(&p_msg).to_bytes()).into_string();
    let (ps, pb) = post_json(
        api::router(state),
        "/api/proofs/submit",
        json!({
            "wallet": &wallet, "node_id": node_id, "signature": p_sig, "nonce": p_nonce,
            "claimed_height": height, "claimed_block_hash": "h",
            "proof_timestamp": p_ts.to_rfc3339(),
            "uptime_seconds": 3600u64, "peers": 4,
        }),
    )
    .await;
    assert_eq!(ps, StatusCode::OK, "proof: {pb}");
    wallet
}

async fn publish_snapshot(state: AppState) -> (StatusCode, Value) {
    let req = Request::builder()
        .method(Method::POST)
        .uri("/api/admin/snapshot/publish")
        .header("x-admin-key", "admin-key")
        .body(Body::empty())
        .unwrap();
    let resp = api::router(state).oneshot(req).await.unwrap();
    let s = resp.status();
    let b = resp.into_body().collect().await.unwrap().to_bytes();
    (s, serde_json::from_slice(&b).unwrap_or(Value::Null))
}

// ---- empty / sad paths --------------------------------------------------

#[tokio::test]
async fn publishing_with_no_eligible_wallets_fails() {
    let state = build_state().await;
    // No one has registered / submitted anything.
    let (s, _body) = publish_snapshot(state).await;
    // merkle::publish_snapshot bails with anyhow → 500
    assert_eq!(s, StatusCode::INTERNAL_SERVER_ERROR);
}

#[tokio::test]
async fn latest_snapshot_404s_before_publish() {
    let state = build_state().await;
    let (s, _) = get_json(api::router(state), "/api/snapshots/latest").await;
    assert_eq!(s, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn claim_for_unknown_wallet_404s() {
    let state = build_state().await;
    let _w = register_and_submit(state.clone(), 100).await;
    let (s, body) = publish_snapshot(state.clone()).await;
    assert_eq!(s, StatusCode::OK, "publish: {body}");

    // Different, never-registered wallet.
    let (other, _) = fresh_kp();
    let (s, _) = get_json(
        api::router(state),
        &format!("/api/wallet/{other}/claim/latest"),
    )
    .await;
    assert_eq!(s, StatusCode::NOT_FOUND);
}

// ---- happy multi-cycle --------------------------------------------------

#[tokio::test]
async fn multiple_cycles_increment_and_persist() {
    let state = build_state().await;
    let _w = register_and_submit(state.clone(), 100).await;

    let (s, b1) = publish_snapshot(state.clone()).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(b1["cycle"], 1);

    // Submit another proof from another wallet so the second snapshot has
    // different leaves (otherwise it'd contain identical (wallet,points) and
    // still publish — but we want variety).
    let _w2 = register_and_submit(state.clone(), 101).await;

    let (s, b2) = publish_snapshot(state.clone()).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(b2["cycle"], 2);
    assert_ne!(b1["merkle_root"], b2["merkle_root"]);
    assert!(b2["leaves"].as_i64().unwrap() >= b1["leaves"].as_i64().unwrap());

    // /api/snapshots/latest reflects the newest cycle.
    let (s, latest) = get_json(api::router(state), "/api/snapshots/latest").await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(latest["cycle"], 2);
    assert_eq!(latest["merkle_root"], b2["merkle_root"]);
}

#[tokio::test]
async fn claim_payload_has_valid_shape() {
    let state = build_state().await;
    let wallet = register_and_submit(state.clone(), 100).await;
    let (s, _) = publish_snapshot(state.clone()).await;
    assert_eq!(s, StatusCode::OK);

    let (s, body) = get_json(
        api::router(state),
        &format!("/api/wallet/{wallet}/claim/latest"),
    )
    .await;
    assert_eq!(s, StatusCode::OK);

    assert_eq!(body["wallet"], wallet);
    assert!(body["cycle"].as_i64().unwrap() >= 1);
    assert_eq!(body["merkle_root"].as_str().unwrap().len(), 64);
    assert_eq!(body["leaf_hash"].as_str().unwrap().len(), 64);
    assert!(body["points"].as_u64().unwrap() > 0);
    assert!(body["proof"]["siblings"].is_array());
    assert!(body["proof"]["leaf_index"].as_u64().is_some());
}

#[tokio::test]
async fn snapshot_carries_spl_mint_through() {
    let state = build_state().await;
    let wallet = register_and_submit(state.clone(), 100).await;
    let (s, _) = publish_snapshot(state.clone()).await;
    assert_eq!(s, StatusCode::OK);

    let (s, body) = get_json(
        api::router(state),
        &format!("/api/wallet/{wallet}/claim/latest"),
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(body["spl_mint"], "So11111111111111111111111111111111111111112");
    assert_eq!(body["solana_cluster"], "devnet");
}

#[tokio::test]
async fn admin_publish_with_wrong_key_is_401() {
    let state = build_state().await;
    let _w = register_and_submit(state.clone(), 100).await;
    let req = Request::builder()
        .method(Method::POST)
        .uri("/api/admin/snapshot/publish")
        .header("x-admin-key", "wrong-key")
        .body(Body::empty())
        .unwrap();
    let resp = api::router(state).oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn admin_publish_without_key_header_is_401() {
    let state = build_state().await;
    let _w = register_and_submit(state.clone(), 100).await;
    let req = Request::builder()
        .method(Method::POST)
        .uri("/api/admin/snapshot/publish")
        .body(Body::empty())
        .unwrap();
    let resp = api::router(state).oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}
