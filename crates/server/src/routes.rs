//! HTTP + WebSocket routes for the terminal.
//!
//! Socket protocol (server -> client): `WireEvent` JSON frames; each socket
//! receives full `book` frames only for its selected market, plus `ticker`,
//! `trades`, `history`, `alert_set`, `alert_fired`, `arb_snapshot`,
//! `arb_update`, `arb_fill_event` and `status` for everything.
//!
//! Socket protocol (client -> server):
//! - `{"type":"select","market":"doge"}`  — switch the socket's market
//! - `{"type":"alert_create","market":"sol","dir":"above","price":"123.4"}`
//! - `{"type":"alert_delete","id":7}`
//! - `{"type":"arb_config",...}`  — partial engine config update
//! - `{"type":"arb_reset"}`       — reset paper-trading stats
//!
//! REST: `/api/health`, `/api/markets`, `/api/book?market=`, `/api/alerts`
//! (GET/POST), `/api/alerts/{id}` (DELETE), `/api/history?market=`,
//! `/api/tape?market=`, `/api/arb`, `/api/arb/config` (GET/PUT).

use crate::arb;
use crate::state::Registry;
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::{Html, IntoResponse, Response};
use axum::routing::{delete, get};
use axum::{Json, Router};
use ob_core::{AlertDir, ArbConfigUpdate, Market, WireEvent};
use serde::Deserialize;
use std::sync::Arc;
use std::time::Duration;
use tower_http::services::{ServeDir, ServeFile};

pub fn router(reg: Arc<Registry>) -> Router {
    let dist = ServeDir::new("dist").not_found_service(ServeFile::new("dist/index.html"));
    Router::new()
        .route("/ws", get(ws_handler))
        .route("/api/health", get(api_health))
        .route("/api/book", get(api_book))
        .route("/api/markets", get(api_markets))
        .route("/api/alerts", get(api_alerts).post(api_alert_create))
        .route("/api/alerts/{id}", delete(api_alert_delete))
        .route("/api/history", get(api_history))
        .route("/api/tape", get(api_tape))
        .route("/api/arb", get(api_arb))
        .route("/api/arb/config", get(api_arb_config_get).put(api_arb_config_put))
        .route("/api/cycles", get(api_cycles))
        .route("/api/ga", get(api_ga).put(api_ga_put))
        .fallback_service(dist)
        .with_state(reg)
}

/// GET /ws — live orderbook stream.
async fn ws_handler(State(reg): State<Arc<Registry>>, ws: WebSocketUpgrade) -> Response {
    ws.on_upgrade(move |socket| handle_socket(reg, socket))
}

/// Messages a browser may send us over the socket.
#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum ClientMsg {
    Select { market: Market },
    AlertCreate {
        market: Market,
        dir: AlertDir,
        price: String,
    },
    AlertDelete { id: u64 },
    ArbConfig(ArbConfigUpdate),
    ArbReset,
    GaToggle { enabled: bool },
    GaApply,
    GaReset,
}

