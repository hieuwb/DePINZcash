use std::net::SocketAddr;

use anyhow::Context;
use depinzcash_server::{api, config::Config, rpc::ZcashRpcQuorum, scheduler, state::AppState, store::SqliteStore};
use tracing_subscriber::{prelude::*, EnvFilter};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenvy::dotenv().ok();
    init_tracing();

    let config = Config::from_env().context("loading config from env")?;
    tracing::info!(?config.bind_addr, db = %config.database_url, "starting depinzcash-server");

    let store = SqliteStore::connect(&config.database_url)
        .await
        .context("connecting to sqlite store")?;
    store.migrate().await.context("running migrations")?;

    let quorum = ZcashRpcQuorum::new(config.trusted_rpcs.clone(), config.rpc_timeout);

    let state = AppState::new(config.clone(), store, quorum);

    if config.scheduler_enabled {
        scheduler::spawn(state.clone());
    } else {
        tracing::warn!("scheduler disabled via config");
    }

    let app = api::router(state.clone());

    let addr: SocketAddr = config.bind_addr.parse().context("parsing bind address")?;
    let listener = tokio::net::TcpListener::bind(addr).await.context("binding listener")?;
    tracing::info!(%addr, "depinzcash-server listening");

    // into_make_service_with_connect_info wires ConnectInfo<SocketAddr> into each
    // request's extensions — required by the per-IP rate limiter.
    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .await
    .context("serving")?;
    Ok(())
}

fn init_tracing() {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info,sqlx=warn,hyper=warn"));
    let json = std::env::var("LOG_FORMAT").ok().as_deref() == Some("json");
    let registry = tracing_subscriber::registry().with(filter);
    if json {
        registry.with(tracing_subscriber::fmt::layer().json()).init();
    } else {
        registry.with(tracing_subscriber::fmt::layer()).init();
    }
}
