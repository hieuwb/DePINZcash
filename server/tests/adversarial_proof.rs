// Adversarial / negative-path tests for /api/proofs/submit.
// Each test proves the server rejects a class of bad proof input.

use axum::{
    body::Body,
    http::{Method, Request, StatusCode},
};
use chrono::{Duration as ChronoDuration, Utc};
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
use uuid::Uuid;

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
        max_height_drift: 8,
        max_clock_skew: Duration::from_secs(15 * 60),
        rate_limit_enabled: false,
        rate_limit_per_second: 1000,
        rate_limit_burst: 5000,
        spl_mint: None,
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

// Register a fresh node and return (wallet, signing key, node_id).
async fn fresh_node(state: AppState) -> (String, SigningKey, String) {
    let (wallet, sk) = fresh_kp();
    let nonce = format!("reg-{:x}", rand::random::<u128>());
    let ts = Utc::now();
    let msg = registration_message(&wallet, &nonce, &ts.to_rfc3339(), "zebra-full", "mainnet", "");
    let sig = bs58::encode(sk.sign(&msg).to_bytes()).into_string();
    let app = api::router(state.clone());
    let (s, body) = post_json(
        app,
        "/api/nodes/register",
        json!({
            "wallet": &wallet, "signature": sig, "nonce": nonce,
            "timestamp": ts.to_rfc3339(), "kind": "zebra-full",
        }),
    )
    .await;
    assert_eq!(s, StatusCode::OK, "fresh_node register failed: {body}");
    let node_id = body["node"]["id"].as_str().unwrap().to_string();
    (wallet, sk, node_id)
}

fn proof_body(
    wallet: &str,
    sk: &SigningKey,
    node_id: &str,
    nonce: &str,
    height: u64,
    block_hash: &str,
) -> Value {
    let ts = Utc::now();
    let msg = proof_message(wallet, node_id, height, block_hash, &ts.to_rfc3339(), nonce);
    let sig = bs58::encode(sk.sign(&msg).to_bytes()).into_string();
    json!({
        "wallet": wallet, "node_id": node_id, "signature": sig, "nonce": nonce,
        "claimed_height": height, "claimed_block_hash": block_hash,
        "proof_timestamp": ts.to_rfc3339(),
        "uptime_seconds": 3600u64, "peers": 8,
    })
}

// ---- shape & basic auth -------------------------------------------------