async fn handle_socket(reg: Arc<Registry>, mut socket: WebSocket) {
    // Every socket starts on ETH; it re-selects via client messages.
    let mut sel = Market::Eth;
    reg.select_market(sel);
    tracing::debug!("[ws] socket connected, default selection {}", sel.label());

    // --- initial burst -------------------------------------------------------
    // 1) tickers for all markets (so the selector paints immediately)
    for m in Market::ALL {
        if let Some(json) = reg.ticker_json(m) {
            if socket.send(Message::Text(json.into())).await.is_err() {
                reg.deselect_market(sel);
                return;
            }
        }
    }
    // 2) full book for the selected market
    if let Some(json) = reg.book_json(sel) {
        if socket.send(Message::Text(json.into())).await.is_err() {
            reg.deselect_market(sel);
            return;
        }
    }
    // 3) trade tape backlog
    let ev = WireEvent::Trades {
        market: sel,
        trades: reg.tape_backlog(sel),
    };
    if let Ok(json) = serde_json::to_string(&ev) {
        if socket.send(Message::Text(json.into())).await.is_err() {
            reg.deselect_market(sel);
            return;
        }
    }
    // 4) depth history window
    let samples = reg.history_snapshot(sel);
    let mut last_hist_t = samples.last().map(|s| s.t).unwrap_or(0);
    let ev = WireEvent::History {
        market: sel,
        samples,
    };
    if let Ok(json) = serde_json::to_string(&ev) {
        if socket.send(Message::Text(json.into())).await.is_err() {
            reg.deselect_market(sel);
            return;
        }
    }
    // 5) alert list
    let ev = WireEvent::AlertSet {
        alerts: reg.alerts_snapshot(),
    };
    if let Ok(json) = serde_json::to_string(&ev) {
        if socket.send(Message::Text(json.into())).await.is_err() {
            reg.deselect_market(sel);
            return;
        }
    }
    // 6) arbitrage engine state
    if let Ok(json) = serde_json::to_string(&arb::snapshot(&reg)) {
        if socket.send(Message::Text(json.into())).await.is_err() {
            reg.deselect_market(sel);
            return;
        }
    }
    // 7) multi-hop cycle engine state
    if let Ok(json) = serde_json::to_string(&crate::cycles::snapshot(&reg)) {
        if socket.send(Message::Text(json.into())).await.is_err() {
            reg.deselect_market(sel);
            return;
        }
    }
    // 8) genetic-algorithm optimizer state
    if let Ok(json) = serde_json::to_string(&WireEvent::GaUpdate {
        state: crate::ga::snapshot(&reg),
    }) {
        if socket.send(Message::Text(json.into())).await.is_err() {
            reg.deselect_market(sel);
            return;
        }
    }

    let mut rx = reg.subscribe();
    let mut hist_tick = tokio::time::interval(Duration::from_secs(ob_core::SAMPLE_SECS));
    hist_tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    // First tick fires immediately; skip it so we don't resend the window.
    hist_tick.tick().await;

    loop {
        let outgoing = tokio::select! {
            // History appends for the selected market.
            _ = hist_tick.tick() => {
                let samples = reg.history_after(sel, last_hist_t);
                if samples.is_empty() {
                    continue;
                }
                last_hist_t = samples.last().map(|s| s.t).unwrap_or(last_hist_t);
                let ev = WireEvent::History { market: sel, samples };
                match serde_json::to_string(&ev) {
                    Ok(json) => Some(json),
                    Err(_) => continue,
                }
            }
            m = rx.recv() => match m {
                Ok(bc) => {
                    if bc.kind.wants(sel, bc.market) {
                        Some(bc.json)
                    } else {
                        continue;
                    }
                }
                Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                    tracing::debug!("client lagged behind by {n} events");
                    continue;
                }
                Err(_) => break, // registry gone: shutdown
            },
            incoming = socket.recv() => {
                match incoming {
                    Some(Ok(Message::Text(t))) => {
                        let msg: ClientMsg = match serde_json::from_str(&t) {
                            Ok(m) => m,
                            Err(e) => {
                                tracing::debug!("[ws] unparsable client message ({e}): {t}");
                                continue;
                            }
                        };
                        match msg {
                            ClientMsg::Select { market } => {
                                if market == sel {
                                    continue;
                                }
                                reg.deselect_market(sel);
                                sel = market;
                                reg.select_market(sel);
                                tracing::debug!("[ws] socket switched to {}", sel.label());
                                // Fresh state for the new market.
                                if let Some(json) = reg.book_json(sel) {
                                    if socket.send(Message::Text(json.into())).await.is_err() {
                                        break;
                                    }
                                }
                                let ev = WireEvent::Trades {
                                    market: sel,
                                    trades: reg.tape_backlog(sel),
                                };
                                if let Ok(json) = serde_json::to_string(&ev) {
                                    if socket.send(Message::Text(json.into())).await.is_err() {
                                        break;
                                    }
                                }
                                let samples = reg.history_snapshot(sel);
                                last_hist_t = samples.last().map(|s| s.t).unwrap_or(0);
                                let ev = WireEvent::History { market: sel, samples };
                                if let Ok(json) = serde_json::to_string(&ev) {
                                    if socket.send(Message::Text(json.into())).await.is_err() {
                                        break;
                                    }
                                }
                            }
                            ClientMsg::AlertCreate { market, dir, price } => {
                                if let Err(e) = reg.create_alert(market, dir, price) {
                                    tracing::debug!("[ws] alert create rejected: {e}");
                                }
                            }
                            ClientMsg::AlertDelete { id } => {
                                reg.delete_alert(id);
                            }
                            ClientMsg::ArbConfig(upd) => {
                                if let Err(e) = arb::set_config(&reg, upd) {
                                    tracing::debug!("[ws] arb config rejected: {e}");
                                }
                            }
                            ClientMsg::ArbReset => {
                                arb::reset(&reg);
                                crate::cycles::reset(&reg);
                                tracing::info!("[ws] arb + cycle stats reset");
                            }
                            ClientMsg::GaToggle { enabled } => {
                                reg.ga.set_enabled(enabled);
                                crate::ga::broadcast_update(&reg);
                                tracing::info!("[ws] ga enabled={enabled}");
                            }
                            ClientMsg::GaApply => {
                                crate::ga::apply_now(&reg);
                                crate::ga::broadcast_update(&reg);
                            }
                            ClientMsg::GaReset => {
                                crate::ga::reset(&reg);
                                tracing::info!("[ws] ga evolution reset");
                            }
                        }
                        continue;
                    }
                    Some(Ok(Message::Close(_))) | None => break,
                    Some(Ok(_)) => continue, // ignore other client frames
                    Some(Err(_)) => break,
                }
            }
        };
        let Some(json) = outgoing else { continue };
        if socket.send(Message::Text(json.into())).await.is_err() {
            break;
        }
    }

    reg.deselect_market(sel);
    tracing::debug!("[ws] socket closed (was on {})", sel.label());
}

