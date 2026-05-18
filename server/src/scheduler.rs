use chrono::{Duration as ChronoDuration, Utc};
use std::time::Duration;
use tokio::time::{interval, Instant};

use crate::{state::AppState, types::NodeStatus};

pub fn spawn(state: AppState) {
    tokio::spawn(tip_refresh_loop(state.clone()));
    tokio::spawn(uptime_loop(state.clone()));
    tokio::spawn(staleness_loop(state.clone()));
    tokio::spawn(challenge_expiry_loop(state.clone()));
    if let Some(_) = state.config().snapshot_interval {
        tokio::spawn(snapshot_loop(state));
    }
}

async fn tip_refresh_loop(state: AppState) {
    let mut tick = interval(state.config().heartbeat_interval.max(Duration::from_secs(15)));
    tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    if !state.rpc().is_configured() {
        tracing::warn!("tip_refresh_loop: no trusted rpcs — skipping");
        return;
    }
    loop {
        tick.tick().await;
        match state.rpc().get_block_count().await {
            Ok(h) => {
                state.set_trusted_tip(h).await;
                tracing::debug!(tip = h, "trusted tip refreshed");
            }
            Err(e) => tracing::warn!(error = ?e, "tip refresh failed"),
        }
    }
}

// Award uptime points for nodes that produced an accepted proof within the
// last `2 * uptime_reward_interval`. Mirrors DePINonBNB's uptime ticker concept.
async fn uptime_loop(state: AppState) {
    let interval_dur = state.config().uptime_reward_interval.max(Duration::from_secs(60));
    let mut tick = interval(interval_dur);
    tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    // Skip the first immediate tick — gives the server a moment to come up before crediting.
    tick.tick().await;
    let _start = Instant::now();

    loop {
        tick.tick().await;
        match state.store().list_all_nodes().await {
            Ok(nodes) => {
                let cutoff =
                    Utc::now() - ChronoDuration::from_std(interval_dur * 2).unwrap_or(ChronoDuration::minutes(10));
                for node in nodes {
                    if node.status == NodeStatus::Suspended {
                        continue;
                    }
                    let Some(last_proof) = node.last_proof_at else { continue };
                    if last_proof < cutoff {
                        continue;
                    }
                    // 1 point per tier-unit per tick.
                    let pts = node.kind.reward_tier() as u64;
                    if let Err(e) = state
                        .store()
                        .add_uptime_and_points(node.id, interval_dur.as_secs(), pts)
                        .await
                    {
                        tracing::warn!(error = ?e, node_id = %node.id, "uptime credit failed");
                    }
                }
            }
            Err(e) => tracing::warn!(error = ?e, "uptime loop list_all_nodes failed"),
        }
    }
}

// Mark nodes Stale if no proof in last `staleness_threshold`. Resurrects to Active
// on next accepted proof (handled in proofs handler).
async fn staleness_loop(state: AppState) {
    let dur = state.config().challenge_check_interval.max(Duration::from_secs(60));
    let mut tick = interval(dur * 2);
    tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    loop {
        tick.tick().await;
        let stale_after = ChronoDuration::from_std(state.config().heartbeat_interval * 6)
            .unwrap_or(ChronoDuration::minutes(30));
        let cutoff = Utc::now() - stale_after;
        let nodes = match state.store().list_all_nodes().await {
            Ok(n) => n,
            Err(e) => {
                tracing::warn!(error = ?e, "staleness list failed");
                continue;
            }
        };
        for node in nodes {
            if node.status != NodeStatus::Active {
                continue;
            }
            let Some(last) = node.last_proof_at else { continue };
            if last < cutoff {
                if let Err(e) = state.store().update_node_status(node.id, NodeStatus::Stale).await {
                    tracing::warn!(error = ?e, node_id = %node.id, "marking stale failed");
                }
            }
        }
    }
}

async fn challenge_expiry_loop(state: AppState) {
    let mut tick = interval(state.config().challenge_check_interval.max(Duration::from_secs(30)));
    tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    loop {
        tick.tick().await;
        match state.store().expire_old_challenges(Utc::now()).await {
            Ok(n) if n > 0 => tracing::info!(expired = n, "challenges expired"),
            Ok(_) => {}
            Err(e) => tracing::warn!(error = ?e, "challenge expiry failed"),
        }
    }
}

async fn snapshot_loop(state: AppState) {
    let Some(snap_interval) = state.config().snapshot_interval else {
        return;
    };
    let mut tick = interval(snap_interval);
    tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    // Skip immediate tick — give operators a chance to earn before first snapshot.
    tick.tick().await;
    loop {
        tick.tick().await;
        match crate::merkle::publish_snapshot(&state).await {
            Ok(res) => tracing::info!(cycle = res.cycle, leaves = res.leaves, root = %res.merkle_root, "scheduled snapshot published"),
            Err(e) => tracing::warn!(error = ?e, "scheduled snapshot failed"),
        }
    }
}
