// Concurrency / race tests. Fire many parallel requests at the same shared
// state and assert the invariants the server promises:
//
//   - exactly-once nonces (no two requests with the same nonce both succeed)
//   - duplicate (height, block_hash) is rejected even under race
//   - point totals are consistent (sum of awarded == wallet stats)

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
use std::{sync::Arc, time::Duration};
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
    // Single shared on-disk-style memory file so all router clones see the same data.
    // Using `:memory:` per-connect produces isolated databases in sqlite, so we have to
    // share via the connection pool. SqliteStore already pools internally — good.
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

async fn register_node(state: AppState) -> (String, SigningKey, String) {
    let (wallet, sk) = fresh_kp();
    let nonce = format!("conc-reg-{:x}", rand::random::<u128>());
    let ts = Utc::now();
    let msg = registration_message(&wallet, &nonce, &ts.to_rfc3339(), "zebra-full", "mainnet", "");
    let sig = bs58::encode(sk.sign(&msg).to_bytes()).into_string();
    let (s, body) = post_json(
        api::router(state),
        "/api/nodes/register",
        json!({
            "wallet": &wallet, "signature": sig, "nonce": nonce,
            "timestamp": ts.to_rfc3339(), "kind": "zebra-full",
        }),
    )
    .await;
    assert_eq!(s, StatusCode::OK, "register failed: {body}");
    (wallet, sk, body["node"]["id"].as_str().unwrap().to_string())
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
    })
}

// ---- nonce exactly-once -------------------------------------------------

#[tokio::test]
async fn parallel_same_nonce_only_one_wins() {
    let state = build_state().await;
    let (wallet, sk, node_id) = register_node(state.clone()).await;

    // 32 parallel submissions, all using the SAME nonce. Exactly one must be
    // OK; the rest must be CONFLICT (or BAD_REQUEST if the nonce check loses
    // the race — but the server-side INSERT OR IGNORE makes CONFLICT correct).
    let body = proof_body(&wallet, &sk, &node_id, "conc-nonce-1234567890abcd", 100, "h-100");

    let mut handles = Vec::new();
    for _ in 0..32 {
        let state = state.clone();
        let body = body.clone();
        handles.push(tokio::spawn(async move {
            post_json(api::router(state), "/api/proofs/submit", body).await
        }));
    }

    let mut ok = 0;
    let mut conflict = 0;
    for h in handles {
        let (s, _) = h.await.unwrap();
        if s == StatusCode::OK {
            ok += 1;
        } else if s == StatusCode::CONFLICT {
            conflict += 1;
        } else {
            panic!("unexpected status: {s}");
        }
    }
    assert_eq!(ok, 1, "exactly one submission must be accepted");
    assert_eq!(conflict, 31, "31 must conflict");
}

#[tokio::test]
async fn parallel_same_height_hash_only_one_wins() {
    // Distinct nonces but same (height, block_hash) on the same node → also exactly one wins.
    let state = build_state().await;
    let (wallet, sk, node_id) = register_node(state.clone()).await;

    let mut handles = Vec::new();
    for i in 0..16 {
        let nonce = format!("conc-dup-h-{i}-1234567890ab");
        let body = proof_body(&wallet, &sk, &node_id, &nonce, 200, "h-200");
        let state = state.clone();
        handles.push(tokio::spawn(async move {
            post_json(api::router(state), "/api/proofs/submit", body).await
        }));
    }

    let mut ok = 0;
    let mut conflict = 0;
    for h in handles {
        let (s, _) = h.await.unwrap();
        if s == StatusCode::OK {
            ok += 1;
        } else if s == StatusCode::CONFLICT {
            conflict += 1;
        }
    }
    assert_eq!(ok, 1);
    assert!(conflict >= 15, "expected at least 15 conflicts, got {conflict}");
}

// ---- accumulation correctness -------------------------------------------

#[tokio::test]
async fn parallel_distinct_proofs_all_accepted_and_points_sum() {
    let state = build_state().await;
    let (wallet, sk, node_id) = register_node(state.clone()).await;

    let n = 20u64;
    let mut handles = Vec::new();
    for i in 0..n {
        let nonce = format!("conc-dist-{i:04}-12345678");
        let body = proof_body(
            &wallet,
            &sk,
            &node_id,
            &nonce,
            1000 + i, // each proof has a unique height
            &format!("h-{i:04}"),
        );
        let state = state.clone();
        handles.push(tokio::spawn(async move {
            post_json(api::router(state), "/api/proofs/submit", body).await
        }));
    }

    let mut total_awarded: u64 = 0;
    for h in handles {
        let (s, body) = h.await.unwrap();
        assert_eq!(s, StatusCode::OK, "submission failed: {body}");
        total_awarded += body["points_awarded"].as_u64().unwrap_or(0);
    }
    assert!(total_awarded > 0);

    // Wallet stats must equal the sum of awarded points.
    let req = Request::builder()
        .method(Method::GET)
        .uri(format!("/api/wallet/{wallet}/stats"))
        .body(Body::empty())
        .unwrap();
    let resp = api::router(state).oneshot(req).await.unwrap();
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    let stats: Value = serde_json::from_slice(&bytes).unwrap();
    let stored = stats["total_points"].as_u64().unwrap();
    assert_eq!(stored, total_awarded, "stored {stored} != sum of awarded {total_awarded}");
}

// ---- registration race ---------------------------------------------------

#[tokio::test]
async fn parallel_duplicate_registration_one_wins() {
    let state = build_state().await;
    let (wallet, sk) = fresh_kp();
    let nonce = "conc-reg-dup-1234567890abcd";
    let ts = Utc::now();
    let msg = registration_message(&wallet, nonce, &ts.to_rfc3339(), "zebra-full", "mainnet", "");
    let sig = bs58::encode(sk.sign(&msg).to_bytes()).into_string();
    let body = json!({
        "wallet": &wallet, "signature": sig, "nonce": nonce,
        "timestamp": ts.to_rfc3339(), "kind": "zebra-full",
    });

    let mut handles = Vec::new();
    for _ in 0..16 {
        let state = state.clone();
        let body = body.clone();
        handles.push(tokio::spawn(async move {
            post_json(api::router(state), "/api/nodes/register", body).await
        }));
    }

    let mut ok = 0;
    let mut conflict = 0;
    for h in handles {
        let (s, _) = h.await.unwrap();
        if s == StatusCode::OK {
            ok += 1;
        } else if s == StatusCode::CONFLICT {
            conflict += 1;
        }
    }
    assert_eq!(ok, 1, "exactly one registration must succeed");
    assert_eq!(conflict, 15);
}

// ---- store-level race ----------------------------------------------------

#[tokio::test]
async fn store_try_use_nonce_is_atomic() {
    // Directly hit the store under heavy contention — only one of N concurrent
    // try_use_nonce calls may return true.
    let store = SqliteStore::connect("sqlite::memory:").await.unwrap();
    store.migrate().await.unwrap();
    let store = Arc::new(store);

    let mut handles = Vec::new();
    for _ in 0..50 {
        let store = store.clone();
        handles.push(tokio::spawn(async move {
            store.try_use_nonce("shared-race-nonce", "walletX").await.unwrap()
        }));
    }
    let mut wins = 0;
    for h in handles {
        if h.await.unwrap() {
            wins += 1;
        }
    }
    assert_eq!(wins, 1, "exactly one task must win the nonce race");
}
