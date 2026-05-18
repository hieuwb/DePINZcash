use anyhow::Context;
use chrono::Utc;
use sha2::{Digest, Sha256};

use crate::state::AppState;

// Snapshot layout (deliberately Solana-friendly so the on-chain Distributor
// can verify a Merkle proof without any tricks):
//
//   leaf  = sha256( base58_wallet || u64_le(points) )
//   node  = sha256( sort(left, right) )                  (sorted-pair so proofs work without index)
//
// All hashes are 32 bytes, hex-encoded for storage / JSON.

#[derive(Debug)]
pub struct PublishResult {
    pub cycle: i64,
    pub merkle_root: String,
    pub leaves: usize,
    pub total_points: u64,
}

pub async fn publish_snapshot(state: &AppState) -> anyhow::Result<PublishResult> {
    let cfg = state.config();
    let mut leaves = state
        .store()
        .total_points_per_wallet(cfg.network.as_str())
        .await
        .context("loading per-wallet point totals")?;
    leaves.sort_by(|a, b| a.0.cmp(&b.0));

    if leaves.is_empty() {
        anyhow::bail!("no eligible wallets — cannot publish empty snapshot");
    }

    let total_points: u64 = leaves.iter().map(|(_, p)| *p).sum();
    let leaf_hashes: Vec<[u8; 32]> = leaves
        .iter()
        .map(|(wallet, pts)| hash_leaf(wallet, *pts))
        .collect();

    let tree = build_tree(&leaf_hashes);
    let root_hex = hex::encode(tree.root);

    // Pick next cycle number = max(existing) + 1 (1-indexed).
    let last_cycle = match state.store().latest_snapshot().await? {
        Some((_, cycle, _, _)) => cycle,
        None => 0,
    };
    let cycle = last_cycle + 1;
    let snapshot_id = state
        .store()
        .insert_snapshot(cycle, &root_hex, total_points, cfg.spl_mint.as_deref())
        .await?;

    for (idx, (wallet, points)) in leaves.iter().enumerate() {
        let leaf_hash = leaf_hashes[idx];
        let proof = tree.proof_for(idx);
        let proof_json = serde_json::json!({
            "siblings": proof.iter().map(hex::encode).collect::<Vec<_>>(),
            "leaf_index": idx,
        });
        state
            .store()
            .insert_snapshot_leaf(
                snapshot_id,
                wallet,
                *points,
                &hex::encode(leaf_hash),
                &serde_json::to_string(&proof_json)?,
            )
            .await?;
    }

    tracing::info!(
        cycle,
        leaves = leaves.len(),
        total_points,
        merkle_root = %root_hex,
        published_at = %Utc::now(),
        "snapshot published"
    );

    Ok(PublishResult {
        cycle,
        merkle_root: root_hex,
        leaves: leaves.len(),
        total_points,
    })
}

pub fn hash_leaf(wallet: &str, points: u64) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(wallet.as_bytes());
    hasher.update(points.to_le_bytes());
    hasher.finalize().into()
}

fn hash_pair_sorted(a: &[u8; 32], b: &[u8; 32]) -> [u8; 32] {
    let (lo, hi) = if a <= b { (a, b) } else { (b, a) };
    let mut hasher = Sha256::new();
    hasher.update(lo);
    hasher.update(hi);
    hasher.finalize().into()
}

struct MerkleTree {
    layers: Vec<Vec<[u8; 32]>>, // layer 0 = leaves
    root: [u8; 32],
}

impl MerkleTree {
    fn proof_for(&self, mut index: usize) -> Vec<[u8; 32]> {
        let mut siblings = Vec::new();
        for layer in &self.layers {
            if layer.len() <= 1 {
                break;
            }
            let sibling_idx = index ^ 1;
            let sib = if sibling_idx < layer.len() {
                layer[sibling_idx]
            } else {
                // Odd node duplicated up.
                layer[index]
            };
            siblings.push(sib);
            index /= 2;
        }
        siblings
    }
}

fn build_tree(leaves: &[[u8; 32]]) -> MerkleTree {
    let mut layers: Vec<Vec<[u8; 32]>> = Vec::new();
    layers.push(leaves.to_vec());

    if leaves.len() == 1 {
        return MerkleTree {
            root: leaves[0],
            layers,
        };
    }

    loop {
        let last = layers.last().unwrap();
        if last.len() == 1 {
            break;
        }
        let mut next = Vec::with_capacity((last.len() + 1) / 2);
        let mut i = 0;
        while i < last.len() {
            if i + 1 < last.len() {
                next.push(hash_pair_sorted(&last[i], &last[i + 1]));
            } else {
                // Odd leaf: hash with itself.
                next.push(hash_pair_sorted(&last[i], &last[i]));
            }
            i += 2;
        }
        layers.push(next);
    }
    let root = *layers.last().unwrap().first().unwrap();
    MerkleTree { layers, root }
}

pub fn verify_proof(leaf: &[u8; 32], proof: &[[u8; 32]], root: &[u8; 32]) -> bool {
    let mut cur = *leaf;
    for sib in proof {
        cur = hash_pair_sorted(&cur, sib);
    }
    &cur == root
}

#[cfg(test)]
mod tests {
    use super::*;

    fn h(b: u8) -> [u8; 32] {
        let mut x = [0u8; 32];
        x[0] = b;
        x
    }

    #[test]
    fn single_leaf_root_is_leaf() {
        let tree = build_tree(&[h(1)]);
        assert_eq!(tree.root, h(1));
        assert!(tree.proof_for(0).is_empty());
    }

    #[test]
    fn two_leaves() {
        let leaves = vec![h(1), h(2)];
        let tree = build_tree(&leaves);
        let proof0 = tree.proof_for(0);
        let proof1 = tree.proof_for(1);
        assert!(verify_proof(&leaves[0], &proof0, &tree.root));
        assert!(verify_proof(&leaves[1], &proof1, &tree.root));
    }

    #[test]
    fn five_leaves_each_verifies() {
        let leaves: Vec<[u8; 32]> = (1u8..=5).map(h).collect();
        let tree = build_tree(&leaves);
        for (i, leaf) in leaves.iter().enumerate() {
            let proof = tree.proof_for(i);
            assert!(verify_proof(leaf, &proof, &tree.root), "leaf {i} failed");
        }
    }

    #[test]
    fn wrong_leaf_fails() {
        let leaves = vec![h(1), h(2), h(3), h(4)];
        let tree = build_tree(&leaves);
        let proof = tree.proof_for(0);
        assert!(!verify_proof(&h(9), &proof, &tree.root));
    }

    #[test]
    fn leaf_hash_deterministic() {
        let a = hash_leaf("Alice", 100);
        let b = hash_leaf("Alice", 100);
        let c = hash_leaf("Alice", 101);
        assert_eq!(a, b);
        assert_ne!(a, c);
    }
}
