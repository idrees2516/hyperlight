//! Kraken v2 websocket connector.
//!
//! Protocol (live-verified against wss://ws.kraken.com/v2):
//! - subscribe:
//!   `{"method":"subscribe","params":{"channel":"book","symbol":["ETH/USD","SOL/USD","USDT/USD"],"depth":100}}`
//!   `{"method":"subscribe","params":{"channel":"trade","symbol":["ETH/USD","SOL/USD"]}}`
//! - book: `{"channel":"book","type":"snapshot"|"update","data":[{"symbol":..,
//!    "bids":[{"price":..,"qty":..}],"asks":[...],"republish":true?}]}`
//!    (numbers, not strings — read via serde_json::Number to keep exact text);
//!    qty 0 removes a level; republish=true => treat as a snapshot.
//! - trade: `{"channel":"trade","type":"update","data":[{"symbol":..,"side":"buy",
//!    "price":..,"qty":..,"trade_id":..,"timestamp":"ISO"}]}` — side is the taker.
//! - heartbeat: client sends `{"method":"ping"}` every 20 s -> `{"method":"pong"}`.
//! - The USDT/USD book feeds the registry's live USDT/USD conversion rate.
//!
//! Number handling: Kraken sends JSON numbers; `serde_json::Number`'s Display
//! prints the shortest round-trip form, so `to_string().parse::<Decimal>()`
//! is exact for every real price/size.

use crate::state::Registry;
use futures_util::{SinkExt, StreamExt};
use ob_core::{FeedStatus, Market, Side, Trade, Venue};
use rust_decimal::Decimal;
use std::sync::Arc;
use std::time::Duration;
use tokio::time::{interval_at, Instant};
use tokio_tungstenite::tungstenite::Message;

pub const KRAKEN_WS_URL: &str = "wss://ws.kraken.com/v2";

#[derive(serde::Deserialize)]
struct KrLevel {
    price: serde_json::Number,
    qty: serde_json::Number,
}

#[derive(serde::Deserialize)]
struct KrBookEntry {
    symbol: String,
    #[serde(default)]
    bids: Vec<KrLevel>,
    #[serde(default)]
    asks: Vec<KrLevel>,
    #[serde(default)]
    republish: bool,
}

#[derive(serde::Deserialize)]
struct KrTrade {
    symbol: String,
    side: String,
    price: serde_json::Number,
    qty: serde_json::Number,
    trade_id: u64,
}

#[derive(serde::Deserialize)]
struct KrMsg {
    channel: String,
    #[serde(rename = "type")]
    kind: String,
    #[serde(default)]
    data: serde_json::Value,
}

fn market_for_symbol(s: &str) -> Option<Market> {
    match s {
        "ETH/USD" => Some(Market::Eth),
        "SOL/USD" => Some(Market::Sol),
        _ => None,
    }
}

fn parse_levels(raw: &[KrLevel]) -> Vec<(Decimal, Decimal, Option<u32>)> {
    raw.iter()
        .filter_map(|l| {
            let px: Decimal = l.price.to_string().parse().ok()?;
            let sz: Decimal = l.qty.to_string().parse().ok()?;
            Some((px, sz, None))
        })
        .collect()
}

/// Mid of a snapshot entry (for the USDT/USD conversion book).
fn entry_mid(e: &KrBookEntry) -> Option<Decimal> {
    let bb = e
        .bids
        .iter()
        .max_by(|a, b| a.price.as_f64().unwrap_or(0.0).total_cmp(&b.price.as_f64().unwrap_or(0.0)))?;
    let ba = e
        .asks
        .iter()
        .min_by(|a, b| a.price.as_f64().unwrap_or(0.0).total_cmp(&b.price.as_f64().unwrap_or(0.0)))?;
    let bb: Decimal = bb.price.to_string().parse().ok()?;
    let ba: Decimal = ba.price.to_string().parse().ok()?;
    Some((bb + ba) / Decimal::from(2u64))
}

pub async fn run(reg: Arc<Registry>) {
    let mut backoff = 1u64;
    loop {
        reg.feed(Venue::Kraken).set_status(FeedStatus::Connecting);
        match connect_once(&reg).await {
            Ok(reason) => {
                tracing::warn!("[kraken] stream ended ({reason}); reconnecting in {backoff}s");
            }
            Err(e) => {
                tracing::warn!("[kraken] error: {e}; reconnecting in {backoff}s");
            }
        }
        reg.feed(Venue::Kraken).set_status(FeedStatus::Reconnecting);
        tokio::time::sleep(Duration::from_secs(backoff)).await;
        backoff = (backoff * 2).min(30);
    }
}

