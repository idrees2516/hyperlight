//! Hyperliquid websocket connector.
//!
//! Protocol (live-verified against wss://api.hyperliquid.xyz/ws):
//! - subscribe: `{"method":"subscribe","subscription":{"type":"l2Book","coin":"ETH"}}`
//! - pushes:    `{"channel":"l2Book","data":{"coin":"ETH","time":ms,"levels":[bids,asks]}}`
//!              every push is a FULL snapshot; level = `{"px","sz","n"}`.
//! - trades:    `{"method":"subscribe","subscription":{"type":"trades","coin":"SOL"}}`
//!              pushes `{"channel":"trades","data":[{"coin","side":"B|A","px","sz","time":ms,"tid":u64}]}`
//! - heartbeat: client sends `{"method":"ping"}` -> server replies `{"channel":"pong"}`.

use crate::state::Registry;
use futures_util::{SinkExt, StreamExt};
use ob_core::{FeedStatus, Market, Trade, Venue};
use rust_decimal::Decimal;
use std::sync::Arc;
use std::time::Duration;
use tokio::time::{interval_at, Instant};
use tokio_tungstenite::tungstenite::Message;

pub const HL_WS_URL: &str = "wss://api.hyperliquid.xyz/ws";

#[derive(serde::Deserialize)]
struct HlLevel {
    px: String,
    sz: String,
    #[serde(default)]
    n: Option<u32>,
}

#[derive(serde::Deserialize)]
struct HlBookData {
    coin: String,
    #[serde(default)]
    #[allow(dead_code)]
    time: u64,
    levels: (Vec<HlLevel>, Vec<HlLevel>),
}

#[derive(serde::Deserialize)]
struct HlTrade {
    coin: String,
    side: String,
    px: String,
    sz: String,
    #[serde(default)]
    time: u64,
    #[serde(default)]
    tid: Option<u64>,
    #[serde(default)]
    hash: Option<String>,
}

fn market_for_coin(coin: &str) -> Option<Market> {
    Market::ALL.iter().copied().find(|m| m.hyperliquid_coin() == coin)
}

fn parse_levels(raw: &[HlLevel]) -> Vec<(Decimal, Decimal, Option<u32>)> {
    raw.iter()
        .filter_map(|l| {
            let px: Decimal = l.px.parse().ok()?;
            let sz: Decimal = l.sz.parse().ok()?;
            Some((px, sz, l.n))
        })
        .collect()
}

/// Run forever: connect, subscribe, stream, reconnect with backoff.
pub async fn run(reg: Arc<Registry>) {
    let mut backoff = 1u64;
    loop {
        reg.hl.set_status(FeedStatus::Connecting);
        match connect_once(&reg).await {
            Ok(reason) => {
                tracing::warn!("[hyperliquid] stream ended ({reason}); reconnecting in {backoff}s");
            }
            Err(e) => {
                tracing::warn!("[hyperliquid] error: {e}; reconnecting in {backoff}s");
            }
        }
        reg.hl.set_status(FeedStatus::Reconnecting);
        tokio::time::sleep(Duration::from_secs(backoff)).await;
        backoff = (backoff * 2).min(30);
    }
}

async fn connect_once(reg: &Arc<Registry>) -> Result<String, String> {
    let (ws, _resp) = tokio_tungstenite::connect_async(HL_WS_URL)
        .await
        .map_err(|e| format!("connect failed: {e}"))?;
    tracing::info!("[hyperliquid] connected to {HL_WS_URL}");
    let (mut sink, mut stream) = ws.split();

    for m in Market::ALL {
        let sub = serde_json::json!({
            "method": "subscribe",
            "subscription": { "type": "l2Book", "coin": m.hyperliquid_coin() }
        });
        sink.send(Message::Text(sub.to_string().into()))
            .await
            .map_err(|e| format!("subscribe send failed: {e}"))?;
    }
    for m in Market::ALL {
        let sub = serde_json::json!({
            "method": "subscribe",
            "subscription": { "type": "trades", "coin": m.hyperliquid_coin() }
        });
        sink.send(Message::Text(sub.to_string().into()))
            .await
            .map_err(|e| format!("subscribe send failed: {e}"))?;
    }
    tracing::info!(
        "[hyperliquid] subscribed l2Book + trades for {} coins",
        Market::ALL.len()
    );

    let mut ping = interval_at(Instant::now() + Duration::from_secs(20), Duration::from_secs(20));
    let mut got_snapshot = false;
    let mut backoff_reset = false;

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

                let v: serde_json::Value = match serde_json::from_str(&text) {
                    Ok(v) => v,
                    Err(_) => continue,
                };
                match v.get("channel").and_then(|c| c.as_str()) {
                    Some("l2Book") => {
                        let data: HlBookData = match serde_json::from_value(v["data"].clone()) {
                            Ok(d) => d,
                            Err(e) => {
                                tracing::debug!("[hyperliquid] bad l2Book payload: {e}");
                                continue;
                            }
                        };
                        let Some(market) = market_for_coin(&data.coin) else { continue };
                        let bids = parse_levels(&data.levels.0);
                        let asks = parse_levels(&data.levels.1);
                        reg.replace_hl(market, bids, asks);
                        reg.hl.record_msg();
                        if !got_snapshot {
                            got_snapshot = true;
                            reg.hl.set_status(FeedStatus::Live);
                        }
                        if !backoff_reset {
                            backoff_reset = true;
                        }
                    }
                    Some("trades") => {
                        let data: Vec<HlTrade> = match serde_json::from_value(v["data"].clone()) {
                            Ok(d) => d,
                            Err(e) => {
                                tracing::debug!("[hyperliquid] bad trades payload: {e}");
                                continue;
                            }
                        };
                        // Route each trade to its market; batch per market.
                        let mut by_market: Vec<(Market, Vec<Trade>)> = Vec::new();
                        for t in &data {
                            let Some(market) = market_for_coin(&t.coin) else { continue };
                            let id = t
                                .tid
                                .map(|id| id.to_string())
                                .or_else(|| t.hash.clone())
                                .unwrap_or_else(|| format!("{}-{}-{}", t.coin, t.time, t.px));
                            let trade = Trade {
                                venue: Venue::Hyperliquid,
                                side: t.side.clone(),
                                px: t.px.clone(),
                                sz: t.sz.clone(),
                                t: t.time,
                                id,
                            };
                            match by_market.iter_mut().find(|(m, _)| *m == market) {
                                Some((_, v)) => v.push(trade),
                                None => by_market.push((market, vec![trade])),
                            }
                        }
                        for (market, trades) in by_market {
                            reg.push_trades(market, trades);
                        }
                        reg.hl.record_msg();
                    }
                    Some("pong") => {}
                    _ => {
                        // Some deployments send {"method":"ping"}; answer it.
                        if v.get("method").and_then(|m| m.as_str()) == Some("ping") {
                            let _ = sink
                                .send(Message::Text("{\"method\":\"pong\"}".into()))
                                .await;
                        }
                    }
                }
            }
        }
    }
}
