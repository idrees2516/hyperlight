//! HyperLight — a real-time cross-venue orderbook terminal and arbitrage
//! engine.
//!
//! Backend binary: connects to Hyperliquid + Lighter and the seven major
//! ETH/SOL CLOBs (Binance, Bybit, OKX, Kraken, Coinbase, Bitstamp, Gate),
//! maintains normalized full-depth orderbooks, runs four arbitrage engines
//! (2-leg depth walk, global sweep optimizer, multi-hop swap cycles, and a
//! genetic-algorithm parameter optimizer), and streams coalesced snapshots
//! to browser clients over a single WebSocket.
//!
//! ## Runtime configuration (environment variables)
//!
//! - `PORT`                     — HTTP/WS listen port (default 3000)
//! - `HYPAR_DISARMED=1`         — boot with the paper executor disarmed
//! - `HYPAR_MIN_EDGE_BPS`       — listing threshold, bps
//! - `HYPAR_FIRE_EDGE_BPS`      — firing threshold, bps
//! - `HYPAR_MAX_NOTIONAL_USD`   — capital cap per fill
//! - `HYPAR_LATENCY_MS`         — simulated round-trip latency
//! - `HYPAR_COOLDOWN_MS`        — per-route cooldown
//!
//! The engine boots **armed** by default (paper execution fires
//! automatically on every positive-EV opportunity) unless disarmed above.

mod arb;
mod binance;
mod bitstamp;
mod bybit;
mod coinbase;
mod cycles;
mod gate;
mod ga;
mod hyperliquid;
mod kraken;
mod lighter;
mod okx;
mod routes;
mod state;
mod sweep;
mod wsio;

use ob_core::ArbConfigUpdate;
use std::sync::Arc;

fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .with_target(false)
        .init();

    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("failed to build tokio runtime");

    runtime.block_on(async_main());
}

/// Apply `HYPAR_*` environment overrides to the engine configuration at
/// boot (before any connector starts) so deployment platforms can tune the
/// engine without rebuilding.
fn env_config(reg: &state::Registry) {
    let upd = ArbConfigUpdate {
        enabled: match std::env::var("HYPAR_DISARMED").as_deref() {
            Ok("1") | Ok("true") => Some(false),
            _ => None, // keep the armed-by-default config
        },
        min_edge_bps: std::env::var("HYPAR_MIN_EDGE_BPS").ok(),
        fire_edge_bps: std::env::var("HYPAR_FIRE_EDGE_BPS").ok(),
        max_notional_usd: std::env::var("HYPAR_MAX_NOTIONAL_USD").ok(),
        latency_ms: std::env::var("HYPAR_LATENCY_MS")
            .ok()
            .and_then(|v| v.parse().ok()),
        cooldown_ms: std::env::var("HYPAR_COOLDOWN_MS")
            .ok()
            .and_then(|v| v.parse().ok()),
        fees_bps: None,
    };
    let noop = upd.enabled.is_none()
        && upd.min_edge_bps.is_none()
        && upd.fire_edge_bps.is_none()
        && upd.max_notional_usd.is_none()
        && upd.latency_ms.is_none()
        && upd.cooldown_ms.is_none()
        && upd.fees_bps.is_none();
    if !noop {
        if let Err(e) = arb::set_config(reg, upd) {
            tracing::warn!("[boot] env config rejected: {e}");
        } else {
            tracing::info!("[boot] engine configured from HYPAR_* environment");
        }
    }
}

async fn async_main() {
    let port: u16 = std::env::var("PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(3000);

    let reg = state::Registry::new();

    // Engine tuning from the environment (before connectors start).
    env_config(&reg);

    // Venue feed connectors (each reconnects internally).
    tokio::spawn(hyperliquid::run(reg.clone()));
    tokio::spawn(lighter::run(reg.clone()));
    tokio::spawn(binance::run(reg.clone()));
    tokio::spawn(bybit::run(reg.clone()));
    tokio::spawn(okx::run(reg.clone()));
    tokio::spawn(kraken::run(reg.clone()));
    tokio::spawn(coinbase::run(reg.clone()));
    tokio::spawn(bitstamp::run(reg.clone()));
    tokio::spawn(gate::run(reg.clone()));

    // Coalescing publisher (10 Hz max per market) + depth sampler (5 s).
    tokio::spawn(reg.clone().publish_loop());
    tokio::spawn(reg.clone().history_loop());

    // Genetic-algorithm optimizer (one generation every 30 s).
    tokio::spawn(ga::run(reg.clone()));

    let app = routes::router(reg.clone());
    let listener = tokio::net::TcpListener::bind(("0.0.0.0", port))
        .await
        .expect("failed to bind");
    tracing::info!("HyperLight backend listening on http://0.0.0.0:{port}");
    tracing::info!(
        "9 venues live · 4 arbitrage engines · serving static frontend from ./dist"
    );

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await
        .expect("server error");

    let _ = Arc::downgrade(&reg);
}

/// Graceful shutdown on SIGINT (Ctrl-C) **and** SIGTERM (container
/// orchestrators send SIGTERM; without this handler in-flight WebSocket
/// writes would be dropped).
async fn shutdown_signal() {
    let ctrl_c = async {
        let _ = tokio::signal::ctrl_c().await;
    };
    #[cfg(unix)]
    let terminate = async {
        use tokio::signal::unix::{signal, SignalKind};
        match signal(SignalKind::terminate()) {
            Ok(mut sig) => {
                sig.recv().await;
            }
            Err(_) => std::future::pending::<()>().await,
        }
    };
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }
    tracing::info!("shutdown signal received");
}