async fn connect_once(reg: &Arc<Registry>) -> Result<String, String> {
    let (ws, _resp) = crate::wsio::connect(KRAKEN_WS_URL).await?;
    tracing::info!("[kraken] connected to {KRAKEN_WS_URL}");
    let (mut sink, mut stream) = ws.split();

    let book_sub = serde_json::json!({
        "method": "subscribe",
        "params": {
            "channel": "book",
            "symbol": ["ETH/USD", "SOL/USD", "USDT/USD"],
            "depth": 100,
        }
    });
    sink.send(Message::Text(book_sub.to_string().into()))
        .await
        .map_err(|e| format!("book subscribe send failed: {e}"))?;
    let trade_sub = serde_json::json!({
        "method": "subscribe",
        "params": { "channel": "trade", "symbol": ["ETH/USD", "SOL/USD"] }
    });
    sink.send(Message::Text(trade_sub.to_string().into()))
        .await
        .map_err(|e| format!("trade subscribe send failed: {e}"))?;
    tracing::info!("[kraken] subscribed book(100) ETH/SOL/USDT + trade ETH/SOL");

    let feed = reg.feed(Venue::Kraken);
    let mut got_snapshot = false;
    let mut ping = interval_at(Instant::now() + Duration::from_secs(20), Duration::from_secs(20));

    loop {
        tokio::select! {
            _ = ping.tick() => {
                if sink
                    .send(Message::Text("{\"method\":\"ping\"}".into()))
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

                let m: KrMsg = match serde_json::from_str(&text) {
                    Ok(m) => m,
                    Err(_) => continue, // status / pong / subscribe acks
                };
                match m.channel.as_str() {
                    "book" => {
                        let Ok(entries) = serde_json::from_value::<Vec<KrBookEntry>>(m.data) else { continue };
                        for e in entries {
                            if e.symbol == "USDT/USD" {
                                if let Some(mid) = entry_mid(&e) {
                                    reg.set_usdt_rate(mid);
                                }
                                continue;
                            }
                            let Some(market) = market_for_symbol(&e.symbol) else { continue };
                            let is_snapshot = m.kind == "snapshot" || e.republish;
                            if is_snapshot {
                                reg.replace_venue(
                                    Venue::Kraken,
                                    market,
                                    parse_levels(&e.bids),
                                    parse_levels(&e.asks),
                                );
                            } else {
                                reg.touch_venue(Venue::Kraken, market);
                                if !e.bids.is_empty() {
                                    reg.apply_venue_side(Venue::Kraken, market, Side::Bid, &parse_levels(&e.bids));
                                }
                                if !e.asks.is_empty() {
                                    reg.apply_venue_side(Venue::Kraken, market, Side::Ask, &parse_levels(&e.asks));
                                }
                            }
                            if !got_snapshot {
                                got_snapshot = true;
                                feed.set_status(FeedStatus::Live);
                            }
                        }
                    }
                    "trade" => {
                        let Ok(trades) = serde_json::from_value::<Vec<KrTrade>>(m.data) else { continue };
                        let mut by_market: Vec<(Market, Vec<Trade>)> = Vec::new();
                        for t in trades {
                            let Some(market) = market_for_symbol(&t.symbol) else { continue };
                            let trade = Trade {
                                venue: Venue::Kraken,
                                side: if t.side.eq_ignore_ascii_case("buy") { "B" } else { "A" }
                                    .to_string(),
                                px: t.price.to_string(),
                                sz: t.qty.to_string(),
                                t: crate::state::now_ms(), // ISO ts; receipt time is close enough
                                id: format!("kr-{}", t.trade_id),
                            };
                            match by_market.iter_mut().find(|(m, _)| *m == market) {
                                Some((_, v)) => v.push(trade),
                                None => by_market.push((market, vec![trade])),
                            }
                        }
                        for (market, out) in by_market {
                            reg.push_trades(market, out);
                        }
                    }
                    _ => {}
                }
            }
        }
    }
}
