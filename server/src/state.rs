use std::sync::Arc;

use tokio::sync::Mutex;

use crate::{config::Config, rpc::ZcashRpcQuorum, store::SqliteStore};

#[derive(Clone)]
pub struct AppState {
    inner: Arc<AppStateInner>,
}

struct AppStateInner {
    pub config: Config,
    pub store: SqliteStore,
    pub rpc: ZcashRpcQuorum,
    // Cached trusted tip height (refreshed by the scheduler). None until first scheduler tick.
    pub trusted_tip: Mutex<Option<u64>>,
}

impl AppState {
    pub fn new(config: Config, store: SqliteStore, rpc: ZcashRpcQuorum) -> Self {
        Self {
            inner: Arc::new(AppStateInner {
                config,
                store,
                rpc,
                trusted_tip: Mutex::new(None),
            }),
        }
    }

    pub fn config(&self) -> &Config {
        &self.inner.config
    }

    pub fn store(&self) -> &SqliteStore {
        &self.inner.store
    }

    pub fn rpc(&self) -> &ZcashRpcQuorum {
        &self.inner.rpc
    }

    pub async fn trusted_tip(&self) -> Option<u64> {
        *self.inner.trusted_tip.lock().await
    }

    pub async fn set_trusted_tip(&self, height: u64) {
        *self.inner.trusted_tip.lock().await = Some(height);
    }
}
