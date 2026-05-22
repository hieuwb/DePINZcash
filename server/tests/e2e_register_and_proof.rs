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
        admin_api_key: Some("test-admin-key".into()),
        cors_allowed_origins: vec![],
        scheduler_enabled: false,
        heartbeat_interval: Duration::from_secs(60),
        challenge_check_interval: Duration::from_secs(60),
        uptime_reward_interval: Duration::from_secs(60),
        snapshot_interval: None,
        exposed_rpc_poll_interval: None,
        max_height_drift: 8,
        max_clock_skew: Duration::from_secs(15 * 60),
        rate_limit_enabled: false,
        rate_limit_per_second: 1000,
        rate_limit_burst: 5000,
        registration_enabled: true,
        spl_mint: Some("So11111111111111111111111111111111111111112".into()),
        solana_cluster: "devnet".into(),
        network: ZcashNetwork::Mainnet,
    }
}

async fn build_state() -> AppState {
    let store = SqliteStore::connect("sqlite::memory:").await.unwrap();
    store.migrate().await.unwrap();
    let rpc = ZcashRpcQuorum::new(vec![], Duration::from_secs(1));
    AppState::new(test_config(), store, rpc)
}

fn fresh_keypair() -> (String, SigningKey) {
    let mut secret = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut secret);
    let sk = SigningKey::from_bytes(&secret);
    let wallet = bs58::encode(sk.verifying_key().to_bytes()).into_string();
    (wallet, sk)
}

fn b58_sig(sk: &SigningKey, msg: &[u8]) -> String {
    bs58::encode(sk.sign(msg).to_bytes()).into_string()
}

async fn json_post(app: axum::Router, path: &str, body: Value) -> (StatusCode, Value) {
    let req = Request::builder()
        .method(Method::POST)
        .uri(path)
        .header("content-type", "application/json")
        .body(Body::from(body.to_string()))
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    let status = resp.status();
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    let body: Value = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
    (status, body)
}

async fn json_get(app: axum::Router, path: &str) -> (StatusCode, Value) {
    let req = Request::builder()
        .method(Method::GET)
        .uri(path)
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    let status = resp.status();
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    let body: Value = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
    (status, body)
}

#[tokio::test]
async fn healthz_returns_ok() {
    let state = build_state().await;
    let app = api::router(state);
    let (status, body) = json_get(app, "/healthz").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["status"], "ok");
}

