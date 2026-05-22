// Direct store-level tests against the SQLite backend. These bypass the HTTP
// layer and exercise the persistence guarantees the store promises to the rest
// of the system: idempotent migrations, uniqueness, monotonic point accrual,
// nonce single-use, snapshot insertion, etc.

use chrono::Utc;
use depinzcash_server::{
    store::SqliteStore,
    types::{Challenge, ChallengeKind, ChallengeStatus, Node, NodeKind, NodeStatus, Proof, ProofVerdict},
};
use uuid::Uuid;

async fn fresh_store() -> SqliteStore {
    let s = SqliteStore::connect("sqlite::memory:").await.unwrap();
    s.migrate().await.unwrap();
    s
}

fn sample_node(wallet: &str, label: Option<&str>) -> Node {
    Node {
        id: Uuid::new_v4(),
        wallet: wallet.to_string(),
        kind: NodeKind::ZebraFull,
        label: label.map(String::from),
        rpc_endpoint: None,
        network: "mainnet".into(),
        status: NodeStatus::Registered,
        last_height: None,
        last_block_hash: None,
        last_proof_at: None,
        registered_at: Utc::now(),
        points: 0,
        uptime_seconds: 0,
    }
}

fn sample_proof(node_id: Uuid, wallet: &str, height: u64, hash: &str, verdict: ProofVerdict, pts: u64) -> Proof {
    Proof {
        id: Uuid::new_v4(),
        node_id,
        wallet: wallet.to_string(),
        claimed_height: height,
        claimed_block_hash: hash.to_string(),
        proof_timestamp: Utc::now(),
        binary_hash: None,
        uptime_seconds: Some(3600),
        peers: Some(8),
        verdict,
        reject_reason: None,
        points_awarded: pts,
        received_at: Utc::now(),
    }
}

// ---- migrations + connection -------------------------------------------

#[tokio::test]
async fn migrate_is_idempotent() {
    let store = fresh_store().await;
    // Running again must not error.
    store.migrate().await.unwrap();
    store.migrate().await.unwrap();
}

// ---- node CRUD ---------------------------------------------------------

#[tokio::test]
async fn insert_then_fetch_node() {
    let store = fresh_store().await;
    let node = sample_node("walletA", Some("primary"));
    store.insert_node(&node, "tok-123").await.unwrap();
    let got = store.get_node(node.id).await.unwrap().unwrap();
    assert_eq!(got.wallet, "walletA");
    assert_eq!(got.label.as_deref(), Some("primary"));
    assert_eq!(got.status, NodeStatus::Registered);
}

#[tokio::test]
async fn get_node_returns_none_for_unknown_id() {
    let store = fresh_store().await;
    let result = store.get_node(Uuid::new_v4()).await.unwrap();
    assert!(result.is_none());
}

#[tokio::test]
async fn auth_token_lookup_returns_node() {
    let store = fresh_store().await;
    let node = sample_node("walletA", None);
    store.insert_node(&node, "secret-token-xyz").await.unwrap();
    let by_tok = store.get_node_by_auth_token("secret-token-xyz").await.unwrap().unwrap();
    assert_eq!(by_tok.id, node.id);
    assert!(store.get_node_by_auth_token("not-a-real-token").await.unwrap().is_none());
}

#[tokio::test]
async fn list_nodes_by_wallet_orders_by_registered_at() {
    let store = fresh_store().await;
    let n1 = sample_node("multi", Some("first"));
    let n2 = sample_node("multi", Some("second"));
    let n3 = sample_node("other", None);
    store.insert_node(&n1, "t1").await.unwrap();
    store.insert_node(&n2, "t2").await.unwrap();
    store.insert_node(&n3, "t3").await.unwrap();

    let multi_nodes = store.list_nodes_by_wallet("multi").await.unwrap();
    assert_eq!(multi_nodes.len(), 2);
    assert_eq!(multi_nodes[0].label.as_deref(), Some("first"));
    assert_eq!(multi_nodes[1].label.as_deref(), Some("second"));

    let other = store.list_nodes_by_wallet("other").await.unwrap();
    assert_eq!(other.len(), 1);

    let absent = store.list_nodes_by_wallet("ghost").await.unwrap();
    assert!(absent.is_empty());
}

#[tokio::test]
async fn update_node_status_persists() {
    let store = fresh_store().await;
    let node = sample_node("walletA", None);
    store.insert_node(&node, "tok").await.unwrap();
    store.update_node_status(node.id, NodeStatus::Stale).await.unwrap();
    let updated = store.get_node(node.id).await.unwrap().unwrap();
    assert_eq!(updated.status, NodeStatus::Stale);
}

