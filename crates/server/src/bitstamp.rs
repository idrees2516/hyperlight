//! Bitstamp websocket connector.
//!
//! Protocol (live-verified against wss://ws.bitstamp.net):
//! - subscribe: `{"event":"bts:subscribe","data":{"channel":"order_book_ethusd"}}`
//!   (also order_book_solusd, live_trades_ethusd, live_trades_solusd)
//! - order_book pushes are FULL snapshots (~5 Hz):
//!   `{"event":"data","channel":"order_book_ethusd","data":{"bids":[[px,sz]..],"asks":[...]}}`
//! - live_trades: `{"event":"trade","channel":"live_trades_ethusd","data":{
//!    "id":..,"timestamp":"s","amount_str":"..","price_str":"..","type":0|1}}`
//!   type 0 = taker bought, 1 = taker sold.
//! - heartbeat: client sends `{"event":"bts:heartbeat"}` every 15 s.

use crate::state::Registry;
use futures_util::{SinkExt, StreamExt};
use ob_core::{FeedStatus, Market, Trade, Venue};
use rust_decimal::Decimal;
use std::sync::Arc;
use std::time::Duration;
use tokio::time::{interval_at, Instant};
use tokio_tungstenite::tungstenite::Message;

pub const BITSTAMP_WS_URL: &str = "wss://ws.bitstamp.net";

#[derive(serde::Deserialize)]
struct BsMsg {
    event: String,
    #[serde(default)]
    channel: String,
    #[serde(default)]
    data: serde_json::Value,
}

#[derive(serde::Deserialize)]
struct BsBook {
    #[serde(default)]
    bids: Vec<(String, String)>,
    #[serde(default)]
    asks: Vec<(String, String)>,
}

#[derive(serde::Deserialize)]
struct BsTrade {
    id: u64,
    #[serde(default)]
    timestamp: String,
    amount_str: String,
    price_str: String,
    /// 0 = taker bought, 1 = taker sold.
    #[serde(rename = "type")]
    kind: u8,
}

fn market_for_channel(ch: &str) -> Option<Market> {
    match ch {
        "order_book_ethusd" | "live_trades_ethusd" => Some(Market::Eth),
        "order_book_solusd" | "live_trades_solusd" => Some(Market::Sol),
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
        reg.feed(Venue::Bitstamp).set_status(FeedStatus::Connecting);
        match connect_once(&reg).await {
            Ok(reason) => {
                tracing::warn!("[bitstamp] stream ended ({reason}); reconnecting in {backoff}s");
            }
            Err(e) => {
                tracing::warn!("[bitstamp] error: {e}; reconnecting in {backoff}s");
            }
        }
        reg.feed(Venue::Bitstamp).set_status(FeedStatus::Reconnecting);
        tokio::time::sleep(Duration::from_secs(backoff)).await;
        backoff = (backoff * 2).min(30);
    }
}

async fn connect_once(reg: &Arc<Registry>) -> Result<String, String> {
    let (ws, _resp) = crate::wsio::connect(BITSTAMP_WS_URL).await?;
    tracing::info!("[bitstamp] connected to {BITSTAMP_WS_URL}");
    let (mut sink, mut stream) = ws.split();

    for ch in [
        "order_book_ethusd",
        "order_book_solusd",
        "live_trades_ethusd",
        "live_trades_solusd",
    ] {
        let sub = serde_json::json!({ "event": "bts:subscribe", "data": { "channel": ch } });
        sink.send(Message::Text(sub.to_string().into()))
            .await
            .map_err(|e| format!("subscribe send failed: {e}"))?;
    }
    tracing::info!("[bitstamp] subscribed order_book + live_trades for ETH/SOL");

    let feed = reg.feed(Venue::Bitstamp);
    let mut got_snapshot = false;
    let mut hb = interval_at(Instant::now() + Duration::from_secs(15), Duration::from_secs(15));

    loop {
        tokio::select! {
            _ = hb.tick() => {
                if sink
                    .send(Message::Text("{\"event\":\"bts:heartbeat\"}".into()))
                    .await
                    .is_err()
                {
                    return Err("heartbeat send failed".into());
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

                let m: BsMsg = match serde_json::from_str(&text) {
                    Ok(m) => m,
                    Err(_) => continue,
                };
                if m.event != "data" && m.event != "trade" {
                    continue; // subscription acks, heartbeats
                }
                let Some(market) = market_for_channel(&m.channel) else { continue };

                if m.channel.starts_with("order_book") {
                    if let Ok(b) = serde_json::from_value::<BsBook>(m.data) {
                        reg.replace_venue(
                            Venue::Bitstamp,
                            market,
                            parse_levels(&b.bids),
                            parse_levels(&b.asks),
                        );
                        if !got_snapshot {
                            got_snapshot = true;
                            feed.set_status(FeedStatus::Live);
                        }
                    }
                } else if m.event == "trade" {
                    if let Ok(t) = serde_json::from_value::<BsTrade>(m.data) {
                        let t_ms: u64 = t
                            .timestamp
                            .parse::<u64>()
                            .map(|s| s * 1000)
                            .unwrap_or_else(|_| crate::state::now_ms());
                        let trade = Trade {
                            venue: Venue::Bitstamp,
                            side: if t.kind == 0 { "B" } else { "A" }.to_string(),
                            px: t.price_str,
                            sz: t.amount_str,
                            t: t_ms,
                            id: format!("bs-{}", t.id),
                        };
                        reg.push_trades(market, vec![trade]);
                    }
                }
            }
        }
    }
}
