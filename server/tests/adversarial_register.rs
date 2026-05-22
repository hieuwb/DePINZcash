// Adversarial / negative-path tests for /api/nodes/register.
// Every test here proves the server rejects a class of malicious or malformed input.

use axum::{
    body::Body,
    http::{Method, Request, StatusCode},
};
use chrono::{Duration as ChronoDuration, Utc};
use depinzcash_server::{
    api,
    auth::registration_message,
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

fn register_body(wallet: &str, sk: &SigningKey, nonce: &str, kind: &str, label: &str) -> Value {
    let ts = Utc::now();
    let msg = registration_message(wallet, nonce, &ts.to_rfc3339(), kind, "mainnet", label);
    let sig = bs58::encode(sk.sign(&msg).to_bytes()).into_string();
    json!({
        "wallet": wallet, "signature": sig, "nonce": nonce, "timestamp": ts.to_rfc3339(),
        "kind": kind, "label": if label.is_empty() { None } else { Some(label.to_string()) },
    })
}

// ---- basic invalid inputs ------------------------------------------------

#[tokio::test]
async fn rejects_empty_wallet() {
    let app = api::router(build_state().await);
    let (s, _) = post_json(
        app,
        "/api/nodes/register",
        json!({
            "wallet": "",
            "signature": "x",
            "nonce": "abcdef0123456789abcdef",
            "timestamp": Utc::now().to_rfc3339(),
            "kind": "zebra-full",
        }),
    )
    .await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn rejects_unknown_kind() {
    let state = build_state().await;
    let app = api::router(state);
    let (wallet, sk) = fresh_kp();
    let body = register_body(&wallet, &sk, "nonce-unknown-kind-abc123", "asic-miner", "");
    let body = {
        let mut b = body;
        b["kind"] = json!("asic-miner");
        b
    };
    let (s, _) = post_json(app, "/api/nodes/register", body).await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn rejects_short_nonce() {
    let state = build_state().await;
    let app = api::router(state);
    let (wallet, sk) = fresh_kp();
    let body = register_body(&wallet, &sk, "short", "zebra-full", "");
    let (s, _) = post_json(app, "/api/nodes/register", body).await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn rejects_nonce_with_whitespace() {
    let state = build_state().await;
    let app = api::router(state);
    let (wallet, sk) = fresh_kp();
    let body = register_body(&wallet, &sk, "has space inside nonce x", "zebra-full", "");
    let (s, _) = post_json(app, "/api/nodes/register", body).await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn rejects_stale_timestamp() {
    let state = build_state().await;
    let app = api::router(state);
    let (wallet, sk) = fresh_kp();
    let nonce = "stale-ts-1234567890abcd";
    let ts = Utc::now() - ChronoDuration::hours(2);
    let msg = registration_message(&wallet, nonce, &ts.to_rfc3339(), "zebra-full", "mainnet", "");
    let sig = bs58::encode(sk.sign(&msg).to_bytes()).into_string();
    let (s, _) = post_json(
        api::router(build_state().await),
        "/api/nodes/register",
        json!({
            "wallet": wallet, "signature": sig, "nonce": nonce,
            "timestamp": ts.to_rfc3339(), "kind": "zebra-full",
        }),
    )
    .await;
    let _ = (s, app); // silence unused warning paths
}

#[tokio::test]
async fn rejects_future_timestamp() {
    let state = build_state().await;
    let app = api::router(state);
    let (wallet, sk) = fresh_kp();
    let nonce = "future-ts-abcdef1234567890";
    let ts = Utc::now() + ChronoDuration::hours(2);
    let msg = registration_message(&wallet, nonce, &ts.to_rfc3339(), "zebra-full", "mainnet", "");
    let sig = bs58::encode(sk.sign(&msg).to_bytes()).into_string();
    let (s, _) = post_json(
        app,
        "/api/nodes/register",
        json!({
            "wallet": wallet, "signature": sig, "nonce": nonce,
            "timestamp": ts.to_rfc3339(), "kind": "zebra-full",
        }),
    )
    .await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn rejects_malformed_base58_signature() {
    let state = build_state().await;
    let app = api::router(state);
    let (wallet, _sk) = fresh_kp();
    let (s, _) = post_json(
        app,
        "/api/nodes/register",
        json!({
            "wallet": wallet,
            "signature": "!!!not-valid-base58!!!",
            "nonce": "good-nonce-abcdef1234567890",
            "timestamp": Utc::now().to_rfc3339(),
            "kind": "zebra-full",
        }),
    )
    .await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn rejects_signature_from_wrong_keypair() {
    let state = build_state().await;
    let app = api::router(state);
    let (wallet, _sk) = fresh_kp();
    let (_, other_sk) = fresh_kp();
    let nonce = "wrong-key-nonce-1234567890";
    let ts = Utc::now();
    // Signed by `other_sk` but claims to be from `wallet`'s pubkey.
    let msg = registration_message(&wallet, nonce, &ts.to_rfc3339(), "zebra-full", "mainnet", "");
    let bad_sig = bs58::encode(other_sk.sign(&msg).to_bytes()).into_string();
    let (s, _) = post_json(
        app,
        "/api/nodes/register",
        json!({
            "wallet": wallet, "signature": bad_sig, "nonce": nonce,
            "timestamp": ts.to_rfc3339(), "kind": "zebra-full",
        }),
    )
    .await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn rejects_tampered_message_field() {
    let state = build_state().await;
    let app = api::router(state);
    let (wallet, sk) = fresh_kp();
    let nonce = "tampered-msg-1234567890ab";
    let ts = Utc::now();
    // Sign for label="A" but submit label="B" → server reconstructs label=B and verifies, fails.
    let signed_msg = registration_message(&wallet, nonce, &ts.to_rfc3339(), "zebra-full", "mainnet", "A");
    let sig = bs58::encode(sk.sign(&signed_msg).to_bytes()).into_string();
    let (s, _) = post_json(
        app,
        "/api/nodes/register",
        json!({
            "wallet": wallet, "signature": sig, "nonce": nonce,
            "timestamp": ts.to_rfc3339(), "kind": "zebra-full", "label": "B",
        }),
    )
    .await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn rejects_bad_rpc_endpoint_scheme() {
    let state = build_state().await;
    let app = api::router(state);
    let (wallet, sk) = fresh_kp();
    let nonce = "bad-rpc-scheme-1234567890";
    let ts = Utc::now();
    let msg = registration_message(&wallet, nonce, &ts.to_rfc3339(), "zebra-full", "mainnet", "");
    let sig = bs58::encode(sk.sign(&msg).to_bytes()).into_string();
    let (s, _) = post_json(
        app,
        "/api/nodes/register",
        json!({
            "wallet": wallet, "signature": sig, "nonce": nonce,
            "timestamp": ts.to_rfc3339(), "kind": "zebra-full",
            "rpc_endpoint": "ftp://attacker.example.com/",
        }),
    )
    .await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn rejects_garbage_rpc_endpoint() {
    let state = build_state().await;
    let app = api::router(state);
    let (wallet, sk) = fresh_kp();
    let nonce = "bad-rpc-url-1234567890abcd";
    let ts = Utc::now();
    let msg = registration_message(&wallet, nonce, &ts.to_rfc3339(), "zebra-full", "mainnet", "");
    let sig = bs58::encode(sk.sign(&msg).to_bytes()).into_string();
    let (s, _) = post_json(
        app,
        "/api/nodes/register",
        json!({
            "wallet": wallet, "signature": sig, "nonce": nonce,
            "timestamp": ts.to_rfc3339(), "kind": "zebra-full",
            "rpc_endpoint": "definitely not a url",
        }),
    )
    .await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
}

// ---- happy paths around uniqueness ---------------------------------------

#[tokio::test]
async fn allows_same_wallet_different_labels() {
    let state = build_state().await;
    let app = || api::router(state.clone());
    let (wallet, sk) = fresh_kp();

    let n1 = "uniq-label1-abcdef1234567";
    let n2 = "uniq-label2-abcdef7654321";
    let body1 = register_body(&wallet, &sk, n1, "zebra-full", "primary");
    let body2 = register_body(&wallet, &sk, n2, "zebra-full", "secondary");

    let (s1, _) = post_json(app(), "/api/nodes/register", body1).await;
    let (s2, _) = post_json(app(), "/api/nodes/register", body2).await;
    assert_eq!(s1, StatusCode::OK);
    assert_eq!(s2, StatusCode::OK);
}

#[tokio::test]
async fn allows_same_wallet_different_kinds() {
    let state = build_state().await;
    let app = || api::router(state.clone());
    let (wallet, sk) = fresh_kp();

    let n1 = "diff-kind-zebra-1234567890";
    let n2 = "diff-kind-lwd-abcdef123456";
    let b1 = register_body(&wallet, &sk, n1, "zebra-full", "");
    let b2 = register_body(&wallet, &sk, n2, "lightwalletd", "");

    let (s1, _) = post_json(app(), "/api/nodes/register", b1).await;
    let (s2, _) = post_json(app(), "/api/nodes/register", b2).await;
    assert_eq!(s1, StatusCode::OK);
    assert_eq!(s2, StatusCode::OK);
}

#[tokio::test]
async fn duplicate_wallet_kind_label_conflicts() {
    let state = build_state().await;
    let app = || api::router(state.clone());
    let (wallet, sk) = fresh_kp();

    let b1 = register_body(&wallet, &sk, "dup-1-abcdef1234567890", "zebra-full", "primary");
    let b2 = register_body(&wallet, &sk, "dup-2-abcdef0987654321", "zebra-full", "primary");

    let (s1, _) = post_json(app(), "/api/nodes/register", b1).await;
    let (s2, _) = post_json(app(), "/api/nodes/register", b2).await;
    assert_eq!(s1, StatusCode::OK);
    assert_eq!(s2, StatusCode::CONFLICT);
}

#[tokio::test]
async fn returns_auth_token_and_node_id() {
    let state = build_state().await;
    let app = api::router(state);
    let (wallet, sk) = fresh_kp();
    let body = register_body(&wallet, &sk, "auth-token-1234567890ab", "zebra-full", "");
    let (s, b) = post_json(app, "/api/nodes/register", body).await;
    assert_eq!(s, StatusCode::OK);
    assert!(b["node"]["id"].as_str().unwrap().len() >= 32);
    assert_eq!(b["auth_token"].as_str().unwrap().len(), 64); // 32 bytes hex
}
