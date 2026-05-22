use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum NodeKind {
    ZebraFull,
    Lightwalletd,
}

impl NodeKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            NodeKind::ZebraFull => "zebra-full",
            NodeKind::Lightwalletd => "lightwalletd",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "zebra-full" | "zebra_full" | "zebrafull" => Some(NodeKind::ZebraFull),
            "lightwalletd" | "lwd" => Some(NodeKind::Lightwalletd),
            _ => None,
        }
    }

    // Higher tier = larger reward weight in the points engine.
    pub fn reward_tier(&self) -> u32 {
        match self {
            NodeKind::ZebraFull => 10,
            NodeKind::Lightwalletd => 6,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum NodeStatus {
    Registered,
    Active,
    Stale,
    Suspended,
}

impl NodeStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            NodeStatus::Registered => "registered",
            NodeStatus::Active => "active",
            NodeStatus::Stale => "stale",
            NodeStatus::Suspended => "suspended",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "registered" => Some(NodeStatus::Registered),
            "active" => Some(NodeStatus::Active),
            "stale" => Some(NodeStatus::Stale),
            "suspended" => Some(NodeStatus::Suspended),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Node {
    pub id: Uuid,
    pub wallet: String, // Solana base58 pubkey
    pub kind: NodeKind,
    pub label: Option<String>,
    pub rpc_endpoint: Option<String>,
    pub network: String, // "mainnet" or "testnet"
    pub status: NodeStatus,
    pub last_height: Option<u64>,
    pub last_block_hash: Option<String>,
    pub last_proof_at: Option<DateTime<Utc>>,
    pub registered_at: DateTime<Utc>,
    pub points: u64,
    pub uptime_seconds: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ProofVerdict {
    Pending,
    Accepted,
    Rejected,
}

impl ProofVerdict {
    pub fn as_str(&self) -> &'static str {
        match self {
            ProofVerdict::Pending => "pending",
            ProofVerdict::Accepted => "accepted",
            ProofVerdict::Rejected => "rejected",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "pending" => Some(ProofVerdict::Pending),
            "accepted" => Some(ProofVerdict::Accepted),
            "rejected" => Some(ProofVerdict::Rejected),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Proof {
    pub id: Uuid,
    pub node_id: Uuid,
    pub wallet: String,
    pub claimed_height: u64,
    pub claimed_block_hash: String,
    pub proof_timestamp: DateTime<Utc>,
    pub binary_hash: Option<String>,
    pub uptime_seconds: Option<u64>,
    pub peers: Option<u32>,
    pub verdict: ProofVerdict,
    pub reject_reason: Option<String>,
    pub points_awarded: u64,
    pub received_at: DateTime<Utc>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ChallengeKind {
    // "What's the block hash at height H?" — server picks H from trusted quorum, operator answers.
    BlockHash,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ChallengeStatus {
    Open,
    Answered,
    Expired,
}

impl ChallengeStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            ChallengeStatus::Open => "open",
            ChallengeStatus::Answered => "answered",
            ChallengeStatus::Expired => "expired",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "open" => Some(ChallengeStatus::Open),
            "answered" => Some(ChallengeStatus::Answered),
            "expired" => Some(ChallengeStatus::Expired),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Challenge {
    pub id: Uuid,
    pub node_id: Uuid,
    pub kind: ChallengeKind,
    pub target_height: u64,
    pub expected_hash: String,
    pub issued_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    pub status: ChallengeStatus,
    pub answered_at: Option<DateTime<Utc>>,
    pub passed: Option<bool>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct NodeDailyBucket {
    pub day: String,
    pub proofs: u64,
    pub accepted: u64,
    pub points: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct WalletStats {
    pub wallet: String,
    pub nodes: u32,
    pub total_points: u64,
    pub total_uptime_seconds: u64,
    pub last_seen: Option<DateTime<Utc>>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct NetworkStats {
    pub total_nodes: u32,
    pub active_nodes: u32,
    pub total_proofs: u64,
    pub accepted_proofs: u64,
    pub total_points: u64,
    pub network: String,
    pub spl_mint: Option<String>,
    pub solana_cluster: String,
    pub trusted_tip_height: Option<u64>,
}
