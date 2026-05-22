// HTTP-level tests for the challenge endpoints. Spins up mock JSON-RPC servers
// to back the trusted quorum, then drives the full request → submit flow.

use axum::{
    body::Body,
    http::{Method, Request, StatusCode},
    routing::post,
    Json, Router,
};
use chrono::Utc;
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
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{net::SocketAddr, sync::Arc, time::Duration};
use tokio::{net::TcpListener, sync::Mutex, task::JoinHandle};
use tower::ServiceExt;

// ---- mock RPC scaffolding ------------------------------------------------

#[derive(Deserialize)]
struct JsonRpcReq {
    method: String,
    #[serde(default)]
    params: Value,
}

#[derive(Serialize)]
struct JsonRpcResp {
    jsonrpc: &'static str,
    id: u32,
    result: Value,
}

#[derive(Clone)]
struct MockState {
    tip: u64,
    block_hashes: std::collections::HashMap<u64, String>,
}

impl MockState {
    fn new(tip: u64) -> Self {
        let mut block_hashes = std::collections::HashMap::new();
        for h in (tip.saturating_sub(300))..=tip {
            block_hashes.insert(h, format!("hash-{h:08x}"));
        }
        Self { tip, block_hashes }
    }
}

struct MockServer {
    addr: SocketAddr,
    handle: JoinHandle<()>,
}

impl MockServer {
    async fn start(state: MockState) -> Self {
        let state = Arc::new(Mutex::new(state));
        let app = Router::new().route(
            "/",
            post(move |Json(req): Json<JsonRpcReq>| {
                let state = state.clone();
                async move {
                    let s = state.lock().await.clone();
                    let result = match req.method.as_str() {
                        "getblockcount" => json!(s.tip),
                        "getbestblockhash" => {
                            json!(s.block_hashes.get(&s.tip).cloned().unwrap_or_default())
                        }
                        "getblockhash" => {
                            let h = req.params.get(0).and_then(|v| v.as_u64()).unwrap_or(0);
                            json!(s.block_hashes.get(&h).cloned().unwrap_or_default())
                        }
                        _ => json!(null),
                    };
                    Ok::<_, axum::http::StatusCode>(Json(JsonRpcResp {
                        jsonrpc: "2.0",
                        id: 1,
                        result,
                    }))
                }
            }),
        );
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let handle = tokio::spawn(async move {
            axum::serve(listener, app.into_make_service()).await.unwrap();
        });
        Self { addr, handle }
    }
    fn url(&self) -> String {
        format!("http://{}/", self.addr)
    }
}
impl Drop for MockServer {
    fn drop(&mut self) {
        self.handle.abort();
    }
}

// ---- app state helpers ---------------------------------------------------