/// GET /api/health — feed status for both venues.
async fn api_health(State(reg): State<Arc<Registry>>) -> impl IntoResponse {
    Json(reg.health_payload())
}

/// GET /api/markets — static market catalogue (live-verified mappings).
async fn api_markets() -> impl IntoResponse {
    Json(serde_json::json!({
        "markets": Market::ALL.iter().map(|m| serde_json::json!({
            "id": serde_json::to_value(m).unwrap_or_default(),
            "label": m.label(),
            "hyperliquid_coin": m.hyperliquid_coin(),
            "lighter_index": m.lighter_market_index(),
        })).collect::<Vec<_>>()
    }))
}

fn market_from_query(s: Option<&str>) -> Result<Market, Response> {
    match s {
        None => Ok(Market::Eth),
        Some(slug) => Market::from_slug(slug).ok_or_else(|| {
            let names: Vec<&str> = Market::ALL.iter().map(|m| m.label()).collect();
            (
                StatusCode::BAD_REQUEST,
                Html(format!(
                    "unknown market {slug:?}; labels: {}",
                    names.join(", ")
                )),
            )
                .into_response()
        }),
    }
}

#[derive(Deserialize)]
struct BookQuery {
    market: Option<String>,
}

/// GET /api/book?market=doge — current snapshot (defaults to eth).
async fn api_book(
    State(reg): State<Arc<Registry>>,
    Query(q): Query<BookQuery>,
) -> Response {
    let market = match market_from_query(q.market.as_deref()) {
        Ok(m) => m,
        Err(resp) => return resp,
    };
    match reg.book_json(market) {
        Some(json) => (StatusCode::OK, Html(json)).into_response(),
        None => (StatusCode::SERVICE_UNAVAILABLE, Html("no data yet")).into_response(),
    }
}

#[derive(Deserialize)]
struct HistoryQuery {
    market: Option<String>,
}

/// GET /api/history?market=sol — depth samples for the last hour.
async fn api_history(
    State(reg): State<Arc<Registry>>,
    Query(q): Query<HistoryQuery>,
) -> Response {
    let market = match market_from_query(q.market.as_deref()) {
        Ok(m) => m,
        Err(resp) => return resp,
    };
    Json(serde_json::json!({
        "market": market.label(),
        "samples": reg.history_snapshot(market),
    }))
    .into_response()
}