#[tokio::test]
async fn apply_proof_acceptance_updates_node_state() {
    let store = fresh_store().await;
    let node = sample_node("walletA", None);
    store.insert_node(&node, "tok").await.unwrap();

    let when = Utc::now();
    store
        .apply_proof_acceptance(node.id, 12345, "blockhash-abc", 50, when)
        .await
        .unwrap();
    let n = store.get_node(node.id).await.unwrap().unwrap();
    assert_eq!(n.last_height, Some(12345));
    assert_eq!(n.last_block_hash.as_deref(), Some("blockhash-abc"));
    assert_eq!(n.points, 50);
    assert_eq!(n.status, NodeStatus::Active);
    assert!(n.last_proof_at.is_some());
}

#[tokio::test]
async fn add_uptime_and_points_is_additive() {
    let store = fresh_store().await;
    let node = sample_node("walletA", None);
    store.insert_node(&node, "tok").await.unwrap();

    store.add_uptime_and_points(node.id, 300, 5).await.unwrap();
    store.add_uptime_and_points(node.id, 300, 5).await.unwrap();
    store.add_uptime_and_points(node.id, 300, 7).await.unwrap();
    let n = store.get_node(node.id).await.unwrap().unwrap();
    assert_eq!(n.uptime_seconds, 900);
    assert_eq!(n.points, 17);
}

// ---- proof storage ------------------------------------------------------

#[tokio::test]
async fn insert_proof_and_list_for_wallet() {
    let store = fresh_store().await;
    let node = sample_node("walletA", None);
    store.insert_node(&node, "tok").await.unwrap();

    let p1 = sample_proof(node.id, "walletA", 100, "h-100", ProofVerdict::Accepted, 50);
    let p2 = sample_proof(node.id, "walletA", 101, "h-101", ProofVerdict::Accepted, 50);
    store.insert_proof(&p1).await.unwrap();
    store.insert_proof(&p2).await.unwrap();

    let proofs = store.list_proofs_by_wallet("walletA", 10).await.unwrap();
    assert_eq!(proofs.len(), 2);
    // Most recent first.
    assert_eq!(proofs[0].claimed_height, 101);

    let count_dup = store.count_proof(node.id, 100, "h-100").await.unwrap();
    assert_eq!(count_dup, 1);
    let count_absent = store.count_proof(node.id, 999, "missing").await.unwrap();
    assert_eq!(count_absent, 0);
}

#[tokio::test]
async fn duplicate_proof_height_hash_for_same_node_conflicts() {
    let store = fresh_store().await;
    let node = sample_node("walletA", None);
    store.insert_node(&node, "tok").await.unwrap();

    let p1 = sample_proof(node.id, "walletA", 100, "h-100", ProofVerdict::Accepted, 50);
    store.insert_proof(&p1).await.unwrap();

    let p2 = sample_proof(node.id, "walletA", 100, "h-100", ProofVerdict::Accepted, 50);
    let err = store.insert_proof(&p2).await;
    assert!(err.is_err(), "expected UNIQUE constraint violation");
}

#[tokio::test]
async fn last_accepted_proof_for_node_returns_most_recent() {
    let store = fresh_store().await;
    let node = sample_node("walletA", None);
    store.insert_node(&node, "tok").await.unwrap();

    store
        .insert_proof(&sample_proof(node.id, "walletA", 100, "h-100", ProofVerdict::Accepted, 10))
        .await
        .unwrap();
    store
        .insert_proof(&sample_proof(node.id, "walletA", 101, "h-101", ProofVerdict::Rejected, 0))
        .await
        .unwrap();
    store
        .insert_proof(&sample_proof(node.id, "walletA", 102, "h-102", ProofVerdict::Accepted, 20))
        .await
        .unwrap();

    let last = store.last_accepted_proof_for_node(node.id).await.unwrap().unwrap();
    assert_eq!(last.claimed_height, 102);
}

// ---- nonce single-use ---------------------------------------------------

#[tokio::test]
async fn nonce_is_single_use_per_wallet() {
    let store = fresh_store().await;
    let ok = store.try_use_nonce("nonce-abc", "walletA").await.unwrap();
    assert!(ok);
    let again = store.try_use_nonce("nonce-abc", "walletA").await.unwrap();
    assert!(!again, "second attempt must return false");
}

