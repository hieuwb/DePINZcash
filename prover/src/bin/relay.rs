// depinzcash-relay — operator-side CLI that
//   1. generates a Solana keypair (or loads an existing one),
//   2. registers a Zebra node with the DePINZcash server,
//   3. submits proofs of node state on a fixed interval.
//
// This is the "fully working prototype" submission path. The Halo 2 proof generator
// (the `depinzcash-prover` binary) is the privacy-preserving variant — once the
// server supports verifying Halo 2 proofs, this relay can swap its payload.

use std::{fs, path::PathBuf, time::Duration};

use anyhow::{anyhow, Context, Result};
use chrono::Utc;
use clap::{Parser, Subcommand};
use ed25519_dalek::{Signer, SigningKey};
use rand::RngCore;
use serde::{Deserialize, Serialize};
use serde_json::json;
use sha2::{Digest, Sha256};

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand, Debug)]
enum Cmd {
    Keygen(KeygenArgs),
    Register(RegisterArgs),
    Submit(SubmitArgs),
    Watch(WatchArgs),
}

#[derive(Parser, Debug)]
struct KeygenArgs {
    #[arg(short, long, default_value = "config/solana-keypair.json")]
    out: PathBuf,
}

#[derive(Parser, Debug)]
struct RegisterArgs {
    #[arg(long, env = "DEPINZCASH_API", default_value = "http://localhost:3000")]
    api: String,
    #[arg(
        long,
        env = "SOLANA_KEYPAIR",
        default_value = "config/solana-keypair.json"
    )]
    keypair: PathBuf,
    #[arg(long, default_value = "zebra-full")]
    kind: String,
    #[arg(long, default_value = "")]
    label: String,
    #[arg(long)]
    rpc_endpoint: Option<String>,
    // Where to persist the credentials returned by /api/nodes/register.
    #[arg(long, default_value = "config/relay-state.json")]
    state: PathBuf,
}

#[derive(Parser, Debug)]
struct SubmitArgs {
    #[arg(long, env = "DEPINZCASH_API", default_value = "http://localhost:3000")]
    api: String,
    #[arg(
        long,
        env = "SOLANA_KEYPAIR",
        default_value = "config/solana-keypair.json"
    )]
    keypair: PathBuf,
    #[arg(long, default_value = "config/relay-state.json")]
    state: PathBuf,
    // Source of node metrics, in priority order:
    //   1. --node-rpc <url>         — query Zebra each tick (recommended for `watch`)
    //   2. --proof-file <path>      — read from a depinzcash-prover JSON
    //   3. --height + --block-hash  — explicit one-off submission (for testing)
    #[arg(long, env = "NODE_RPC")]
    node_rpc: Option<String>,
    #[arg(long)]
    proof_file: Option<PathBuf>,
    #[arg(long)]
    height: Option<u64>,
    #[arg(long)]
    block_hash: Option<String>,
    #[arg(long, default_value_t = 0)]
    uptime_seconds: u64,
    #[arg(long, default_value_t = 0)]
    peers: u32,
    #[arg(long)]
    binary_hash: Option<String>,
}

#[derive(Parser, Debug)]
struct WatchArgs {
    #[command(flatten)]
    submit: SubmitArgs,
    #[arg(long, default_value_t = 300)]
    interval_secs: u64,
}

#[derive(Serialize, Deserialize, Debug)]
struct KeypairFile {
    // Stored format: 64-byte concatenation of secret-key (32 bytes) + public-key (32 bytes),
    // base58-encoded. Same convention as `solana-keygen` minus the JSON byte-array wrapper.
    keypair_b58: String,
}

#[derive(Serialize, Deserialize, Debug)]
struct RelayState {
    api: String,
    wallet: String,
    node_id: String,
    auth_token: String,
    kind: String,
    label: String,
    registered_at: chrono::DateTime<Utc>,
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();
    let args = Args::parse();
    match args.cmd {
        Cmd::Keygen(a) => keygen(a),
        Cmd::Register(a) => register(a).await,
        Cmd::Submit(a) => {
            submit_once(&a).await?;
            Ok(())
        }
        Cmd::Watch(a) => watch(a).await,
    }
}

