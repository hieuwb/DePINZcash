use std::time::Duration;

use anyhow::{bail, Context};

#[derive(Clone, Debug)]
pub struct Config {
    pub bind_addr: String,
    pub database_url: String,
    pub trusted_rpcs: Vec<String>,
    pub rpc_timeout: Duration,
    pub admin_api_key: Option<String>,
    pub cors_allowed_origins: Vec<String>,
    pub scheduler_enabled: bool,
    pub heartbeat_interval: Duration,
    pub challenge_check_interval: Duration,
    pub uptime_reward_interval: Duration,
    pub snapshot_interval: Option<Duration>,
    pub max_height_drift: u64,
    pub max_clock_skew: Duration,
    // $ZePIN (SPL) reward mint — referenced by snapshot publisher and surfaced to clients.
    pub spl_mint: Option<String>,
    pub solana_cluster: String,
    pub network: ZcashNetwork,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ZcashNetwork {
    Mainnet,
    Testnet,
}

impl ZcashNetwork {
    pub fn as_str(&self) -> &'static str {
        match self {
            ZcashNetwork::Mainnet => "mainnet",
            ZcashNetwork::Testnet => "testnet",
        }
    }
}

impl Config {
    pub fn from_env() -> anyhow::Result<Self> {
        let bind_addr = std::env::var("BIND_ADDR").unwrap_or_else(|_| "0.0.0.0:3000".to_string());
        let database_url = std::env::var("DATABASE_URL").unwrap_or_else(|_| "sqlite://depinzcash.sqlite?mode=rwc".to_string());

        let trusted_rpcs = std::env::var("TRUSTED_RPCS")
            .unwrap_or_default()
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>();
        if trusted_rpcs.is_empty() {
            tracing::warn!("TRUSTED_RPCS is empty — proof verification will fall back to permissive mode");
        }

        let rpc_timeout = parse_duration("RPC_TIMEOUT", Duration::from_secs(10))?;

        let admin_api_key = std::env::var("ADMIN_API_KEY").ok().filter(|s| !s.is_empty());
        let cors_allowed_origins = std::env::var("CORS_ALLOWED_ORIGINS")
            .unwrap_or_default()
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>();

        let scheduler_enabled = !matches!(
            std::env::var("SCHEDULER_ENABLED")
                .unwrap_or_default()
                .to_lowercase()
                .as_str(),
            "false" | "0" | "no" | "off"
        );

        let heartbeat_interval = parse_duration("HEARTBEAT_INTERVAL", Duration::from_secs(60))?;
        let challenge_check_interval = parse_duration("CHALLENGE_CHECK_INTERVAL", Duration::from_secs(60))?;
        let uptime_reward_interval = parse_duration("UPTIME_REWARD_INTERVAL", Duration::from_secs(300))?;

        let snapshot_interval = match std::env::var("SNAPSHOT_INTERVAL").ok().as_deref() {
            None | Some("") => Some(Duration::from_secs(7 * 24 * 60 * 60)),
            Some("0" | "off" | "false" | "no" | "disabled") => None,
            Some(other) => Some(parse_duration_str(other)?),
        };

        let max_height_drift = std::env::var("MAX_HEIGHT_DRIFT")
            .ok()
            .map(|s| s.parse::<u64>())
            .transpose()
            .context("parsing MAX_HEIGHT_DRIFT")?
            .unwrap_or(8);
        let max_clock_skew = parse_duration("MAX_CLOCK_SKEW", Duration::from_secs(15 * 60))?;

        let spl_mint = std::env::var("SPL_MINT").ok().filter(|s| !s.is_empty());
        let solana_cluster = std::env::var("SOLANA_CLUSTER").unwrap_or_else(|_| "devnet".to_string());

        let network = match std::env::var("ZCASH_NETWORK").unwrap_or_else(|_| "mainnet".to_string()).to_lowercase().as_str() {
            "mainnet" => ZcashNetwork::Mainnet,
            "testnet" => ZcashNetwork::Testnet,
            other => bail!("unknown ZCASH_NETWORK: {}", other),
        };

        Ok(Self {
            bind_addr,
            database_url,
            trusted_rpcs,
            rpc_timeout,
            admin_api_key,
            cors_allowed_origins,
            scheduler_enabled,
            heartbeat_interval,
            challenge_check_interval,
            uptime_reward_interval,
            snapshot_interval,
            max_height_drift,
            max_clock_skew,
            spl_mint,
            solana_cluster,
            network,
        })
    }
}

fn parse_duration(var: &str, default: Duration) -> anyhow::Result<Duration> {
    match std::env::var(var) {
        Ok(s) if !s.is_empty() => parse_duration_str(&s).with_context(|| format!("parsing {}", var)),
        _ => Ok(default),
    }
}

// Accepts: "30s", "5m", "2h", "1d", or bare seconds "300".
fn parse_duration_str(s: &str) -> anyhow::Result<Duration> {
    let s = s.trim();
    if let Some(num) = s.strip_suffix("ms") {
        return Ok(Duration::from_millis(num.parse()?));
    }
    if let Some(num) = s.strip_suffix('s') {
        return Ok(Duration::from_secs(num.parse()?));
    }
    if let Some(num) = s.strip_suffix('m') {
        return Ok(Duration::from_secs(num.parse::<u64>()? * 60));
    }
    if let Some(num) = s.strip_suffix('h') {
        return Ok(Duration::from_secs(num.parse::<u64>()? * 3600));
    }
    if let Some(num) = s.strip_suffix('d') {
        return Ok(Duration::from_secs(num.parse::<u64>()? * 86_400));
    }
    Ok(Duration::from_secs(s.parse()?))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn duration_parsing() {
        assert_eq!(parse_duration_str("30s").unwrap(), Duration::from_secs(30));
        assert_eq!(parse_duration_str("5m").unwrap(), Duration::from_secs(300));
        assert_eq!(parse_duration_str("2h").unwrap(), Duration::from_secs(7200));
        assert_eq!(parse_duration_str("1d").unwrap(), Duration::from_secs(86_400));
        assert_eq!(parse_duration_str("500ms").unwrap(), Duration::from_millis(500));
        assert_eq!(parse_duration_str("42").unwrap(), Duration::from_secs(42));
    }
}