#[tokio::test]
async fn register_then_submit_proof_full_flow() {
    let state = build_state().await;
    let app = || api::router(state.clone());

    let (wallet, sk) = fresh_keypair();
    let nonce = "test-nonce-abcdef1234567890";
    let ts = Utc::now();
    let kind = "zebra-full";
    let label = "primary";

    let reg_msg = registration_message(
        &wallet,
        nonce,
        &ts.to_rfc3339(),
        kind,
        "mainnet",
        label,
    );
    let reg_sig = b58_sig(&sk, &reg_msg);

    let (status, body) = json_post(
        app(),
        "/api/nodes/register",
        json!({
            "wallet": wallet,
            "signature": reg_sig,
            "nonce": nonce,
            "timestamp": ts.to_rfc3339(),
            "kind": kind,
            "label": label,
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "register failed: {body}");
    let node_id = body["node"]["id"].as_str().unwrap().to_string();
    assert_eq!(body["node"]["wallet"], wallet);

    // Submit a proof — permissive mode (no trusted rpcs) → accepted.
    let proof_nonce = "proof-nonce-abcdef1234567890";
    let proof_ts = Utc::now();
    let height: u64 = 2_500_000;
    let block_hash = "0000000000abcdef1234567890abcdef1234567890abcdef1234567890abcd";

    let proof_msg = proof_message(
        &wallet,
        &node_id,
        height,
        block_hash,
        &proof_ts.to_rfc3339(),
        proof_nonce,
    );
    let proof_sig = b58_sig(&sk, &proof_msg);

    let (status, body) = json_post(
        app(),
        "/api/proofs/submit",
        json!({
            "wallet": wallet,
            "node_id": node_id,
            "signature": proof_sig,
            "nonce": proof_nonce,
            "claimed_height": height,
            "claimed_block_hash": block_hash,
            "proof_timestamp": proof_ts.to_rfc3339(),
            "uptime_seconds": 7200,
            "peers": 12,
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "proof submit failed: {body}");
    assert_eq!(body["verdict"], "accepted", "body={body}");
    assert!(body["points_awarded"].as_u64().unwrap() > 0);

    // Wallet stats should now show points.
    let (status, body) = json_get(app(), &format!("/api/wallet/{wallet}/stats")).await;
    assert_eq!(status, StatusCode::OK);
    assert!(body["total_points"].as_u64().unwrap() > 0, "stats={body}");

    // Leaderboard should include this wallet.
    let (status, body) = json_get(app(), "/api/stats/leaderboard").await;
    assert_eq!(status, StatusCode::OK);
    let arr = body.as_array().unwrap();
    assert!(arr.iter().any(|w| w["wallet"] == wallet), "leaderboard missing wallet: {body}");
}

#[tokio::test]
async fn register_rejects_bad_signature() {
    let state = build_state().await;
    let app = api::router(state);

    let (wallet, _sk) = fresh_keypair();
    let nonce = "bad-sig-nonce-1234567890abcdef";
    let ts = Utc::now();
    let bogus_sig = bs58::encode([0u8; 64]).into_string();
    let (status, _body) = json_post(
        app,
        "/api/nodes/register",
        json!({
            "wallet": wallet,
            "signature": bogus_sig,
            "nonce": nonce,
            "timestamp": ts.to_rfc3339(),
            "kind": "zebra-full",
        }),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn register_rejects_replayed_nonce() {
    let state = build_state().await;
    let app = || api::router(state.clone());

    let (wallet, sk) = fresh_keypair();
    let nonce = "replay-nonce-1234567890abcdef";
    let ts = Utc::now();
    let msg = registration_message(&wallet, nonce, &ts.to_rfc3339(), "zebra-full", "mainnet", "");
    let sig = b58_sig(&sk, &msg);

    let body = json!({
        "wallet": wallet,
        "signature": sig,
        "nonce": nonce,
        "timestamp": ts.to_rfc3339(),
        "kind": "zebra-full",
    });

    let (status1, _) = json_post(app(), "/api/nodes/register", body.clone()).await;
    assert_eq!(status1, StatusCode::OK);

    let (status2, _) = json_post(app(), "/api/nodes/register", body).await;
    assert_eq!(status2, StatusCode::CONFLICT);
}

#[tokio::test]
async fn snapshot_publish_round_trip() {
    let state = build_state().await;
    let app = || api::router(state.clone());

    // Register + submit one accepted proof so the wallet has points.
    let (wallet, sk) = fresh_keypair();
    let nonce_r = "snapshot-reg-1234567890abcdef";
    let ts = Utc::now();
    let reg_msg = registration_message(&wallet, nonce_r, &ts.to_rfc3339(), "zebra-full", "mainnet", "");
    let reg_sig = b58_sig(&sk, &reg_msg);
    let (s, body) = json_post(
        app(),
        "/api/nodes/register",
        json!({
            "wallet": wallet, "signature": reg_sig, "nonce": nonce_r,
            "timestamp": ts.to_rfc3339(), "kind": "zebra-full",
        }),
    ).await;
    assert_eq!(s, StatusCode::OK);
    let node_id = body["node"]["id"].as_str().unwrap().to_string();

    let proof_nonce = "snapshot-proof-1234567890abcdef";
    let proof_ts = Utc::now();
    let height: u64 = 2_500_000;
    let block_hash = "00000000aabbccddeeff00112233445566778899aabbccddeeff001122334455";
    let pmsg = proof_message(&wallet, &node_id, height, block_hash, &proof_ts.to_rfc3339(), proof_nonce);
    let psig = b58_sig(&sk, &pmsg);
    let (s, _) = json_post(
        app(),
        "/api/proofs/submit",
        json!({
            "wallet": wallet, "node_id": node_id, "signature": psig, "nonce": proof_nonce,
            "claimed_height": height, "claimed_block_hash": block_hash,
            "proof_timestamp": proof_ts.to_rfc3339(),
            "uptime_seconds": 3600u64, "peers": 8,
        }),
    ).await;
    assert_eq!(s, StatusCode::OK);

    // Publish snapshot via admin endpoint.
    let req = Request::builder()
        .method(Method::POST)
        .uri("/api/admin/snapshot/publish")
        .header("content-type", "application/json")
        .header("x-admin-key", "test-admin-key")
        .body(Body::empty())
        .unwrap();
    let resp = app().oneshot(req).await.unwrap();
    let status = resp.status();
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    let body: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(status, StatusCode::OK, "snapshot publish failed: {body}");
    assert_eq!(body["cycle"], 1);
    assert_eq!(body["leaves"], 1);
    let root = body["merkle_root"].as_str().unwrap().to_string();
    assert_eq!(root.len(), 64, "expected hex root, got {root}");

    let (status, body) = json_get(app(), &format!("/api/wallet/{wallet}/claim/latest")).await;
    assert_eq!(status, StatusCode::OK, "claim fetch failed: {body}");
    assert_eq!(body["merkle_root"], root);
    assert!(body["points"].as_u64().unwrap() > 0);
}

#[tokio::test]
async fn admin_endpoint_rejects_without_key() {
    let state = build_state().await;
    let app = api::router(state);
    let req = Request::builder()
        .method(Method::POST)
        .uri("/api/admin/snapshot/publish")
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}