fn keygen(args: KeygenArgs) -> Result<()> {
    if args.out.exists() {
        return Err(anyhow!(
            "refusing to overwrite existing keypair at {:?}",
            args.out
        ));
    }
    if let Some(parent) = args.out.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut secret = [0u8; 32];
    rand::thread_rng().fill_bytes(&mut secret);
    let sk = SigningKey::from_bytes(&secret);
    let mut full = [0u8; 64];
    full[..32].copy_from_slice(&secret);
    full[32..].copy_from_slice(&sk.verifying_key().to_bytes());
    let file = KeypairFile {
        keypair_b58: bs58::encode(full).into_string(),
    };
    fs::write(&args.out, serde_json::to_vec_pretty(&file)?)?;
    println!("wrote keypair to {:?}", args.out);
    println!(
        "wallet (public key): {}",
        bs58::encode(sk.verifying_key().to_bytes()).into_string()
    );
    Ok(())
}

fn load_keypair(path: &PathBuf) -> Result<(String, SigningKey)> {
    let bytes = fs::read(path).with_context(|| format!("reading keypair file {:?}", path))?;
    let file: KeypairFile = serde_json::from_slice(&bytes).context("parsing keypair file")?;
    let full = bs58::decode(&file.keypair_b58)
        .into_vec()
        .context("base58 decoding keypair")?;
    if full.len() != 64 {
        return Err(anyhow!("expected 64-byte keypair, got {}", full.len()));
    }
    let secret: [u8; 32] = full[..32].try_into().unwrap();
    let sk = SigningKey::from_bytes(&secret);
    let wallet = bs58::encode(sk.verifying_key().to_bytes()).into_string();
    Ok((wallet, sk))
}

async fn register(args: RegisterArgs) -> Result<()> {
    let (wallet, sk) = load_keypair(&args.keypair)?;
    let nonce = random_nonce();
    let ts = Utc::now();

    // Server's registration_message must match exactly.
    let label = args.label.clone();
    let msg = registration_message(
        &wallet,
        &nonce,
        &ts.to_rfc3339(),
        &args.kind,
        "mainnet",
        &label,
    );
    let sig = sign_b58(&sk, &msg);

    let body = json!({
        "wallet": wallet,
        "signature": sig,
        "nonce": nonce,
        "timestamp": ts.to_rfc3339(),
        "kind": args.kind,
        "label": if args.label.is_empty() { None } else { Some(args.label.clone()) },
        "rpc_endpoint": args.rpc_endpoint,
    });

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(15))
        .build()?;
    let url = format!("{}/api/nodes/register", args.api.trim_end_matches('/'));
    let resp = client.post(&url).json(&body).send().await?;
    let status = resp.status();
    let text = resp.text().await?;
    if !status.is_success() {
        return Err(anyhow!("register failed ({}): {}", status, text));
    }
    let v: serde_json::Value = serde_json::from_str(&text)?;
    let node_id = v["node"]["id"]
        .as_str()
        .ok_or_else(|| anyhow!("missing node.id"))?
        .to_string();
    let auth_token = v["auth_token"]
        .as_str()
        .ok_or_else(|| anyhow!("missing auth_token"))?
        .to_string();

    let state = RelayState {
        api: args.api.clone(),
        wallet: wallet.clone(),
        node_id: node_id.clone(),
        auth_token,
        kind: args.kind.clone(),
        label,
        registered_at: ts,
    };
    if let Some(parent) = args.state.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(&args.state, serde_json::to_vec_pretty(&state)?)?;
    println!("registered node {}", node_id);
    println!("state saved to {:?}", args.state);
    Ok(())
}

