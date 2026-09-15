//! Lighter (zkLighter) websocket connector.
//!
//! Protocol (source: apidocs.lighter.xyz WS reference + live-verified
//! against wss://mainnet.zklighter.elliot.ai/stream):
//! - on connect the server sends `{"session_id":..,"type":"connected"}`; only
//!   then may the client subscribe.
//! - order book:  `{"type":"subscribe","channel":"order_book/{idx}"}`
//!   first push:  `{"type":"subscribed/order_book","channel":"order_book:{idx}","order_book":{"bids":[..],"asks":[..]}}`
//!                — FULL snapshot (mainnet books are ~800-1700 levels/side).
//!   subsequent:  `{"type":"update/order_book",...}` — INCREMENTAL deltas
//!                batched every ~50ms; upsert by price, size 0 removes.
//! - trades:      `{"type":"subscribe","channel":"trade/{idx}"}`
//!   pushes:      `{"type":"update/trade","channel":"trade:{idx}","trades":[Trade],"liquidation_trades":[Trade]}`
//!                Trade = {trade_id, price, size, is_maker_ask, timestamp(ms), ...}.
//!                Taker side: is_maker_ask=true -> taker bought ("B").
//! - heartbeat:   server sends `{"type":"ping"}` -> reply `{"type":"pong"}`.

use crate::state::Registry;
use futures_util::{SinkExt, StreamExt};
use ob_core::{FeedStatus, Market, Side, Trade, Venue};
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
struct LtTrade {
    #[serde(default)]
    trade_id: Option<u64>,
    price: String,
    size: String,
    #[serde(default)]
    is_maker_ask: Option<bool>,
    #[serde(default)]
    timestamp: u64,
}

#[derive(serde::Deserialize)]
struct LtMsg {
    #[serde(rename = "type")]
    kind: String,
    #[serde(default)]
    channel: Option<String>,
    #[serde(default)]
    order_book: Option<LtOrderBook>,
    #[serde(default)]
    trades: Vec<LtTrade>,
    #[serde(default)]
    liquidation_trades: Vec<LtTrade>,
}

fn market_for_channel(channel: &str) -> Option<Market> {
    // channels arrive as "order_book:{index}" / "trade:{index}"
    let idx: u16 = channel.rsplit(':').next()?.parse().ok()?;
    Market::ALL
        .iter()
        .copied()
        .find(|m| m.lighter_market_index() == idx)
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
        reg.feed(Venue::Lighter).set_status(FeedStatus::Connecting);
        match connect_once(&reg).await {
            Ok(reason) => {
                tracing::warn!("[lighter] stream ended ({reason}); reconnecting in {backoff}s");
            }
            Err(e) => {
                tracing::warn!("[lighter] error: {e}; reconnecting in {backoff}s");
            }
        }
        reg.feed(Venue::Lighter).set_status(FeedStatus::Reconnecting);
        tokio::time::sleep(Duration::from_secs(backoff)).await;
        backoff = (backoff * 2).min(30);
    }
}

async fn connect_once(reg: &Arc<Registry>) -> Result<String, String> {
    let (ws, _resp) = crate::wsio::connect(LT_WS_URL).await?;
    tracing::info!("[lighter] connected to {LT_WS_URL}");
    let (mut sink, mut stream) = ws.split();
    let feed = reg.feed(Venue::Lighter);

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
                        for market in Market::ALL {
                            let sub = serde_json::json!({
                                "type": "subscribe",
                                "channel": format!("trade/{}", market.lighter_market_index())
                            });
                            sink.send(Message::Text(sub.to_string().into()))
                                .await
                                .map_err(|e| format!("subscribe send failed: {e}"))?;
                        }
                        subscribed = true;
                        tracing::info!(
                            "[lighter] session accepted, subscribed to {} order books + {} trade channels",
                            Market::ALL.len(),
                            Market::ALL.len()
                        );
                    }
                    "ping" => {
                        if sink
                            .send(Message::Text("{\"type\":\"pong\"}".into()))
                            .await
                            .is_err()
                        {
                            return Err("pong send failed".into());
                        }
                        feed.record_msg();
                    }
                    "subscribed/trade" => {
                        feed.record_msg();
                    }
                    "update/trade" => {
                        if !subscribed {
                            continue;
                        }
                        let Some(channel) = m.channel.as_deref() else { continue };
                        let Some(market) = market_for_channel(channel) else { continue };
                        let mut trades: Vec<Trade> = Vec::with_capacity(m.trades.len());
                        for t in m.trades.iter().chain(m.liquidation_trades.iter()) {
                            // Taker side: if the maker was on the ask, the
                            // taker lifted the offer -> aggressive buy.
                            let side = if t.is_maker_ask.unwrap_or(false) { "B" } else { "A" };
                            trades.push(Trade {
                                venue: Venue::Lighter,
                                side: side.to_string(),
                                px: t.price.clone(),
                                sz: t.size.clone(),
                                t: t.timestamp,
                                id: t.trade_id.map(|i| i.to_string()).unwrap_or_else(|| {
                                    format!("{}-{}-{}", t.timestamp, t.price, t.size)
                                }),
                            });
                        }
                        if !trades.is_empty() {
                            reg.push_trades(market, trades);
                        }
                        feed.record_msg();
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
                            reg.replace_venue(
                                Venue::Lighter,
                                market,
                                parse_levels(&ob.bids),
                                parse_levels(&ob.asks),
                            );
                            if !got_snapshot {
                                got_snapshot = true;
                                feed.set_status(FeedStatus::Live);
                                tracing::info!("[lighter] {} snapshot: {} bids / {} asks",
                                    market.label(), ob.bids.len(), ob.asks.len());
                            }
                        } else {
                            // Incremental delta; create the book lazily in case
                            // the snapshot is still in flight.
                            reg.touch_venue(Venue::Lighter, market);
                            if !ob.bids.is_empty() {
                                reg.apply_venue_side(Venue::Lighter, market, Side::Bid, &parse_levels(&ob.bids));
                            }
                            if !ob.asks.is_empty() {
                                reg.apply_venue_side(Venue::Lighter, market, Side::Ask, &parse_levels(&ob.asks));
                            }
                        }
                        feed.record_msg();
                    }
                    other => {
                        tracing::debug!("[lighter] ignoring message type {other}");
                    }
                }
            }
        }
    }
}