/// GET /api/tape?market=sol — recent trades (oldest first).
async fn api_tape(State(reg): State<Arc<Registry>>, Query(q): Query<HistoryQuery>) -> Response {
    let market = match market_from_query(q.market.as_deref()) {
        Ok(m) => m,
        Err(resp) => return resp,
    };
    Json(serde_json::json!({
        "market": market.label(),
        "trades": reg.tape_backlog(market),
    }))
    .into_response()
}

/// GET /api/alerts — list alerts.
async fn api_alerts(State(reg): State<Arc<Registry>>) -> impl IntoResponse {
    Json(serde_json::json!({ "alerts": reg.alerts_snapshot() }))
}

#[derive(Deserialize)]
struct AlertCreateBody {
    market: Market,
    dir: AlertDir,
    price: String,
}

/// POST /api/alerts — create an alert.
async fn api_alert_create(
    State(reg): State<Arc<Registry>>,
    Json(body): Json<AlertCreateBody>,
) -> Response {
    match reg.create_alert(body.market, body.dir, body.price) {
        Ok(alert) => (StatusCode::CREATED, Json(alert)).into_response(),
        Err(e) => (StatusCode::BAD_REQUEST, Html(e)).into_response(),
    }
}

/// DELETE /api/alerts/{id} — remove an alert.
async fn api_alert_delete(State(reg): State<Arc<Registry>>, Path(id): Path<u64>) -> Response {
    if reg.delete_alert(id) {
        (StatusCode::OK, Html("deleted")).into_response()
    } else {
        (StatusCode::NOT_FOUND, Html("no such alert")).into_response()
    }
}

/// GET /api/arb — full arbitrage engine state.
async fn api_arb(State(reg): State<Arc<Registry>>) -> impl IntoResponse {
    Json(arb::snapshot(&reg))
}

/// GET /api/arb/config — engine configuration.
async fn api_arb_config_get(State(reg): State<Arc<Registry>>) -> impl IntoResponse {
    match arb::snapshot(&reg) {
        WireEvent::ArbSnapshot { config, .. } => Json(config),
        _ => Json(ob_core::ArbConfig::default()),
    }
}

/// PUT /api/arb/config — partial engine configuration update.
async fn api_arb_config_put(
    State(reg): State<Arc<Registry>>,
    Json(body): Json<ArbConfigUpdate>,
) -> Response {
    match arb::set_config(&reg, body) {
        Ok(()) => match arb::snapshot(&reg) {
            WireEvent::ArbSnapshot { config, .. } => (StatusCode::OK, Json(config)).into_response(),
            _ => (StatusCode::OK, Json(ob_core::ArbConfig::default())).into_response(),
        },
        Err(e) => (StatusCode::BAD_REQUEST, Html(e)).into_response(),
    }
}

/// GET /api/cycles — multi-hop cycle engine state.
async fn api_cycles(State(reg): State<Arc<Registry>>) -> impl IntoResponse {
    Json(crate::cycles::snapshot(&reg))
}

/// GET /api/ga — genetic-algorithm optimizer state.
async fn api_ga(State(reg): State<Arc<Registry>>) -> impl IntoResponse {
    Json(serde_json::json!({ "state": crate::ga::snapshot(&reg) }))
}

#[derive(Deserialize)]
struct GaPutBody {
    enabled: Option<bool>,
    apply: Option<bool>,
    reset: Option<bool>,
}

/// PUT /api/ga — toggle / apply / reset the optimizer.
async fn api_ga_put(State(reg): State<Arc<Registry>>, Json(body): Json<GaPutBody>) -> impl IntoResponse {
    if let Some(on) = body.enabled {
        reg.ga.set_enabled(on);
    }
    if body.apply.unwrap_or(false) {
        crate::ga::apply_now(&reg);
    }
    if body.reset.unwrap_or(false) {
        crate::ga::reset(&reg);
    }
    crate::ga::broadcast_update(&reg);
    Json(serde_json::json!({ "state": crate::ga::snapshot(&reg) }))
}
