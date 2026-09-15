//! OKX v5 public websocket connector.
//!
//! Protocol (live-verified against wss://ws.okx.com:8443/ws/v5/public):
//! - subscribe: `{"op":"subscribe","args":[{"channel":"books5","instId":"ETH-USDT"},...]}`
//! - books5: FULL top-5 snapshot pushes, ~100 ms:
//!   `{"arg":{...},"data":[{"asks":[["px","sz",..],..],"bids":[...],"ts":".."}]}`
//! - trades: `{"arg":{"channel":"trades","instId":..},"data":[{"tradeId","px","sz","side","ts"}]}`
//!   side is the TAKER side ("buy"/"sell").
//! - heartbeat: client sends the raw text `ping` every 20 s; the server
//!   replies with the raw text `pong` (NOT JSON).

use crate::state::Registry;
use futures_util::{SinkExt, StreamExt};
use ob_core::{FeedStatus, Market, Trade, Venue};
use rust_decimal::Decimal;
use std::sync::Arc;
use std::time::Duration;
use tokio::time::{interval_at, Instant};
use tokio_tungstenite::tungstenite::Message;

pub const OKX_WS_URL: &str = "wss://ws.okx.com:8443/ws/v5/public";

#[derive(serde::Deserialize)]
struct OkxArg {
    channel: String,
    inst_id: String,
}

#[derive(serde::Deserialize)]
struct OkxMsg {
    arg: OkxArg,
    data: serde_json::Value,
}

#[derive(serde::Deserialize)]
struct OkxBookData {
    #[serde(default)]
    bids: Vec<(String, String)>,
    #[serde(default)]
    asks: Vec<(String, String)>,
}

#[derive(serde::Deserialize)]
struct OkxTrade {
    #[serde(rename = "tradeId")]
    id: serde_json::Value,
    px: String,
    sz: String,
    side: String,
    ts: String,
}

fn market_for_inst(inst: &str) -> Option<Market> {
    match inst {
        "ETH-USDT" => Some(Market::Eth),
        "SOL-USDT" => Some(Market::Sol),
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
        reg.feed(Venue::Okx).set_status(FeedStatus::Connecting);
        match connect_once(&reg).await {
            Ok(reason) => {
                tracing::warn!("[okx] stream ended ({reason}); reconnecting in {backoff}s");
            }
            Err(e) => {
                tracing::warn!("[okx] error: {e}; reconnecting in {backoff}s");
            }
        }
        reg.feed(Venue::Okx).set_status(FeedStatus::Reconnecting);
        tokio::time::sleep(Duration::from_secs(backoff)).await;
        backoff = (backoff * 2).min(30);
    }
}

async fn connect_once(reg: &Arc<Registry>) -> Result<String, String> {
    let (ws, _resp) = crate::wsio::connect(OKX_WS_URL).await?;
    tracing::info!("[okx] connected to {OKX_WS_URL}");
    let (mut sink, mut stream) = ws.split();

    let sub = serde_json::json!({
        "op": "subscribe",
        "args": [
            { "channel": "books5", "instId": "ETH-USDT" },
            { "channel": "books5", "instId": "SOL-USDT" },
            { "channel": "trades", "instId": "ETH-USDT" },
            { "channel": "trades", "instId": "SOL-USDT" },
        ]
    });
    sink.send(Message::Text(sub.to_string().into()))
        .await
        .map_err(|e| format!("subscribe send failed: {e}"))?;
    tracing::info!("[okx] subscribed books5 + trades for ETH/SOL");

    let feed = reg.feed(Venue::Okx);
    let mut got_snapshot = false;
    let mut ping = interval_at(Instant::now() + Duration::from_secs(20), Duration::from_secs(20));

    loop {
        tokio::select! {
            _ = ping.tick() => {
                if sink.send(Message::Text("ping".into())).await.is_err() {
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
                if text == "pong" {
                    continue; // heartbeat reply (raw text, not JSON)
                }
                feed.record_msg();

                let m: OkxMsg = match serde_json::from_str(&text) {
                    Ok(m) => m,
                    Err(_) => continue, // subscribe acks, errors
                };
                let Some(market) = market_for_inst(&m.arg.inst_id) else { continue };

                match m.arg.channel.as_str() {
                    "books5" => {
                        if let Ok(books) = serde_json::from_value::<Vec<OkxBookData>>(m.data) {
                            for b in books {
                                reg.replace_venue(
                                    Venue::Okx,
                                    market,
                                    parse_levels(&b.bids),
                                    parse_levels(&b.asks),
                                );
                            }
                            if !got_snapshot {
                                got_snapshot = true;
                                feed.set_status(FeedStatus::Live);
                            }
                        }
                    }
                    "trades" => {
                        if let Ok(trades) = serde_json::from_value::<Vec<OkxTrade>>(m.data) {
                            let out: Vec<Trade> = trades
                                .into_iter()
                                .map(|t| Trade {
                                    venue: Venue::Okx,
                                    side: if t.side.eq_ignore_ascii_case("buy") {
                                        "B"
                                    } else {
                                        "A"
                                    }
                                    .to_string(),
                                    px: t.px,
                                    sz: t.sz,
                                    t: t.ts.parse().unwrap_or_else(|_| crate::state::now_ms()),
                                    id: format!("ok-{}", t.id.as_str().unwrap_or("?")),
                                })
                                .collect();
                            if !out.is_empty() {
                                reg.push_trades(market, out);
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
    }
}