#[tokio::test]
async fn nonce_collisions_across_wallets_still_global() {
    // Our schema makes nonce globally unique; same nonce across wallets must collide.
    let store = fresh_store().await;
    let first = store.try_use_nonce("shared-nonce", "walletA").await.unwrap();
    let second = store.try_use_nonce("shared-nonce", "walletB").await.unwrap();
    assert!(first);
    assert!(!second);
}

// ---- challenges ---------------------------------------------------------

#[tokio::test]
async fn challenge_insert_then_get() {
    let store = fresh_store().await;
    let node = sample_node("walletA", None);
    store.insert_node(&node, "tok").await.unwrap();
    let ch = Challenge {
        id: Uuid::new_v4(),
        node_id: node.id,
        kind: ChallengeKind::BlockHash,
        target_height: 12345,
        expected_hash: "abcdef".to_string(),
        issued_at: Utc::now(),
        expires_at: Utc::now() + chrono::Duration::minutes(10),
        status: ChallengeStatus::Open,
        answered_at: None,
        passed: None,
    };
    store.insert_challenge(&ch).await.unwrap();

    let got = store.get_challenge(ch.id).await.unwrap().unwrap();
    assert_eq!(got.target_height, 12345);
    assert_eq!(got.status, ChallengeStatus::Open);

    let open = store.get_open_challenge_for_node(node.id).await.unwrap().unwrap();
    assert_eq!(open.id, ch.id);
}

#[tokio::test]
async fn mark_challenge_answered_persists() {
    let store = fresh_store().await;
    let node = sample_node("walletA", None);
    store.insert_node(&node, "tok").await.unwrap();
    let ch = Challenge {
        id: Uuid::new_v4(),
        node_id: node.id,
        kind: ChallengeKind::BlockHash,
        target_height: 12345,
        expected_hash: "expect".to_string(),
        issued_at: Utc::now(),
        expires_at: Utc::now() + chrono::Duration::minutes(10),
        status: ChallengeStatus::Open,
        answered_at: None,
        passed: None,
    };
    store.insert_challenge(&ch).await.unwrap();
    store
        .mark_challenge_answered(ch.id, true, Utc::now())
        .await
        .unwrap();

    let got = store.get_challenge(ch.id).await.unwrap().unwrap();
    assert_eq!(got.status, ChallengeStatus::Answered);
    assert_eq!(got.passed, Some(true));
    assert!(got.answered_at.is_some());
}

#[tokio::test]
async fn expire_old_challenges_only_touches_open() {
    let store = fresh_store().await;
    let node = sample_node("walletA", None);
    store.insert_node(&node, "tok").await.unwrap();

    let old_expired = Challenge {
        id: Uuid::new_v4(),
        node_id: node.id,
        kind: ChallengeKind::BlockHash,
        target_height: 1,
        expected_hash: "h1".into(),
        issued_at: Utc::now() - chrono::Duration::hours(2),
        expires_at: Utc::now() - chrono::Duration::hours(1),
        status: ChallengeStatus::Open,
        answered_at: None,
        passed: None,
    };
    let fresh_open = Challenge {
        id: Uuid::new_v4(),
        node_id: node.id,
        kind: ChallengeKind::BlockHash,
        target_height: 2,
        expected_hash: "h2".into(),
        issued_at: Utc::now(),
        expires_at: Utc::now() + chrono::Duration::hours(1),
        status: ChallengeStatus::Open,
        answered_at: None,
        passed: None,
    };
    store.insert_challenge(&old_expired).await.unwrap();
    store.insert_challenge(&fresh_open).await.unwrap();

    let affected = store.expire_old_challenges(Utc::now()).await.unwrap();
    assert_eq!(affected, 1);

    let after = store.get_challenge(old_expired.id).await.unwrap().unwrap();
    assert_eq!(after.status, ChallengeStatus::Expired);
    let still_open = store.get_challenge(fresh_open.id).await.unwrap().unwrap();
    assert_eq!(still_open.status, ChallengeStatus::Open);
}

// ---- stats --------------------------------------------------------------

