//! Binance spot websocket connector (combined stream).
//!
//! Protocol (live-verified against wss://stream.binance.com:9443):
//! - one combined stream URL carries all channels:
//!   `ethusdt@depth20@100ms` — FULL top-20 snapshot every 100 ms
//!   `ethusdt@trade`         — trade events {p, q, T, m}
//! - envelope: `{"stream":"ethusdt@depth20@100ms","data":{...}}`
//! - depth payload: `{"lastUpdateId":..,"bids":[["px","sz"],..],"asks":[...]}`
//! - trade payload: `{"e":"trade","p":"px","q":"sz","T":ms,"t":id,"m":bool}`
//!   m=true  -> the buyer was the maker -> the taker SOLD ("A")
//!   m=false -> taker bought ("B")
//! - server sends WS ping frames (auto-ponged here); connections are capped
//!   at 24h server-side, handled by the reconnect loop.

use crate::state::Registry;
use futures_util::{SinkExt, StreamExt};
use ob_core::{FeedStatus, Market, Trade, Venue};
use rust_decimal::Decimal;
use std::sync::Arc;
use std::time::Duration;
use tokio_tungstenite::tungstenite::Message;

pub const BINANCE_WS_URL: &str = "wss://stream.binance.com:9443/stream\
?streams=ethusdt@depth20@100ms/solusdt@depth20@100ms/ethusdt@trade/solusdt@trade";

#[derive(serde::Deserialize)]
struct BnDepth {
    #[serde(default)]
    bids: Vec<(String, String)>,
    #[serde(default)]
    asks: Vec<(String, String)>,
}

#[derive(serde::Deserialize)]
struct BnTrade {
    #[serde(rename = "p")]
    px: String,
    #[serde(rename = "q")]
    sz: String,
    #[serde(rename = "T")]
    t: u64,
    #[serde(rename = "t")]
    id: u64,
    #[serde(rename = "m")]
    maker_is_buyer: bool,
}

#[derive(serde::Deserialize)]
struct BnEnvelope {
    stream: String,
    data: serde_json::Value,
}

fn market_for_stream(s: &str) -> Option<Market> {
    match s.split('@').next()? {
        "ethusdt" => Some(Market::Eth),
        "solusdt" => Some(Market::Sol),
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
        reg.feed(Venue::Binance).set_status(FeedStatus::Connecting);
        match connect_once(&reg).await {
            Ok(reason) => {
                tracing::warn!("[binance] stream ended ({reason}); reconnecting in {backoff}s");
            }
            Err(e) => {
                tracing::warn!("[binance] error: {e}; reconnecting in {backoff}s");
            }
        }
        reg.feed(Venue::Binance).set_status(FeedStatus::Reconnecting);
        tokio::time::sleep(Duration::from_secs(backoff)).await;
        backoff = (backoff * 2).min(30);
    }
}

async fn connect_once(reg: &Arc<Registry>) -> Result<String, String> {
    let (ws, _resp) = crate::wsio::connect(BINANCE_WS_URL).await?;
    tracing::info!("[binance] connected to stream.binance.com (combined: depth20 + trades)");
    let (mut sink, mut stream) = ws.split();
    let feed = reg.feed(Venue::Binance);
    let mut got_snapshot = false;

    loop {
        let msg = match stream.next().await {
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

        let env: BnEnvelope = match serde_json::from_str(&text) {
            Ok(e) => e,
            Err(_) => continue,
        };
        let Some(market) = market_for_stream(&env.stream) else { continue };
        if env.stream.ends_with("@trade") {
            if let Ok(t) = serde_json::from_value::<BnTrade>(env.data) {
                let trade = Trade {
                    venue: Venue::Binance,
                    side: if t.maker_is_buyer { "A" } else { "B" }.to_string(),
                    px: t.px,
                    sz: t.sz,
                    t: t.t,
                    id: format!("bn-{}", t.id),
                };
                reg.push_trades(market, vec![trade]);
            }
        } else if let Ok(d) = serde_json::from_value::<BnDepth>(env.data) {
            reg.replace_venue(Venue::Binance, market, parse_levels(&d.bids), parse_levels(&d.asks));
            if !got_snapshot {
                got_snapshot = true;
                feed.set_status(FeedStatus::Live);
            }
        }
    }
}
