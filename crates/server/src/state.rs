//! Central book registry: shared state between venue connectors, the publish
//! loop and the HTTP/WS layer.

use ob_core::{
    compute_stats, consolidate, BookStats, ConsolidatedBook, FeedStatus, Market, VenueBook,
    VenueHealth, VenueState, WireEvent, WIRE_DEPTH,
};
use serde_json::json;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::sync::broadcast;

pub fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// Live telemetry for one venue's websocket feed.
pub struct VenueFeed {
    status: Mutex<FeedStatus>,
    total_msgs: AtomicU64,
    last_msg_ms: AtomicU64,
    ema_rate: Mutex<f32>,
}

impl VenueFeed {
    fn new() -> Self {
        Self {
            status: Mutex::new(FeedStatus::Connecting),
            total_msgs: AtomicU64::new(0),
            last_msg_ms: AtomicU64::new(0),
            ema_rate: Mutex::new(0.0),
        }
    }

    pub fn set_status(&self, s: FeedStatus) {
        *self.status.lock().unwrap() = s;
    }

    pub fn status(&self) -> FeedStatus {
        *self.status.lock().unwrap()
    }

    pub fn record_msg(&self) {
        self.total_msgs.fetch_add(1, Ordering::Relaxed);
        self.last_msg_ms.store(now_ms(), Ordering::Relaxed);
    }

    pub fn total(&self) -> u64 {
        self.total_msgs.load(Ordering::Relaxed)
    }

    pub fn last_msg_ms(&self) -> u64 {
        self.last_msg_ms.load(Ordering::Relaxed)
    }

    /// Track the message rate with a light EMA. Called at 10 Hz.
    fn tick_rate(&self, last_sample: &mut u64, last_ts: &mut u64) -> f32 {
        let now = now_ms();
        let total = self.total();
        let dt = (now - *last_ts).max(1) as f32 / 1000.0;
        let inst = (total.saturating_sub(*last_sample)) as f32 / dt;
        *last_sample = total;
        *last_ts = now;
        let mut ema = self.ema_rate.lock().unwrap();
        *ema = *ema * 0.85 + inst * 0.15;
        *ema
    }

    pub fn rate(&self) -> f32 {
        *self.ema_rate.lock().unwrap()
    }
}

struct MarketBook {
    hl: Option<VenueState>,
    lt: Option<VenueState>,
    dirty: bool,
}

/// The whole application state.
pub struct Registry {
    books: Mutex<HashMap<Market, MarketBook>>,
    pub hl: VenueFeed,
    pub lt: VenueFeed,
    tx: broadcast::Sender<String>,
    pub started_ms: u64,
}

impl Registry {
    pub fn new() -> Arc<Self> {
        let (tx, _) = broadcast::channel(512);
        let mut books = HashMap::new();
        for m in Market::ALL {
            books.insert(
                m,
                MarketBook {
                    hl: None,
                    lt: None,
                    dirty: false,
                },
            );
        }
        Arc::new(Self {
            books: Mutex::new(books),
            hl: VenueFeed::new(),
            lt: VenueFeed::new(),
            tx,
            started_ms: now_ms(),
        })
    }

    pub fn subscribe(&self) -> broadcast::Receiver<String> {
        self.tx.subscribe()
    }

    // -- connector write path ------------------------------------------------

    pub fn replace_hl(
        &self,
        market: Market,
        bids: Vec<(rust_decimal::Decimal, rust_decimal::Decimal, Option<u32>)>,
        asks: Vec<(rust_decimal::Decimal, rust_decimal::Decimal, Option<u32>)>,
    ) {
        let mut books = self.books.lock().unwrap();
        if let Some(mb) = books.get_mut(&market) {
            mb.hl.get_or_insert_with(VenueState::default).replace(bids, asks);
            mb.dirty = true;
        }
    }

    pub fn replace_lt(
        &self,
        market: Market,
        bids: Vec<(rust_decimal::Decimal, rust_decimal::Decimal, Option<u32>)>,
        asks: Vec<(rust_decimal::Decimal, rust_decimal::Decimal, Option<u32>)>,
    ) {
        let mut books = self.books.lock().unwrap();
        if let Some(mb) = books.get_mut(&market) {
            mb.lt.get_or_insert_with(VenueState::default).replace(bids, asks);
            mb.dirty = true;
        }
    }

    pub fn apply_lt_side(
        &self,
        market: Market,
        side: ob_core::Side,
        levels: &[(rust_decimal::Decimal, rust_decimal::Decimal, Option<u32>)],
    ) {
        let mut books = self.books.lock().unwrap();
        if let Some(mb) = books.get_mut(&market) {
            if let Some(st) = mb.lt.as_mut() {
                st.apply_side(side, levels);
                mb.dirty = true;
            }
        }
    }