#[tokio::test]
async fn network_stats_count_correctly() {
    let store = fresh_store().await;
    // network_stats only counts nodes that have produced an accepted proof
    // (last_proof_at IS NOT NULL) — registration-only spam doesn't inflate
    // the public counter.
    for (i, w) in ["wA", "wB", "wC"].iter().enumerate() {
        let mut n = sample_node(w, None);
        n.status = if i == 2 { NodeStatus::Registered } else { NodeStatus::Active };
        if i < 2 {
            n.last_proof_at = Some(Utc::now());
        }
        store.insert_node(&n, &format!("t{i}")).await.unwrap();
    }
    let stats = store.network_stats("mainnet").await.unwrap();
    // 2 nodes have proofs (wA active, wB active), 1 has none (wC registered)
    assert_eq!(stats.total_nodes, 2);
    assert_eq!(stats.active_nodes, 2);
    assert_eq!(stats.network, "mainnet");
}

#[tokio::test]
async fn leaderboard_orders_by_total_points_desc() {
    let store = fresh_store().await;
    for (i, (wallet, pts)) in [("low", 5u64), ("mid", 50), ("hi", 500)].iter().enumerate() {
        let n = sample_node(wallet, None);
        store.insert_node(&n, &format!("t{i}")).await.unwrap();
        store.add_uptime_and_points(n.id, 0, *pts).await.unwrap();
    }
    let board = store.leaderboard("mainnet", 10).await.unwrap();
    assert_eq!(board[0].wallet, "hi");
    assert_eq!(board[0].total_points, 500);
    assert_eq!(board[1].wallet, "mid");
    assert_eq!(board[2].wallet, "low");
}

#[tokio::test]
async fn wallet_stats_aggregates_across_nodes() {
    let store = fresh_store().await;
    let a = sample_node("walletA", Some("a1"));
    let b = sample_node("walletA", Some("a2"));
    store.insert_node(&a, "t1").await.unwrap();
    store.insert_node(&b, "t2").await.unwrap();
    store.add_uptime_and_points(a.id, 1000, 30).await.unwrap();
    store.add_uptime_and_points(b.id, 2000, 20).await.unwrap();

    let stats = store.wallet_stats("walletA").await.unwrap();
    assert_eq!(stats.nodes, 2);
    assert_eq!(stats.total_points, 50);
    assert_eq!(stats.total_uptime_seconds, 3000);
}

// ---- snapshots ----------------------------------------------------------

#[tokio::test]
async fn snapshot_cycle_increments() {
    let store = fresh_store().await;
    let n = sample_node("walletA", None);
    store.insert_node(&n, "tok").await.unwrap();
    store.add_uptime_and_points(n.id, 0, 100).await.unwrap();

    let id1 = store.insert_snapshot(1, "root-1", 100, Some("mint")).await.unwrap();
    let id2 = store.insert_snapshot(2, "root-2", 200, Some("mint")).await.unwrap();
    assert_ne!(id1, id2);

    let latest = store.latest_snapshot().await.unwrap().unwrap();
    assert_eq!(latest.1, 2);
    assert_eq!(latest.2, "root-2");
}

#[tokio::test]
async fn snapshot_leaf_round_trip() {
    let store = fresh_store().await;
    let sid = store.insert_snapshot(1, "root-x", 100, None).await.unwrap();
    store
        .insert_snapshot_leaf(sid, "walletA", 100, "leaf-hash-A", r#"{"siblings":[],"leaf_index":0}"#)
        .await
        .unwrap();
    let leaf = store.snapshot_leaf_for_wallet(sid, "walletA").await.unwrap().unwrap();
    assert_eq!(leaf.0, 100);
    assert_eq!(leaf.1, "leaf-hash-A");
    assert!(leaf.2.contains("siblings"));

    let missing = store.snapshot_leaf_for_wallet(sid, "ghost").await.unwrap();
    assert!(missing.is_none());
}

#[tokio::test]
async fn total_points_per_wallet_filters_zeroes() {
    let store = fresh_store().await;
    let pay = sample_node("paid", None);
    let zero = sample_node("zero", None);
    store.insert_node(&pay, "t1").await.unwrap();
    store.insert_node(&zero, "t2").await.unwrap();
    store.add_uptime_and_points(pay.id, 0, 42).await.unwrap();

    let rows = store.total_points_per_wallet("mainnet").await.unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].0, "paid");
    assert_eq!(rows[0].1, 42);
}

#[tokio::test]
async fn snapshots_belong_to_their_app_only() {
    // Two separate in-memory stores must not see each other's data.
    let s1 = fresh_store().await;
    let s2 = fresh_store().await;

    s1.insert_snapshot(1, "root", 100, None).await.unwrap();
    let s1_latest = s1.latest_snapshot().await.unwrap();
    let s2_latest = s2.latest_snapshot().await.unwrap();

    assert!(s1_latest.is_some());
    assert!(s2_latest.is_none());
}