#[tokio::test]
async fn rejects_empty_block_hash() {
    let state = build_state().await;
    let (wallet, sk, node_id) = fresh_node(state.clone()).await;
    let body = proof_body(&wallet, &sk, &node_id, "ebh-1234567890abcdef", 100, "");
    let (s, _) = post_json(api::router(state), "/api/proofs/submit", body).await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn rejects_oversized_block_hash() {
    let state = build_state().await;
    let (wallet, sk, node_id) = fresh_node(state.clone()).await;
    let huge = "a".repeat(200);
    let body = proof_body(&wallet, &sk, &node_id, "obh-1234567890abcdef", 100, &huge);
    let (s, _) = post_json(api::router(state), "/api/proofs/submit", body).await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn rejects_unknown_node_id() {
    let state = build_state().await;
    let (wallet, sk, _) = fresh_node(state.clone()).await;
    let fake_node = Uuid::new_v4().to_string();
    let body = proof_body(&wallet, &sk, &fake_node, "fake-node-1234567890ab", 100, "abc");
    let (s, _) = post_json(api::router(state), "/api/proofs/submit", body).await;
    assert_eq!(s, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn rejects_wrong_wallet_for_node() {
    let state = build_state().await;
    let (_, _, node_id) = fresh_node(state.clone()).await;
    let (other_wallet, other_sk) = fresh_kp();
    let body = proof_body(&other_wallet, &other_sk, &node_id, "wrong-wallet-1234567890", 100, "abc");
    let (s, _) = post_json(api::router(state), "/api/proofs/submit", body).await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn rejects_signature_from_wrong_key() {
    let state = build_state().await;
    let (wallet, _sk, node_id) = fresh_node(state.clone()).await;
    let (_, other_sk) = fresh_kp();
    // Sign with the wrong key but claim ownership of the registered wallet.
    let ts = Utc::now();
    let nonce = "wrong-key-proof-1234567890";
    let msg = proof_message(&wallet, &node_id, 100, "abc", &ts.to_rfc3339(), nonce);
    let bad_sig = bs58::encode(other_sk.sign(&msg).to_bytes()).into_string();
    let (s, _) = post_json(
        api::router(state),
        "/api/proofs/submit",
        json!({
            "wallet": wallet, "node_id": node_id, "signature": bad_sig, "nonce": nonce,
            "claimed_height": 100, "claimed_block_hash": "abc",
            "proof_timestamp": ts.to_rfc3339(),
        }),
    )
    .await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
}

// ---- timestamp window ---------------------------------------------------

#[tokio::test]
async fn rejects_old_proof_timestamp() {
    let state = build_state().await;
    let (wallet, sk, node_id) = fresh_node(state.clone()).await;
    let nonce = "old-proof-ts-1234567890ab";
    let old_ts = Utc::now() - ChronoDuration::hours(2);
    let msg = proof_message(&wallet, &node_id, 100, "abc", &old_ts.to_rfc3339(), nonce);
    let sig = bs58::encode(sk.sign(&msg).to_bytes()).into_string();
    let (s, _) = post_json(
        api::router(state),
        "/api/proofs/submit",
        json!({
            "wallet": wallet, "node_id": node_id, "signature": sig, "nonce": nonce,
            "claimed_height": 100, "claimed_block_hash": "abc",
            "proof_timestamp": old_ts.to_rfc3339(),
        }),
    )
    .await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
}

// ---- replay --------------------------------------------------------------

#[tokio::test]
async fn rejects_replayed_nonce() {
    let state = build_state().await;
    let (wallet, sk, node_id) = fresh_node(state.clone()).await;
    let body = proof_body(&wallet, &sk, &node_id, "replay-1234567890abcd", 100, "abc");

    let (s1, _) = post_json(api::router(state.clone()), "/api/proofs/submit", body.clone()).await;
    assert_eq!(s1, StatusCode::OK);

    let (s2, _) = post_json(api::router(state), "/api/proofs/submit", body).await;
    assert_eq!(s2, StatusCode::CONFLICT);
}

#[tokio::test]
async fn rejects_same_height_and_hash_twice_with_different_nonces() {
    let state = build_state().await;
    let (wallet, sk, node_id) = fresh_node(state.clone()).await;
    let b1 = proof_body(&wallet, &sk, &node_id, "dup-h-1-1234567890abc", 100, "abc");
    let b2 = proof_body(&wallet, &sk, &node_id, "dup-h-2-1234567890abc", 100, "abc");
    let (s1, _) = post_json(api::router(state.clone()), "/api/proofs/submit", b1).await;
    assert_eq!(s1, StatusCode::OK);
    let (s2, _) = post_json(api::router(state), "/api/proofs/submit", b2).await;
    assert_eq!(s2, StatusCode::CONFLICT);
}

// ---- monotonic-height guard ---------------------------------------------

#[tokio::test]
async fn rejects_far_behind_resubmission() {
    let state = build_state().await;
    let (wallet, sk, node_id) = fresh_node(state.clone()).await;

    // First, accept a high-height proof.
    let high = proof_body(&wallet, &sk, &node_id, "mono-high-1234567890ab", 1_000_000, "h-high");
    let (s1, _) = post_json(api::router(state.clone()), "/api/proofs/submit", high).await;
    assert_eq!(s1, StatusCode::OK);

    // Then try to submit a much-lower-height proof — should be rejected as "behind".
    let low = proof_body(&wallet, &sk, &node_id, "mono-low-1234567890abcd", 1_000, "h-low");
    let (s2, _) = post_json(api::router(state), "/api/proofs/submit", low).await;
    assert_eq!(s2, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn accepts_strictly_increasing_height() {
    let state = build_state().await;
    let (wallet, sk, node_id) = fresh_node(state.clone()).await;

    for (i, h) in [100u64, 105, 110, 200].into_iter().enumerate() {
        let nonce = format!("inc-{i}-abcdef1234567890");
        let body = proof_body(&wallet, &sk, &node_id, &nonce, h, &format!("h-{h}"));
        let (s, body) = post_json(api::router(state.clone()), "/api/proofs/submit", body).await;
        assert_eq!(s, StatusCode::OK, "tick {i} h={h}: {body}");
    }
}

// ---- correctness: points accumulate ------------------------------------

#[tokio::test]
async fn points_accumulate_across_proofs() {
    let state = build_state().await;
    let (wallet, sk, node_id) = fresh_node(state.clone()).await;

    for (i, h) in [100u64, 101, 102].into_iter().enumerate() {
        let nonce = format!("acc-{i}-1234567890abcdef");
        let body = proof_body(&wallet, &sk, &node_id, &nonce, h, &format!("h-{h}"));
        let (s, _) = post_json(api::router(state.clone()), "/api/proofs/submit", body).await;
        assert_eq!(s, StatusCode::OK);
    }

    // Fetch wallet stats and assert points > 3 * minimum tier credit.
    let req = Request::builder()
        .method(Method::GET)
        .uri(format!("/api/wallet/{wallet}/stats"))
        .body(Body::empty())
        .unwrap();
    let resp = api::router(state).oneshot(req).await.unwrap();
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    let stats: Value = serde_json::from_slice(&bytes).unwrap();
    let pts = stats["total_points"].as_u64().unwrap();
    assert!(pts >= 30, "expected at least 3 ticks of credit, got {pts}");
}

#[tokio::test]
async fn last_proof_at_updates_on_accept() {
    let state = build_state().await;
    let (wallet, sk, node_id) = fresh_node(state.clone()).await;

    let body = proof_body(&wallet, &sk, &node_id, "last-proof-at-1234567890", 100, "h-100");
    let (s, _) = post_json(api::router(state.clone()), "/api/proofs/submit", body).await;
    assert_eq!(s, StatusCode::OK);

    let req = Request::builder()
        .method(Method::GET)
        .uri(format!("/api/wallet/{wallet}/nodes"))
        .body(Body::empty())
        .unwrap();
    let resp = api::router(state).oneshot(req).await.unwrap();
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    let nodes: Value = serde_json::from_slice(&bytes).unwrap();
    assert!(nodes[0]["last_proof_at"].as_str().is_some());
    assert_eq!(nodes[0]["status"], "active");
    assert_eq!(nodes[0]["last_height"], 100);
}

// ---- 404s ---------------------------------------------------------------

#[tokio::test]
async fn wallet_with_no_nodes_returns_empty_list() {
    let state = build_state().await;
    let (wallet, _sk) = fresh_kp();
    let req = Request::builder()
        .method(Method::GET)
        .uri(format!("/api/wallet/{wallet}/nodes"))
        .body(Body::empty())
        .unwrap();
    let resp = api::router(state).oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    let v: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(v.as_array().unwrap().len(), 0);
}

#[tokio::test]
async fn malformed_wallet_in_url_is_rejected() {
    let state = build_state().await;
    let req = Request::builder()
        .method(Method::GET)
        .uri("/api/wallet/notabase58wallet0OIl/nodes")
        .body(Body::empty())
        .unwrap();
    let resp = api::router(state).oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}