    /// Ensure an (empty) Lighter book exists so updates have a home even if
    /// the snapshot arrives late.
    pub fn touch_lt(&self, market: Market) {
        let mut books = self.books.lock().unwrap();
        if let Some(mb) = books.get_mut(&market) {
            mb.lt.get_or_insert_with(VenueState::default);
        }
    }

    // -- read path -----------------------------------------------------------

    /// Build the full wire event for one market (fully owned, cheap to hold).
    fn build_event(&self, market: Market) -> Option<WireEvent> {
        let now = now_ms();
        let books = self.books.lock().unwrap();
        let mb = books.get(&market)?;

        let venue_book = |st: &Option<VenueState>, feed: &VenueFeed| -> Option<VenueBook> {
            let st = st.as_ref()?;
            Some(VenueBook {
                health: VenueHealth {
                    status: feed.status(),
                    msg_per_sec: feed.rate(),
                    last_msg_age_ms: now.saturating_sub(feed.last_msg_ms()),
                    levels: st.total_levels(),
                },
                bids: st.top_bids(WIRE_DEPTH),
                asks: st.top_asks(WIRE_DEPTH),
            })
        };

        let consolidated = match (mb.hl.as_ref(), mb.lt.as_ref()) {
            (Some(h), Some(l)) => {
                let (bids, asks) = consolidate(h, l, WIRE_DEPTH);
                Some(ConsolidatedBook { bids, asks })
            }
            _ => None,
        };

        let stats: BookStats = compute_stats(mb.hl.as_ref(), mb.lt.as_ref());

        Some(WireEvent::Book {
            market,
            hyperliquid: venue_book(&mb.hl, &self.hl),
            lighter: venue_book(&mb.lt, &self.lt),
            consolidated,
            stats,
            ts: now,
        })
    }

    pub fn book_json(&self, market: Market) -> Option<String> {
        serde_json::to_string(&self.build_event(market)?).ok()
    }

    /// JSON payload for GET /api/health.
    pub fn health_payload(&self) -> serde_json::Value {
        let now = now_ms();
        let feed = |f: &VenueFeed| {
            json!({
                "status": f.status().label(),
                "msgs_per_sec": (f.rate() * 10.0).round() / 10.0,
                "total_msgs": f.total(),
                "last_msg_age_ms": now.saturating_sub(f.last_msg_ms()),
            })
        };
        let books = self.books.lock().unwrap();
        let markets: Vec<serde_json::Value> = Market::ALL
            .iter()
            .map(|m| {
                let mb = books.get(m);
                json!({
                    "market": m.label(),
                    "hyperliquid_levels": mb.and_then(|b| b.hl.as_ref()).map(|s| s.total_levels()).unwrap_or(0),
                    "lighter_levels": mb.and_then(|b| b.lt.as_ref()).map(|s| s.total_levels()).unwrap_or(0),
                })
            })
            .collect();
        json!({
            "uptime_ms": now.saturating_sub(self.started_ms),
            "hyperliquid": feed(&self.hl),
            "lighter": feed(&self.lt),
            "markets": markets,
        })
    }

    /// Coalescing publisher: caps browser update rate at 10 Hz per market,
    /// merging bursts of venue deltas into one snapshot per tick.
    pub async fn publish_loop(self: Arc<Self>) {
        let mut tick = tokio::time::interval(std::time::Duration::from_millis(100));
        tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        let mut hl_s = 0u64;
        let mut hl_t = now_ms();
        let mut lt_s = 0u64;
        let mut lt_t = hl_t;
        let mut heartbeat = 0u32;

        loop {
            tick.tick().await;
            // EMA bookkeeping (10 Hz samples).
            self.hl.tick_rate(&mut hl_s, &mut hl_t);
            self.lt.tick_rate(&mut lt_s, &mut lt_t);

            let mut pending: Vec<Market> = Vec::with_capacity(4);
            {
                let mut books = self.books.lock().unwrap();
                for (m, mb) in books.iter_mut() {
                    if mb.dirty {
                        mb.dirty = false;
                        pending.push(*m);
                    }
                }
            }
            // build + serialize outside the lock where possible
            for m in pending {
                if let Some(ev) = self.build_event(m) {
                    if let Ok(json) = serde_json::to_string(&ev) {
                        let _ = self.tx.send(json);
                    }
                }
            }

            heartbeat += 1;
            if heartbeat % 20 == 0 {
                let st = WireEvent::Status {
                    hyperliquid: self.hl.status(),
                    lighter: self.lt.status(),
                    ts: now_ms(),
                };
                if let Ok(json) = serde_json::to_string(&st) {
                    let _ = self.tx.send(json);
                }
            }
        }
    }
}