fn test_config(rpcs: Vec<String>) -> Config {
    Config {
        bind_addr: "127.0.0.1:0".into(),
        database_url: "sqlite::memory:".into(),
        trusted_rpcs: rpcs,
        rpc_timeout: Duration::from_secs(2),
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

async fn build_state(rpc_urls: Vec<String>) -> AppState {
    let store = SqliteStore::connect("sqlite::memory:").await.unwrap();
    store.migrate().await.unwrap();
    let rpc = ZcashRpcQuorum::new(rpc_urls.clone(), Duration::from_secs(2));
    AppState::new(test_config(rpc_urls), store, rpc)
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
    let status = resp.status();
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    (status, serde_json::from_slice(&bytes).unwrap_or(Value::Null))
}

async fn register_node(state: AppState) -> (String, SigningKey, String) {
    let (wallet, sk) = fresh_kp();
    let nonce = format!("ch-reg-{:x}", rand::random::<u128>());
    let ts = Utc::now();
    let msg = registration_message(&wallet, &nonce, &ts.to_rfc3339(), "zebra-full", "mainnet", "");
    let sig = bs58::encode(sk.sign(&msg).to_bytes()).into_string();
    let app = api::router(state);
    let (s, body) = post_json(
        app,
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

fn challenge_request_body(wallet: &str, sk: &SigningKey, node_id: &str, nonce: &str) -> Value {
    let ts = Utc::now();
    let msg = format!(
        "depinzcash:challenge:request:v1\n{wallet}\n{node_id}\n{nonce}\n{}\n",
        ts.to_rfc3339()
    );
    let sig = bs58::encode(sk.sign(msg.as_bytes()).to_bytes()).into_string();
    json!({
        "node_id": node_id, "wallet": wallet, "signature": sig,
        "nonce": nonce, "timestamp": ts.to_rfc3339(),
    })
}

fn challenge_submit_body(
    wallet: &str,
    sk: &SigningKey,
    challenge_id: &str,
    answer: &str,
    nonce: &str,
) -> Value {
    let ts = Utc::now();
    let msg = format!(
        "depinzcash:challenge:answer:v1\n{wallet}\n{challenge_id}\n{answer}\n{nonce}\n{}\n",
        ts.to_rfc3339()
    );
    let sig = bs58::encode(sk.sign(msg.as_bytes()).to_bytes()).into_string();
    json!({
        "challenge_id": challenge_id, "wallet": wallet, "signature": sig,
        "answer_block_hash": answer, "nonce": nonce, "timestamp": ts.to_rfc3339(),
    })
}

// ---- happy path ----------------------------------------------------------

#[tokio::test]
async fn challenge_request_then_correct_answer_passes() {
    let mock = MockServer::start(MockState::new(2_500_000)).await;
    let state = build_state(vec![mock.url()]).await;
    let (wallet, sk, node_id) = register_node(state.clone()).await;

    let req_body = challenge_request_body(&wallet, &sk, &node_id, "ch-req-1234567890abcdef");
    let (s, body) = post_json(api::router(state.clone()), "/api/challenges/request", req_body).await;
    assert_eq!(s, StatusCode::OK, "challenge request failed: {body}");
    let challenge_id = body["challenge_id"].as_str().unwrap().to_string();
    let target = body["target_height"].as_u64().unwrap();

    // The mock returns deterministic hashes; recompute the expected answer.
    let expected = format!("hash-{target:08x}");
    let ans = challenge_submit_body(&wallet, &sk, &challenge_id, &expected, "ch-ans-1234567890abcd");
    let (s, body) = post_json(api::router(state), "/api/challenges/submit", ans).await;
    assert_eq!(s, StatusCode::OK, "submit failed: {body}");
    assert_eq!(body["passed"], true);
}

#[tokio::test]
async fn challenge_request_then_wrong_answer_fails() {
    let mock = MockServer::start(MockState::new(2_500_000)).await;
    let state = build_state(vec![mock.url()]).await;
    let (wallet, sk, node_id) = register_node(state.clone()).await;

    let req_body = challenge_request_body(&wallet, &sk, &node_id, "ch-w-req-1234567890abcd");
    let (s, body) = post_json(api::router(state.clone()), "/api/challenges/request", req_body).await;
    assert_eq!(s, StatusCode::OK);
    let challenge_id = body["challenge_id"].as_str().unwrap().to_string();

    let bogus = "ffffffffffffffffffffffffffffffff";
    let ans = challenge_submit_body(&wallet, &sk, &challenge_id, bogus, "ch-w-ans-1234567890abcd");
    let (s, body) = post_json(api::router(state), "/api/challenges/submit", ans).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(body["passed"], false);
}

// ---- failure paths -------------------------------------------------------

#[tokio::test]
async fn challenge_request_requires_trusted_rpcs() {
    // No mock servers; quorum is empty → 502.
    let state = build_state(vec![]).await;
    let (wallet, sk, node_id) = register_node(state.clone()).await;
    let req_body = challenge_request_body(&wallet, &sk, &node_id, "ch-empty-1234567890abcd");
    let (s, _) = post_json(api::router(state), "/api/challenges/request", req_body).await;
    assert_eq!(s, StatusCode::BAD_GATEWAY);
}

#[tokio::test]
async fn challenge_request_wrong_wallet_for_node() {
    let mock = MockServer::start(MockState::new(2_500_000)).await;
    let state = build_state(vec![mock.url()]).await;
    let (_, _, node_id) = register_node(state.clone()).await;
    let (other_wallet, other_sk) = fresh_kp();
    let req_body = challenge_request_body(&other_wallet, &other_sk, &node_id, "ch-wrong-1234567890ab");
    let (s, _) = post_json(api::router(state), "/api/challenges/request", req_body).await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn challenge_request_replayed_nonce_conflicts() {
    let mock = MockServer::start(MockState::new(2_500_000)).await;
    let state = build_state(vec![mock.url()]).await;
    let (wallet, sk, node_id) = register_node(state.clone()).await;
    let nonce = "ch-replay-nonce-1234567890";
    let body = challenge_request_body(&wallet, &sk, &node_id, nonce);
    let (s1, _) = post_json(api::router(state.clone()), "/api/challenges/request", body.clone()).await;
    let (s2, _) = post_json(api::router(state), "/api/challenges/request", body).await;
    // First passes signature check + uses nonce. Second has stale timestamp/sig too but
    // CONFLICT on the nonce is what we're asserting.
    assert_eq!(s1, StatusCode::OK);
    assert_eq!(s2, StatusCode::CONFLICT);
}

#[tokio::test]
async fn challenge_submit_unknown_id_is_404() {
    let mock = MockServer::start(MockState::new(2_500_000)).await;
    let state = build_state(vec![mock.url()]).await;
    let (wallet, sk, _) = register_node(state.clone()).await;
    let fake_id = uuid::Uuid::new_v4().to_string();
    let body = challenge_submit_body(&wallet, &sk, &fake_id, "deadbeef", "ch-404-1234567890abcd");
    let (s, _) = post_json(api::router(state), "/api/challenges/submit", body).await;
    assert_eq!(s, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn challenge_submit_normalises_0x_prefix() {
    // operator includes a leading "0x" — server should normalise and still accept.
    let mock = MockServer::start(MockState::new(2_500_000)).await;
    let state = build_state(vec![mock.url()]).await;
    let (wallet, sk, node_id) = register_node(state.clone()).await;
    let req_body = challenge_request_body(&wallet, &sk, &node_id, "ch-0x-req-1234567890abcd");
    let (s, body) = post_json(api::router(state.clone()), "/api/challenges/request", req_body).await;
    assert_eq!(s, StatusCode::OK);
    let cid = body["challenge_id"].as_str().unwrap().to_string();
    let target = body["target_height"].as_u64().unwrap();
    let expected_with_prefix = format!("0xhash-{target:08x}");
    let ans = challenge_submit_body(&wallet, &sk, &cid, &expected_with_prefix, "ch-0x-ans-1234567890");
    let (s, body) = post_json(api::router(state), "/api/challenges/submit", ans).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(body["passed"], true);
}
