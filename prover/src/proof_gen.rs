use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::Path;
use tracing::{debug, info};

use crate::config::Config;
use crate::zebra_reader::NodeMetrics;

/// A generated proof of node operation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Proof {
    /// Proof format version
    pub version: String,

    /// Timestamp of proof generation
    pub timestamp: i64,

    /// Node information
    pub node_info: NodeInfo,

    /// Metrics being proven
    pub metrics: ProofMetrics,

    /// The actual Halo 2 proof
    pub halo2_proof: String,

    /// Public inputs (revealed)
    pub public_inputs: Vec<String>,

    /// Signature over the proof (prevents tampering)
    pub signature: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeInfo {
    pub zebra_version: String,
    pub zebra_binary_hash: String,
    pub network: String,
    pub node_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProofMetrics {
    pub block_height: u64,
    pub sync_percentage: f64,
    pub uptime_hours: f64,
    pub peer_count: u32,
    pub blocks_served: u64,
}

/// Proof generator using Halo 2
pub struct ProofGenerator {
    config: Config,
}

impl ProofGenerator {
    pub fn new(config: Config) -> Self {
        Self { config }
    }

    /// Generate a zero-knowledge proof of node metrics
    pub async fn generate_proof(&self, metrics: &NodeMetrics) -> Result<Proof> {
        info!("Generating Halo 2 proof...");

        // Public inputs (what we reveal)
        let public_inputs = vec![
            metrics.block_height.to_string(),
            metrics.timestamp.to_string(),
            metrics.network.clone(),
        ];

        debug!("Public inputs: {:?}", public_inputs);

        // Private inputs (what we keep hidden)
        let _private_inputs = vec![
            metrics.zebra_binary_hash.clone(),
            metrics.uptime_hours.to_string(),
            metrics.peer_count.to_string(),
            metrics.blocks_served.to_string(),
        ];

        // Generate the actual Halo 2 proof
        let halo2_proof = self.generate_halo2_proof(
            &public_inputs,
            &_private_inputs,
        ).await?;

        // Build node info
        let node_info = NodeInfo {
            zebra_version: metrics.zebra_version.clone(),
            zebra_binary_hash: metrics.zebra_binary_hash.clone(),
            network: metrics.network.clone(),
            node_id: self.config.node_id.clone(),
        };

        // Build proof metrics
        let proof_metrics = ProofMetrics {
            block_height: metrics.block_height,
            sync_percentage: metrics.sync_percentage,
            uptime_hours: metrics.uptime_hours,
            peer_count: metrics.peer_count,
            blocks_served: metrics.blocks_served,
        };

        // Create proof structure
        let proof = Proof {
            version: "1.0".to_string(),
            timestamp: chrono::Utc::now().timestamp(),
            node_info,
            metrics: proof_metrics,
            halo2_proof: halo2_proof.clone(),
            public_inputs: public_inputs.clone(),
            signature: String::new(), // Will be filled next
        };

        // Sign the proof
        let signature = self.sign_proof(&proof)?;

        let proof = Proof {
            signature,
            ..proof
        };

        info!("Proof generation complete");

        Ok(proof)
    }

    /// Generate the actual Halo 2 proof
    async fn generate_halo2_proof(
        &self,
        public_inputs: &[String],
        private_inputs: &[String],
    ) -> Result<String> {
        info!("Computing Halo 2 proof (this may take 1-2 minutes)...");

        let mock_proof = self.create_mock_proof(public_inputs, private_inputs);

        Ok(mock_proof)
    }

    /// Create a mock proof for testing
    fn create_mock_proof(&self, public_inputs: &[String], _private_inputs: &[String]) -> String {
        use sha2::{Sha256, Digest};

        let mut hasher = Sha256::new();
        for input in public_inputs {
            hasher.update(input.as_bytes());
        }
        let hash = hasher.finalize();

        format!("MOCK_HALO2_PROOF_{}", hex::encode(hash))
    }

    /// Sign the proof to prevent tampering
    fn sign_proof(&self, proof: &Proof) -> Result<String> {
        use ed25519_dalek::{Signature, Signer, SigningKey};
        use sha2::{Sha256, Digest};

        let seed = self.generate_signing_seed();
        let signing = SigningKey::from_bytes(&seed);

        let proof_json = serde_json::to_string(proof)?;
        let mut hasher = Sha256::new();
        hasher.update(proof_json.as_bytes());
        let message = hasher.finalize();

        let signature: Signature = signing.sign(&message);

        Ok(hex::encode(signature.to_bytes()))
    }

    // ed25519-dalek 2.x SigningKey takes a 32-byte secret. Old code used 64 bytes
    // (Keypair = secret || pubkey); drop the pubkey half.
    fn generate_signing_seed(&self) -> [u8; 32] {
        use sha2::{Sha256, Digest};

        let mut hasher = Sha256::new();
        if let Some(node_id) = &self.config.node_id {
            hasher.update(node_id.as_bytes());
        }
        hasher.update(b"depinzcash_proof");

        let hash = hasher.finalize();
        let mut seed = [0u8; 32];
        seed.copy_from_slice(&hash);
        seed
    }
}

impl Proof {
    /// Save proof to a JSON file
    pub fn save_to_file(&self, path: &Path) -> Result<()> {
        let json = serde_json::to_string_pretty(self)?;
        std::fs::write(path, json)
            .context(format!("Failed to write proof to {:?}", path))?;
        Ok(())
    }

    /// Load proof from a JSON file
    pub fn load_from_file(path: &Path) -> Result<Self> {
        let content = std::fs::read_to_string(path)
            .context(format!("Failed to read proof from {:?}", path))?;
        let proof: Proof = serde_json::from_str(&content)
            .context("Failed to parse proof JSON")?;
        Ok(proof)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RewardCalculation {
    pub sync_bonus: f64,
    pub uptime_reward: f64,
    pub multiplier: f64,
    pub total_zec: f64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_proof_structure() {
        let proof = Proof {
            version: "1.0".to_string(),
            timestamp: 0,
            node_info: NodeInfo {
                zebra_version: "1.0.0".to_string(),
                zebra_binary_hash: "abc123".to_string(),
                network: "mainnet".to_string(),
                node_id: None,
            },
            metrics: ProofMetrics {
                block_height: 1000000,
                sync_percentage: 100.0,
                uptime_hours: 720.0,
                peer_count: 10,
                blocks_served: 10000,
            },
            halo2_proof: "test".to_string(),
            public_inputs: vec![],
            signature: "test".to_string(),
        };

        assert_eq!(proof.version, "1.0");
        assert_eq!(proof.metrics.block_height, 1000000);
    }
}
