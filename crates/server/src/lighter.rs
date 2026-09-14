//! Lighter (zkLighter) websocket connector.
//!
//! Protocol (source: official elliottech/lighter-python SDK + live-verified
//! against wss://mainnet.zklighter.elliot.ai/stream):
//! - on connect the server sends `{"session_id":..,"type":"connected"}`; only
//!   then may the client subscribe.
//! - subscribe:     `{"type":"subscribe","channel":"order_book/{market_index}"}`
//! - first push:     `{"type":"subscribed/order_book","channel":"order_book:{idx}","order_book":{"bids":[..],"asks":[..]}}`
//!                   — FULL snapshot (mainnet books are ~800-1700 levels/side).
//! - subsequent:     `{"type":"update/order_book",...}` — INCREMENTAL deltas
//!                   batched every ~50ms; upsert by price, size 0 removes.
//! - heartbeat:      server sends `{"type":"ping"}` -> reply `{"type":"pong"}`.

use crate::state::Registry;
use futures_util::{SinkExt, StreamExt};
use ob_core::{FeedStatus, Market, Side};
use rust_decimal::Decimal;
use std::sync::Arc;
use std::time::Duration;
use tokio::time::{interval_at, Instant};
use tokio_tungstenite::tungstenite::Message;

pub const LT_WS_URL: &str = "wss://mainnet.zklighter.elliot.ai/stream";

#[derive(serde::Deserialize)]
struct LtLevel {
    price: String,
    size: String,
}

#[derive(serde::Deserialize)]
struct LtOrderBook {
    #[serde(default)]
    bids: Vec<LtLevel>,
    #[serde(default)]
    asks: Vec<LtLevel>,
}

#[derive(serde::Deserialize)]
struct LtMsg {
    #[serde(rename = "type")]
    kind: String,
    #[serde(default)]
    channel: Option<String>,
    #[serde(default)]
    order_book: Option<LtOrderBook>,
}

fn market_for_channel(channel: &str) -> Option<Market> {
    // channels arrive as "order_book:{index}"
    let idx: u16 = channel.rsplit(':').next()?.parse().ok()?;
    match idx {
        0 => Some(Market::Eth),
        1 => Some(Market::Btc),
        2 => Some(Market::Sol),
        _ => None,
    }
}

fn parse_levels(raw: &[LtLevel]) -> Vec<(Decimal, Decimal, Option<u32>)> {
    raw.iter()
        .filter_map(|l| {
            let px: Decimal = l.price.parse().ok()?;
            let sz: Decimal = l.size.parse().ok()?;
            Some((px, sz, None))
        })
        .collect()
}

pub async fn run(reg: Arc<Registry>) {
    let mut backoff = 1u64;
    loop {
        reg.lt.set_status(FeedStatus::Connecting);
        match connect_once(&reg).await {
            Ok(reason) => {
                tracing::warn!("[lighter] stream ended ({reason}); reconnecting in {backoff}s");
            }
            Err(e) => {
                tracing::warn!("[lighter] error: {e}; reconnecting in {backoff}s");
            }
        }
        reg.lt.set_status(FeedStatus::Reconnecting);
        tokio::time::sleep(Duration::from_secs(backoff)).await;
        backoff = (backoff * 2).min(30);
    }
}

async fn connect_once(reg: &Arc<Registry>) -> Result<String, String> {
    let (ws, _resp) = tokio_tungstenite::connect_async(LT_WS_URL)
        .await
        .map_err(|e| format!("connect failed: {e}"))?;
    tracing::info!("[lighter] connected to {LT_WS_URL}");
    let (mut sink, mut stream) = ws.split();

    let mut subscribed = false;
    let mut got_snapshot = false;
    // Watchdog: if nothing arrives for 30s something is wrong upstream of TCP
    // (CloudFront idle drop etc.) — force a reconnect.
    let mut watchdog = interval_at(Instant::now() + Duration::from_secs(30), Duration::from_secs(30));
    let mut saw_msg_at = std::time::Instant::now();

    loop {
        tokio::select! {
            _ = watchdog.tick() => {
                if saw_msg_at.elapsed() > Duration::from_secs(30) {
                    return Err("30s without data (watchdog)".into());
                }
            }
            msg = stream.next() => {
                let msg = match msg {
                    Some(Ok(m)) => m,
                    Some(Err(e)) => return Err(format!("read error: {e}")),
                    None => return Ok("server closed".to_string()),
                };
                let text = match msg {
                    Message::Text(t) => t.to_string(),
                    Message::Ping(p) => {
                        let _ = sink.send(Message::Pong(p)).await;
                        continue;
                    }
                    Message::Pong(_) | Message::Binary(_) | Message::Frame(_) => continue,
                    Message::Close(c) => return Ok(format!("close frame: {c:?}")),
                };
                saw_msg_at = std::time::Instant::now();

                let m: LtMsg = match serde_json::from_str(&text) {
                    Ok(m) => m,
                    Err(e) => {
                        tracing::debug!("[lighter] unparsable message: {e}");
                        continue;
                    }
                };
                match m.kind.as_str() {
                    "connected" => {
                        for market in Market::ALL {
                            let sub = serde_json::json!({
                                "type": "subscribe",
                                "channel": format!("order_book/{}", market.lighter_market_index())
                            });
                            sink.send(Message::Text(sub.to_string().into()))
                                .await
                                .map_err(|e| format!("subscribe send failed: {e}"))?;
                        }
                        subscribed = true;
                        tracing::info!("[lighter] session accepted, subscribed to {} order books",
                            Market::ALL.len());
                    }
                    "ping" => {
                        if sink
                            .send(Message::Text("{\"type\":\"pong\"}".into()))
                            .await
                            .is_err()
                        {
                            return Err("pong send failed".into());
                        }
                        reg.lt.record_msg();
                    }
                    "subscribed/order_book" | "update/order_book" => {
                        if !subscribed {
                            continue;
                        }
                        let Some(channel) = m.channel.as_deref() else { continue };
                        let Some(market) = market_for_channel(channel) else { continue };
                        let Some(ob) = m.order_book else { continue };

                        if m.kind == "subscribed/order_book" {
                            // Full snapshot.
                            reg.replace_lt(market, parse_levels(&ob.bids), parse_levels(&ob.asks));
                            if !got_snapshot {
                                got_snapshot = true;
                                reg.lt.set_status(FeedStatus::Live);
                                tracing::info!("[lighter] {} snapshot: {} bids / {} asks",
                                    market.label(), ob.bids.len(), ob.asks.len());
                            }
                        } else {
                            // Incremental delta; create the book lazily in case
                            // the snapshot is still in flight.
                            reg.touch_lt(market);
                            if !ob.bids.is_empty() {
                                reg.apply_lt_side(market, Side::Bid, &parse_levels(&ob.bids));
                            }
                            if !ob.asks.is_empty() {
                                reg.apply_lt_side(market, Side::Ask, &parse_levels(&ob.asks));
                            }
                        }
                        reg.lt.record_msg();
                    }
                    other => {
                        tracing::debug!("[lighter] ignoring message type {other}");
                    }
                }
            }
        }
    }
}
