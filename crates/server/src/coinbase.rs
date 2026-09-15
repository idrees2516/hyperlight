//! Coinbase Exchange websocket connector.
//!
//! Protocol (live-verified against wss://ws-feed.exchange.coinbase.com):
//! - `level2` / `full` channels now REQUIRE authentication, so books are
//!   sourced from the public **ticker** channel: every trade and every
//!   best-bid/ask change pushes `{"type":"ticker","product_id":..,
//!   "best_bid","best_bid_size","best_ask","best_ask_size",...}` — an
//!   exact 1-level book, which the depth-walking arb engine handles
//!   natively (it simply stops after one level).
//! - `matches` streams individual trades:
//!   `{"type":"match","side":"buy"|"sell","price","size","time","trade_id"}`
//!   side is the TAKER side. The first message per product is `last_match`.
//! - no client heartbeat needed (server pings at the WS frame level).

use crate::state::Registry;
use futures_util::{SinkExt, StreamExt};
use ob_core::{FeedStatus, Market, Trade, Venue};
use rust_decimal::Decimal;
use std::sync::Arc;
use std::time::Duration;
use tokio_tungstenite::tungstenite::Message;

pub const COINBASE_WS_URL: &str = "wss://ws-feed.exchange.coinbase.com";

#[derive(serde::Deserialize)]
struct CbMsg {
    #[serde(rename = "type")]
    kind: String,
    #[serde(default)]
    product_id: Option<String>,
    #[serde(default)]
    best_bid: Option<String>,
    #[serde(default)]
    best_bid_size: Option<String>,
    #[serde(default)]
    best_ask: Option<String>,
    #[serde(default)]
    best_ask_size: Option<String>,
    #[serde(default)]
    side: Option<String>,
    #[serde(default)]
    price: Option<String>,
    #[serde(default)]
    size: Option<String>,
    #[serde(default)]
    trade_id: Option<u64>,
}

fn market_for_product(p: &str) -> Option<Market> {
    match p {
        "ETH-USD" => Some(Market::Eth),
        "SOL-USD" => Some(Market::Sol),
        _ => None,
    }
}

pub async fn run(reg: Arc<Registry>) {
    let mut backoff = 1u64;
    loop {
        reg.feed(Venue::Coinbase).set_status(FeedStatus::Connecting);
        match connect_once(&reg).await {
            Ok(reason) => {
                tracing::warn!("[coinbase] stream ended ({reason}); reconnecting in {backoff}s");
                // Connection was established; recover the backoff quickly.
                backoff = (backoff + 1) / 2;
            }
            Err(e) => {
                tracing::warn!("[coinbase] error: {e}; reconnecting in {backoff}s");
            }
        }
        reg.feed(Venue::Coinbase).set_status(FeedStatus::Reconnecting);
        tokio::time::sleep(Duration::from_secs(backoff)).await;
        backoff = (backoff * 2).min(30);
    }
}

async fn connect_once(reg: &Arc<Registry>) -> Result<String, String> {
    let ws = crate::wsio::connect(COINBASE_WS_URL).await?;
    tracing::info!("[coinbase] connected to {COINBASE_WS_URL}");
    let (mut sink, mut stream) = ws.split();

    let sub = serde_json::json!({
        "type": "subscribe",
        "product_ids": ["ETH-USD", "SOL-USD"],
        "channels": ["ticker", "matches"],
    });
    sink.send(Message::Text(sub.to_string().into()))
        .await
        .map_err(|e| format!("subscribe send failed: {e}"))?;
    tracing::info!("[coinbase] subscribed ticker + matches for ETH/SOL (level2 is auth-gated)");

    let feed = reg.feed(Venue::Coinbase);
    let mut got_snapshot = false;

    // Watchdog: ticker+matches are frequent; 30s of silence -> reconnect.
    let mut saw_msg_at = std::time::Instant::now();

    loop {
        let msg = tokio::select! {
            m = stream.next() => match m {
                Some(Ok(m)) => m,
                Some(Err(e)) => return Err(format!("read error: {e}")),
                None => return Ok("server closed".to_string()),
            },
            _ = tokio::time::sleep(Duration::from_secs(30)) => {
                if saw_msg_at.elapsed() > Duration::from_secs(30) {
                    return Err("30s without data (watchdog)".into());
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

        let m: CbMsg = match serde_json::from_str(&text) {
            Ok(m) => m,
            Err(_) => continue,
        };
        match m.kind.as_str() {
            "ticker" => {
                let Some(p) = m.product_id.as_deref() else { continue };
                let Some(market) = market_for_product(p) else { continue };
                let (Some(bb), Some(bbs), Some(ba), Some(bas)) = (
                    m.best_bid.as_deref(),
                    m.best_bid_size.as_deref(),
                    m.best_ask.as_deref(),
                    m.best_ask_size.as_deref(),
                ) else {
                    continue;
                };
                let (Ok(bbp), Ok(bbsz)) = (bb.parse::<Decimal>(), bbs.parse::<Decimal>()) else { continue };
                let (Ok(bap), Ok(basz)) = (ba.parse::<Decimal>(), bas.parse::<Decimal>()) else { continue };
                // Exact 1-level book replacement.
                let bids = if bbsz.is_zero() { vec![] } else { vec![(bbp, bbsz, None)] };
                let asks = if basz.is_zero() { vec![] } else { vec![(bap, basz, None)] };
                reg.replace_venue(Venue::Coinbase, market, bids, asks);
                if !got_snapshot {
                    got_snapshot = true;
                    feed.set_status(FeedStatus::Live);
                }
            }
            "match" | "last_match" => {
                let Some(p) = m.product_id.as_deref() else { continue };
                let Some(market) = market_for_product(p) else { continue };
                let (Some(px), Some(sz), Some(side)) =
                    (m.price.as_deref(), m.size.as_deref(), m.side.as_deref())
                else {
                    continue;
                };
                let trade = Trade {
                    venue: Venue::Coinbase,
                    side: if side.eq_ignore_ascii_case("buy") { "B" } else { "A" }.to_string(),
                    px: px.to_string(),
                    sz: sz.to_string(),
                    t: crate::state::now_ms(), // ISO time; receipt time is close enough
                    id: format!("cb-{}", m.trade_id.unwrap_or_else(crate::state::now_ms)),
                };
                reg.push_trades(market, vec![trade]);
            }
            _ => {}
        }
    }
}