async fn submit_once(args: &SubmitArgs) -> Result<serde_json::Value> {
    let (wallet, sk) = load_keypair(&args.keypair)?;
    let state: RelayState = serde_json::from_slice(
        &fs::read(&args.state).with_context(|| format!("reading relay state {:?}", args.state))?,
    )
    .context("parsing relay state")?;

    if state.wallet != wallet {
        return Err(anyhow!(
            "keypair wallet {} does not match registered state wallet {}",
            wallet,
            state.wallet
        ));
    }

    let inferred_uptime_seconds = if args.uptime_seconds == 0 {
        Utc::now()
            .signed_duration_since(state.registered_at)
            .num_seconds()
            .max(0) as u64
    } else {
        args.uptime_seconds
    };

    let (height, block_hash, uptime, peers, binary_hash) =
        gather_metrics(args, inferred_uptime_seconds)
            .await
            .context("gathering metrics")?;

    let nonce = random_nonce();
    let proof_ts = Utc::now();
    let msg = proof_message(
        &wallet,
        &state.node_id,
        height,
        &block_hash,
        &proof_ts.to_rfc3339(),
        &nonce,
    );
    let sig = sign_b58(&sk, &msg);

    let body = json!({
        "wallet": wallet,
        "node_id": state.node_id,
        "signature": sig,
        "nonce": nonce,
        "claimed_height": height,
        "claimed_block_hash": block_hash,
        "proof_timestamp": proof_ts.to_rfc3339(),
        "uptime_seconds": uptime,
        "peers": peers,
        "binary_hash": binary_hash,
    });

    let url = format!("{}/api/proofs/submit", args.api.trim_end_matches('/'));
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(15))
        .build()?;
    let resp = client.post(&url).json(&body).send().await?;
    let status = resp.status();
    let text = resp.text().await?;
    if !status.is_success() {
        return Err(anyhow!("submit failed ({}): {}", status, text));
    }
    let v: serde_json::Value = serde_json::from_str(&text)?;
    println!(
        "submitted height={} uptime={} peers={} verdict={} points={}",
        height,
        uptime,
        peers,
        v.get("verdict").and_then(|x| x.as_str()).unwrap_or("?"),
        v.get("points_awarded")
            .and_then(|x| x.as_u64())
            .unwrap_or(0)
    );
    Ok(v)
}

async fn watch(args: WatchArgs) -> Result<()> {
    let mut tick = tokio::time::interval(Duration::from_secs(args.interval_secs.max(15)));
    tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    loop {
        tick.tick().await;
        match submit_once(&args.submit).await {
            Ok(_) => {}
            Err(e) => tracing::warn!(error = ?e, "submit failed"),
        }
    }
}

async fn gather_metrics(
    args: &SubmitArgs,
    inferred_uptime_seconds: u64,
) -> Result<(u64, String, u64, u32, Option<String>)> {
    // 1. live Zebra RPC has highest precedence — every tick reflects the current tip.
    if let Some(rpc_url) = &args.node_rpc {
        let (height, block_hash) = query_zebra_tip(rpc_url).await?;
        let peers = if args.peers == 0 {
            query_zebra_peer_count(rpc_url).await.unwrap_or_else(|e| {
                tracing::warn!(error = ?e, "zebra peer count unavailable; using peers=0");
                0
            })
        } else {
            args.peers
        };
        return Ok((
            height,
            block_hash,
            inferred_uptime_seconds,
            peers,
            args.binary_hash.clone(),
        ));
    }
    if let Some(path) = &args.proof_file {
        let bytes = fs::read(path).with_context(|| format!("reading proof file {:?}", path))?;
        let v: serde_json::Value = serde_json::from_slice(&bytes)?;
        let height = v["metrics"]["block_height"]
            .as_u64()
            .ok_or_else(|| anyhow!("proof file missing metrics.block_height"))?;
        let block_hash = v["metrics"]["block_hash"]
            .as_str()
            .map(|s| s.to_string())
            .or_else(|| {
                v["public_inputs"]
                    .as_array()
                    .and_then(|arr| arr.first())
                    .and_then(|x| x.as_str())
                    .map(String::from)
            })
            .ok_or_else(|| anyhow!("proof file missing block hash"))?;
        let uptime_hours = v["metrics"]["uptime_hours"].as_f64().unwrap_or(0.0);
        let uptime_seconds = (uptime_hours * 3600.0) as u64;
        let peers = v["metrics"]["peer_count"].as_u64().unwrap_or(0) as u32;
        let binary_hash = v["node_info"]["zebra_binary_hash"]
            .as_str()
            .map(String::from);
        Ok((height, block_hash, uptime_seconds, peers, binary_hash))
    } else {
        let height = args.height.ok_or_else(|| {
            anyhow!("provide --node-rpc, --proof-file, or --height + --block-hash")
        })?;
        let block_hash = args
            .block_hash
            .clone()
            .ok_or_else(|| anyhow!("--block-hash required when --proof-file is absent"))?;
        Ok((
            height,
            block_hash,
            inferred_uptime_seconds,
            args.peers,
            args.binary_hash.clone(),
        ))
    }
}

