//! Bybit v5 spot websocket connector.
//!
//! Protocol (live-verified against wss://stream.bybit.com/v5/public/spot):
//! - subscribe: `{"op":"subscribe","args":["orderbook.50.ETHUSDT",...]}`
//! - orderbook.50: first push `{"type":"snapshot","data":{"s","b":[[px,sz]],"a":[...],"u":..}}`
//!   then `{"type":"delta",...}` with the same shape; a size of "0" removes
//!   the level. `data.u` is a per-topic update counter: strictly +1 per
//!   delta; a jump means missed messages -> reconnect.
//! - publicTrade: `{"topic":"publicTrade.ETHUSDT","data":[{"T":ms,"S":"Buy","v":"sz","p":"px"}]}`
//!   S is the TAKER side.
//! - server sends WS ping frames (auto-ponged here).

use crate::state::Registry;
use futures_util::{SinkExt, StreamExt};
use ob_core::{FeedStatus, Market, Side, Trade, Venue};
use rust_decimal::Decimal;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio_tungstenite::tungstenite::Message;

pub const BYBIT_WS_URL: &str = "wss://stream.bybit.com/v5/public/spot";

#[derive(serde::Deserialize)]
struct ByBookData {
    #[serde(rename = "s")]
    #[allow(dead_code)]
    symbol: String,
    #[serde(default)]
    b: Vec<(String, String)>,
    #[serde(default)]
    a: Vec<(String, String)>,
    #[serde(default)]
    u: u64,
}

#[derive(serde::Deserialize)]
struct ByMsg {
    topic: String,
    #[serde(rename = "type")]
    kind: String,
    #[serde(default)]
    data: serde_json::Value,
}

#[derive(serde::Deserialize)]
struct ByTrade {
    #[serde(rename = "T")]
    t: u64,
    #[serde(rename = "S")]
    side: String,
    #[serde(rename = "v")]
    sz: String,
    #[serde(rename = "p")]
    px: String,
}

fn market_for_topic(topic: &str) -> Option<Market> {
    let sym = topic.rsplit('.').next()?;
    match sym {
        "ETHUSDT" => Some(Market::Eth),
        "SOLUSDT" => Some(Market::Sol),
        _ => None,
    }
}

fn parse_levels(raw: &[(String, String)]) -> Vec<(Decimal, Decimal, Option<u32>)> {
    raw.iter()
        .filter_map(|(px, sz)| {
            let px: Decimal = px.parse().ok()?;
            let sz: Decimal = sz.parse().ok()?;
            Some((px, sz, None))
        })
        .collect()
}

pub async fn run(reg: Arc<Registry>) {
    let mut backoff = 1u64;
    loop {
        reg.feed(Venue::Bybit).set_status(FeedStatus::Connecting);
        match connect_once(&reg).await {
            Ok(reason) => {
                tracing::warn!("[bybit] stream ended ({reason}); reconnecting in {backoff}s");
                // Connection was established; recover the backoff quickly.
                backoff = (backoff + 1) / 2;
            }
            Err(e) => {
                tracing::warn!("[bybit] error: {e}; reconnecting in {backoff}s");
            }
        }
        reg.feed(Venue::Bybit).set_status(FeedStatus::Reconnecting);
        tokio::time::sleep(Duration::from_secs(backoff)).await;
        backoff = (backoff * 2).min(30);
    }
}

async fn connect_once(reg: &Arc<Registry>) -> Result<String, String> {
    let ws = crate::wsio::connect(BYBIT_WS_URL).await?;
    tracing::info!("[bybit] connected to {BYBIT_WS_URL}");
    let (mut sink, mut stream) = ws.split();

    let sub = serde_json::json!({
        "op": "subscribe",
        "args": [
            "orderbook.50.ETHUSDT", "orderbook.50.SOLUSDT",
            "publicTrade.ETHUSDT", "publicTrade.SOLUSDT",
        ]
    });
    sink.send(Message::Text(sub.to_string().into()))
        .await
        .map_err(|e| format!("subscribe send failed: {e}"))?;
    tracing::info!("[bybit] subscribed orderbook.50 + publicTrade for ETH/SOL");

    let feed = reg.feed(Venue::Bybit);
    let mut got_snapshot = false;
    // Per-topic last update id for gap detection.
    let mut last_u: HashMap<Market, u64> = HashMap::new();

    // Watchdog: Bybit drops silent connections; 20s without any frame -> reconnect.
    let mut saw_msg_at = std::time::Instant::now();

    loop {
        let msg = tokio::select! {
            m = stream.next() => match m {
                Some(Ok(m)) => m,
                Some(Err(e)) => return Err(format!("read error: {e}")),
                None => return Ok("server closed".to_string()),
            },
            _ = tokio::time::sleep(Duration::from_secs(20)) => {
                if saw_msg_at.elapsed() > Duration::from_secs(20) {
                    return Err("20s without data (watchdog)".into());
                }
                continue;
            }
        };
        saw_msg_at = std::time::Instant::now();
        let text = match msg {
            Message::Text(t) => t.to_string(),
            Message::Ping(p) => {
                let _ = sink.send(Message::Pong(p)).await;
                continue;
            }
            Message::Pong(_) | Message::Binary(_) | Message::Frame(_) => continue,
            Message::Close(c) => return Ok(format!("close frame: {c:?}")),
        };
        feed.record_msg();

        let m: ByMsg = match serde_json::from_str(&text) {
            Ok(m) => m,
            Err(_) => continue, // subscribe acks etc.
        };
        let Some(market) = market_for_topic(&m.topic) else { continue };

        if m.topic.starts_with("publicTrade") {
            if let Ok(trades) = serde_json::from_value::<Vec<ByTrade>>(m.data) {
                let out: Vec<Trade> = trades
                    .into_iter()
                    .map(|t| {
                        let id = format!("by-{}-{}", t.t, t.px);
                        Trade {
                            venue: Venue::Bybit,
                            side: if t.side.eq_ignore_ascii_case("buy") { "B" } else { "A" }
                                .to_string(),
                            px: t.px,
                            sz: t.sz,
                            t: t.t,
                            id,
                        }
                    })
                    .collect();
                if !out.is_empty() {
                    reg.push_trades(market, out);
                }
            }
            continue;
        }

        if !m.topic.starts_with("orderbook") {
            continue;
        }
        let Ok(d) = serde_json::from_value::<ByBookData>(m.data) else { continue };
        match m.kind.as_str() {
            "snapshot" => {
                reg.replace_venue(Venue::Bybit, market, parse_levels(&d.b), parse_levels(&d.a));
                last_u.insert(market, d.u);
                if !got_snapshot {
                    got_snapshot = true;
                    feed.set_status(FeedStatus::Live);
                }
            }
            "delta" => {
                let prev = last_u.get(&market).copied().unwrap_or(0);
                if prev > 0 && d.u > prev + 1 {
                    return Err(format!("gap: u jumped {prev} -> {}", d.u));
                }
                if d.u <= prev {
                    continue; // stale / duplicate
                }
                last_u.insert(market, d.u);
                reg.touch_venue(Venue::Bybit, market);
                if !d.b.is_empty() {
                    reg.apply_venue_side(Venue::Bybit, market, Side::Bid, &parse_levels(&d.b));
                }
                if !d.a.is_empty() {
                    reg.apply_venue_side(Venue::Bybit, market, Side::Ask, &parse_levels(&d.a));
                }
            }
            _ => {}
        }
    }
}
