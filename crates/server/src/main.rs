//! HyperLight — a real-time cross-venue orderbook terminal.
//!
//! Backend binary: connects to Hyperliquid + Lighter websocket market data
//! feeds, maintains normalized full-depth orderbooks, and streams coalesced
//! snapshots to browser clients over a single WebSocket.

mod hyperliquid;
mod lighter;
mod routes;
mod state;

use std::sync::Arc;
use tracing_subscriber::EnvFilter;

fn main() {
    // rustls needs an explicit process-level crypto provider.
    let _ = rustls::crypto::ring::default_provider().install_default();

    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .with_target(false)
        .init();

    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("failed to build tokio runtime");

    runtime.block_on(async_main());
}

async fn async_main() {
    let port: u16 = std::env::var("PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(3000);

    let reg = state::Registry::new();

    // Venue feed connectors (each reconnects internally).
    tokio::spawn(hyperliquid::run(reg.clone()));
    tokio::spawn(lighter::run(reg.clone()));

    // Coalescing publisher (10 Hz max per market) + depth sampler (5 s).
    tokio::spawn(reg.clone().publish_loop());
    tokio::spawn(reg.clone().history_loop());

    let app = routes::router(reg.clone());
    let listener = tokio::net::TcpListener::bind(("0.0.0.0", port))
        .await
        .expect("failed to bind");
    tracing::info!("HyperLight backend listening on http://0.0.0.0:{port}");
    tracing::info!("serving static frontend from ./dist");

    axum::serve(listener, app)
        .with_graceful_shutdown(async {
            let _ = tokio::signal::ctrl_c().await;
            tracing::info!("shutdown signal received");
        })
        .await
        .expect("server error");

    let _ = Arc::downgrade(&reg);
}