// Hits Zebra's JSON-RPC for the current tip. Returns (height, best_block_hash).
async fn query_zebra_tip(rpc_url: &str) -> Result<(u64, String)> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(5))
        .build()?;

    let height: u64 = rpc_call(&client, rpc_url, "getblockcount", json!([]))
        .await?
        .as_u64()
        .ok_or_else(|| anyhow!("getblockcount: result is not a number"))?;
    let hash: String = rpc_call(&client, rpc_url, "getbestblockhash", json!([]))
        .await?
        .as_str()
        .map(String::from)
        .ok_or_else(|| anyhow!("getbestblockhash: result is not a string"))?;

    Ok((height, hash))
}

async fn query_zebra_peer_count(rpc_url: &str) -> Result<u32> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(5))
        .build()?;
    let peers = rpc_call(&client, rpc_url, "getpeerinfo", json!([])).await?;
    let count = peers
        .as_array()
        .ok_or_else(|| anyhow!("getpeerinfo: result is not an array"))?
        .len();
    Ok(count.min(u32::MAX as usize) as u32)
}

async fn rpc_call(
    client: &reqwest::Client,
    url: &str,
    method: &str,
    params: serde_json::Value,
) -> Result<serde_json::Value> {
    let body =
        json!({"jsonrpc": "1.0", "id": "depinzcash-relay", "method": method, "params": params});
    let resp = client
        .post(url)
        .json(&body)
        .send()
        .await
        .with_context(|| format!("zebra rpc {method} send"))?;
    let status = resp.status();
    let text = resp.text().await?;
    if !status.is_success() {
        return Err(anyhow!("zebra rpc {method} returned {}: {}", status, text));
    }
    let v: serde_json::Value = serde_json::from_str(&text)
        .with_context(|| format!("parsing zebra rpc {method} response: {text}"))?;
    if let Some(err) = v.get("error").filter(|e| !e.is_null()) {
        return Err(anyhow!("zebra rpc {method} error: {}", err));
    }
    v.get("result")
        .cloned()
        .ok_or_else(|| anyhow!("zebra rpc {method} missing result"))
}

fn registration_message(
    wallet: &str,
    nonce: &str,
    timestamp: &str,
    kind: &str,
    network: &str,
    label: &str,
) -> Vec<u8> {
    format!("depinzcash:register:v1\n{wallet}\n{nonce}\n{timestamp}\n{kind}\n{network}\n{label}\n")
        .into_bytes()
}

fn proof_message(
    wallet: &str,
    node_id: &str,
    height: u64,
    block_hash: &str,
    proof_timestamp: &str,
    nonce: &str,
) -> Vec<u8> {
    format!(
        "depinzcash:proof:v1\n{wallet}\n{node_id}\n{height}\n{block_hash}\n{proof_timestamp}\n{nonce}\n"
    )
    .into_bytes()
}

fn sign_b58(sk: &SigningKey, msg: &[u8]) -> String {
    bs58::encode(sk.sign(msg).to_bytes()).into_string()
}

fn random_nonce() -> String {
    let mut bytes = [0u8; 24];
    rand::thread_rng().fill_bytes(&mut bytes);
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hex::encode(&hasher.finalize()[..16])
}
