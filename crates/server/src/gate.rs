//! Gate.io v4 spot websocket connector.
//!
//! Protocol (live-verified against wss://api.gateio.ws/ws/v4/):
//! - subscribe: `{"time":unix,"channel":"spot.order_book","event":"subscribe",
//!    "payload":["ETH_USDT","20","100ms"]}` — 20 levels, 100 ms cadence;
//!    and `{"channel":"spot.trades","payload":["ETH_USDT"]}`.
//! - order book pushes are FULL snapshots:
//!   `{"channel":"spot.order_book","event":"update","result":{"s":"ETH_USDT",
//!     "b":[[px,sz]..],"a":[[px,sz]..]}}`
//! - trades: `{"channel":"spot.trades","event":"update","result":[{
//!    "id":..,"create_time_ms":..,"side":"sell","role":"taker","amount":"..",
//!    "price":".."}]}` — the taker side is the entry with role=="taker".
//! - heartbeat: `{"time":unix,"channel":"spot.ping"}` every 15 s.

use crate::state::Registry;
use futures_util::{SinkExt, StreamExt};
use ob_core::{FeedStatus, Market, Trade, Venue};
use rust_decimal::Decimal;
use std::sync::Arc;
use std::time::Duration;
use tokio::time::{interval_at, Instant};
use tokio_tungstenite::tungstenite::Message;

pub const GATE_WS_URL: &str = "wss://api.gateio.ws/ws/v4/";

#[derive(serde::Deserialize)]
struct GtMsg {
    channel: String,
    event: String,
    #[serde(default)]
    result: serde_json::Value,
}

#[derive(serde::Deserialize)]
struct GtBook {
    #[serde(rename = "s")]
    #[allow(dead_code)]
    symbol: String,
    #[serde(default, alias = "b")]
    bids: Vec<(String, String)>,
    #[serde(default, alias = "a")]
    asks: Vec<(String, String)>,
}

#[derive(serde::Deserialize)]
struct GtTrade {
    id: serde_json::Value,
    #[serde(default)]
    currency_pair: String,
    #[serde(default)]
    create_time_ms: String,
    #[serde(default)]
    side: String,
    amount: String,
    price: String,
}

fn market_for_symbol(s: &str) -> Option<Market> {
    match s {
        "ETH_USDT" => Some(Market::Eth),
        "SOL_USDT" => Some(Market::Sol),
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
        reg.feed(Venue::Gate).set_status(FeedStatus::Connecting);
        match connect_once(&reg).await {
            Ok(reason) => {
                tracing::warn!("[gate] stream ended ({reason}); reconnecting in {backoff}s");
                // Connection was established; recover the backoff quickly.
                backoff = (backoff + 1) / 2;
            }
            Err(e) => {
                tracing::warn!("[gate] error: {e}; reconnecting in {backoff}s");
            }
        }
        reg.feed(Venue::Gate).set_status(FeedStatus::Reconnecting);
        tokio::time::sleep(Duration::from_secs(backoff)).await;
        backoff = (backoff * 2).min(30);
    }
}

async fn connect_once(reg: &Arc<Registry>) -> Result<String, String> {
    let ws = crate::wsio::connect(GATE_WS_URL).await?;
    tracing::info!("[gate] connected to {GATE_WS_URL}");
    let (mut sink, mut stream) = ws.split();

    let now = || std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    for (sym, kind) in [
        ("ETH_USDT", "book"),
        ("SOL_USDT", "book"),
        ("ETH_USDT", "trades"),
        ("SOL_USDT", "trades"),
    ] {
        let sub = if kind == "book" {
            serde_json::json!({
                "time": now(), "channel": "spot.order_book", "event": "subscribe",
                "payload": [sym, "20", "100ms"],
            })
        } else {
            serde_json::json!({
                "time": now(), "channel": "spot.trades", "event": "subscribe",
                "payload": [sym],
            })
        };
        sink.send(Message::Text(sub.to_string().into()))
            .await
            .map_err(|e| format!("subscribe send failed: {e}"))?;
    }
    tracing::info!("[gate] subscribed spot.order_book(20,100ms) + spot.trades for ETH/SOL");

    let feed = reg.feed(Venue::Gate);
    let mut got_snapshot = false;
    let mut hb = interval_at(Instant::now() + Duration::from_secs(15), Duration::from_secs(15));

    loop {
        tokio::select! {
            _ = hb.tick() => {
                if sink
                    .send(Message::Text(serde_json::json!({
                        "time": now(), "channel": "spot.ping",
                    }).to_string().into()))
                    .await
                    .is_err()
                {
                    return Err("ping send failed".into());
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
                feed.record_msg();

                let m: GtMsg = match serde_json::from_str(&text) {
                    Ok(m) => m,
                    Err(_) => continue,
                };
                match m.channel.as_str() {
                    "spot.order_book" => {
                        if m.event != "update" {
                            continue; // subscribe acks
                        }
                        let Ok(b) = serde_json::from_value::<GtBook>(m.result) else { continue };
                        let Some(market) = market_for_symbol(&b.symbol) else { continue };
                        reg.replace_venue(Venue::Gate, market, parse_levels(&b.bids), parse_levels(&b.asks));
                        if !got_snapshot {
                            got_snapshot = true;
                            feed.set_status(FeedStatus::Live);
                        }
                    }
                    "spot.trades" => {
                        if m.event != "update" {
                            continue;
                        }
                        // Single trade object with `currency_pair` inside.
                        let Ok(t) = serde_json::from_value::<GtTrade>(m.result) else { continue };
                        let Some(market) = market_for_symbol(&t.currency_pair) else { continue };
                        let t_ms: u64 = t
                            .create_time_ms
                            .split('.')
                            .next()
                            .and_then(|s| s.parse().ok())
                            .unwrap_or_else(crate::state::now_ms);
                        let id = match &t.id {
                            serde_json::Value::Number(n) => n.to_string(),
                            serde_json::Value::String(s) => s.clone(),
                            _ => format!("{}-{}", t_ms, t.price),
                        };
                        let trade = Trade {
                            venue: Venue::Gate,
                            side: if t.side.eq_ignore_ascii_case("buy") {
                                "B"
                            } else {
                                "A"
                            }
                            .to_string(),
                            px: t.price,
                            sz: t.amount,
                            t: t_ms,
                            id: format!("gt-{id}"),
                        };
                        reg.push_trades(market, vec![trade]);
                    }
                    _ => {}
                }
            }
        }
    }
}
