use std::{collections::HashMap, time::Duration};

use anyhow::{anyhow, Context};
use futures::future::join_all;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use url::Url;

#[derive(Debug, thiserror::Error)]
pub enum RpcError {
    #[error("no trusted rpcs configured")]
    NoEndpoints,
    #[error("all rpcs failed")]
    AllFailed,
    #[error("no quorum")]
    NoQuorum,
    #[error("{0}")]
    Other(String),
}

#[derive(Debug, Serialize)]
struct JsonRpcRequest<'a> {
    jsonrpc: &'static str,
    id: u32,
    method: &'a str,
    params: Value,
}

#[derive(Debug, Deserialize)]
struct JsonRpcResponse {
    #[serde(default)]
    result: Option<Value>,
    #[serde(default)]
    error: Option<JsonRpcError>,
}

#[derive(Debug, Deserialize)]
struct JsonRpcError {
    code: i64,
    message: String,
}

#[derive(Clone)]
pub struct ZcashRpcQuorum {
    endpoints: Vec<String>,
    client: Client,
}

impl ZcashRpcQuorum {
    pub fn new(endpoints: Vec<String>, timeout: Duration) -> Self {
        let client = Client::builder()
            .timeout(timeout)
            .pool_idle_timeout(Duration::from_secs(60))
            .build()
            .expect("building reqwest client");
        Self { endpoints, client }
    }

    pub fn is_configured(&self) -> bool {
        !self.endpoints.is_empty()
    }

    pub fn endpoints(&self) -> &[String] {
        &self.endpoints
    }

    pub async fn call_single(&self, endpoint: &str, method: &str, params: Value) -> anyhow::Result<Value> {
        let url = Url::parse(endpoint).with_context(|| format!("parsing rpc url: {endpoint}"))?;
        let req = JsonRpcRequest {
            jsonrpc: "2.0",
            id: 1,
            method,
            params,
        };
        let mut builder = self.client.post(url).json(&req);
        if let Some(auth) = parse_basic_auth(endpoint) {
            builder = builder.basic_auth(auth.0, Some(auth.1));
        }
        let resp = builder.send().await.context("rpc send")?;
        let status = resp.status();
        let body = resp.text().await.context("rpc body")?;
        if !status.is_success() {
            return Err(anyhow!("rpc {} returned {}: {}", endpoint, status, body));
        }
        let parsed: JsonRpcResponse = serde_json::from_str(&body)
            .with_context(|| format!("parsing rpc response from {endpoint}: {body}"))?;
        if let Some(err) = parsed.error {
            return Err(anyhow!("rpc {} error {}: {}", endpoint, err.code, err.message));
        }
        parsed.result.ok_or_else(|| anyhow!("rpc {} missing result", endpoint))
    }

    pub async fn quorum_call(&self, method: &str, params: Value) -> Result<Value, RpcError> {
        if self.endpoints.is_empty() {
            return Err(RpcError::NoEndpoints);
        }
        let futs = self.endpoints.iter().map(|ep| {
            let m = method.to_string();
            let p = params.clone();
            async move {
                let res = self.call_single(ep, &m, p).await;
                (ep.clone(), res)
            }
        });
        let results = join_all(futs).await;
        let mut tally: HashMap<String, (Value, u32)> = HashMap::new();
        let mut errors = 0u32;
        for (ep, res) in results {
            match res {
                Ok(v) => {
                    let key = canonicalize(&v);
                    tally
                        .entry(key)
                        .and_modify(|e| e.1 += 1)
                        .or_insert((v.clone(), 1));
                }
                Err(e) => {
                    errors += 1;
                    tracing::warn!(endpoint = %ep, error = ?e, "rpc endpoint failed");
                }
            }
        }
        if tally.is_empty() {
            return if errors == self.endpoints.len() as u32 {
                Err(RpcError::AllFailed)
            } else {
                Err(RpcError::Other("no successful responses".into()))
            };
        }
        let total = self.endpoints.len() as u32;
        let majority_needed = total / 2 + 1;
        let (best_val, best_count) = tally
            .into_values()
            .max_by_key(|(_, c)| *c)
            .ok_or(RpcError::NoQuorum)?;
        if best_count < majority_needed {
            return Err(RpcError::NoQuorum);
        }
        Ok(best_val)
    }

    pub async fn get_block_count(&self) -> Result<u64, RpcError> {
        let v = self.quorum_call("getblockcount", json!([])).await?;
        v.as_u64().ok_or_else(|| RpcError::Other(format!("expected u64, got {v}")))
    }

    pub async fn get_block_hash(&self, height: u64) -> Result<String, RpcError> {
        let v = self.quorum_call("getblockhash", json!([height])).await?;
        v.as_str()
            .map(|s| s.to_string())
            .ok_or_else(|| RpcError::Other(format!("expected string, got {v}")))
    }

    pub async fn get_best_block_hash(&self) -> Result<String, RpcError> {
        let v = self.quorum_call("getbestblockhash", json!([])).await?;
        v.as_str()
            .map(|s| s.to_string())
            .ok_or_else(|| RpcError::Other(format!("expected string, got {v}")))
    }
}

fn canonicalize(v: &Value) -> String {
    serde_json::to_string(v).unwrap_or_else(|_| String::new())
}

// If an endpoint embeds basic-auth credentials (http://user:pass@host), pull them out.
fn parse_basic_auth(endpoint: &str) -> Option<(String, String)> {
    let url = Url::parse(endpoint).ok()?;
    let user = url.username();
    let pass = url.password()?;
    if user.is_empty() {
        return None;
    }
    Some((user.to_string(), pass.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn empty_quorum_fails_fast() {
        let q = ZcashRpcQuorum::new(vec![], Duration::from_secs(1));
        let res = q.get_block_count().await;
        assert!(matches!(res, Err(RpcError::NoEndpoints)));
    }
}
