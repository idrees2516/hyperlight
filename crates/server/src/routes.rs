//! HTTP + WebSocket routes for the terminal.

use crate::state::Registry;
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::response::{Html, IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router};
use ob_core::Market;
use serde::Deserialize;
use std::sync::Arc;
use tower_http::services::{ServeDir, ServeFile};

pub fn router(reg: Arc<Registry>) -> Router {
    let dist = ServeDir::new("dist").not_found_service(ServeFile::new("dist/index.html"));
    Router::new()
        .route("/ws", get(ws_handler))
        .route("/api/health", get(api_health))
        .route("/api/book", get(api_book))
        .route("/api/markets", get(api_markets))
        .fallback_service(dist)
        .with_state(reg)
}

/// GET /ws — live orderbook stream.
async fn ws_handler(State(reg): State<Arc<Registry>>, ws: WebSocketUpgrade) -> Response {
    ws.on_upgrade(move |socket| handle_socket(reg, socket))
}

async fn handle_socket(reg: Arc<Registry>, mut socket: WebSocket) {
    // Initial state so a fresh tab paints immediately.
    for market in Market::ALL {
        if let Some(json) = reg.book_json(market) {
            if socket
                .send(Message::Text(json.into()))
                .await
                .is_err()
            {
                return;
            }
        }
    }

    let mut rx = reg.subscribe();
    loop {
        let outgoing = tokio::select! {
            m = rx.recv() => match m {
                Ok(json) => Some(json),
                Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                    tracing::debug!("client lagged behind by {n} events");
                    continue;
                }
                Err(_) => break, // registry gone: shutdown
            },
            incoming = socket.recv() => {
                match incoming {
                    Some(Ok(Message::Close(_))) | None => break,
                    Some(Ok(_)) => continue, // ignore client frames
                    Some(Err(_)) => break,
                }
            }
        };
        let Some(json) = outgoing else { continue };
        if socket.send(Message::Text(json.into())).await.is_err() {
            break;
        }
    }
    let _ = &socket;
}

/// GET /api/health — feed status for both venues.
async fn api_health(State(reg): State<Arc<Registry>>) -> impl IntoResponse {
    Json(reg.health_payload())
}

/// GET /api/markets — static market catalogue.
async fn api_markets() -> impl IntoResponse {
    Json(serde_json::json!({
        "markets": [
            { "id": "eth", "label": "ETH-USD", "hyperliquid_coin": "ETH", "lighter_index": 0 },
            { "id": "btc", "label": "BTC-USD", "hyperliquid_coin": "BTC", "lighter_index": 1 },
            { "id": "sol", "label": "SOL-USD", "hyperliquid_coin": "SOL", "lighter_index": 2 },
        ]
    }))
}

#[derive(Deserialize)]
struct BookQuery {
    market: Option<String>,
}

/// GET /api/book?market=eth — current snapshot (defaults to eth).
async fn api_book(
    State(reg): State<Arc<Registry>>,
    Query(q): Query<BookQuery>,
) -> Response {
    let market = match q.market.as_deref() {
        None | Some("eth") => Market::Eth,
        Some("btc") => Market::Btc,
        Some("sol") => Market::Sol,
        _ => {
            return (
                StatusCode::BAD_REQUEST,
                Html("supported markets: eth, btc, sol"),
            )
                .into_response()
        }
    };
    match reg.book_json(market) {
        Some(json) => (StatusCode::OK, Html(json)).into_response(),
        None => (StatusCode::SERVICE_UNAVAILABLE, Html("no data yet")).into_response(),
    }
}
