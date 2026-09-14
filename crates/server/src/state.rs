//! Central book registry: shared state between venue connectors, the publish
//! loop, the depth sampler and the HTTP/WS layer.
//!
//! Scaling model (12 markets): every socket selects ONE market. The broadcast
//! channel carries a compact `Broadcast` envelope — (kind, market, json) — so
//! each socket can route events without re-parsing JSON:
//! - `Book`   → forwarded only to sockets whose selection matches (full depth)
//! - `Ticker` → forwarded to everyone (compact, coalesced at 2 Hz)
//! - `Trades` → forwarded to everyone (batches, small)
//! - `AlertSet` / `AlertFired` / `Status` → forwarded to everyone

use ob_core::{
    compute_stats, consolidate, cross_mid, Alert, AlertDir, BookStats, ConsolidatedBook,
    DepthSample, FeedStatus, Market, Trade, VenueBook, VenueHealth, VenueState, WireEvent,
    SAMPLE_SECS, SAMPLE_WINDOW, TAPE_BACKLOG, WIRE_DEPTH,
};
use rust_decimal::prelude::ToPrimitive;
use rust_decimal::Decimal;
use serde_json::json;
use std::collections::{HashMap, HashSet, VecDeque};
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

/// Kind tag on the internal broadcast envelope.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BcKind {
    Book,
    Ticker,
    Trades,
    AlertSet,
    AlertFired,
    Status,
}

impl BcKind {
    /// Should a socket with selection `sel` receive an event about `market`?
    pub fn wants(self, sel: Market, market: Market) -> bool {
        match self {
            BcKind::Book => sel == market,
            _ => true,
        }
    }
}

/// Pre-serialized event + routing tag (one serialization, N sockets).
#[derive(Clone)]
pub struct Broadcast {
    pub kind: BcKind,
    pub market: Market,
    pub json: String,
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

/// Per-market trade tape: backlog for new sockets + dedup + publish pending.
#[derive(Default)]
struct TapeState {
    /// Newest last.
    trades: VecDeque<Trade>,
    seen: HashSet<String>,
    seen_order: VecDeque<String>,
    /// New trades since the last publish flush.
    pending: Vec<Trade>,
}

/// The whole application state.
pub struct Registry {
    books: Mutex<HashMap<Market, MarketBook>>,
    pub hl: VenueFeed,
    pub lt: VenueFeed,
    tx: broadcast::Sender<Broadcast>,
    /// Number of connected sockets currently showing each market.
    selections: Mutex<HashMap<Market, u32>>,
    tapes: Mutex<HashMap<Market, TapeState>>,
    history: Mutex<HashMap<Market, VecDeque<DepthSample>>>,
    alerts: Mutex<Vec<Alert>>,
    next_alert_id: AtomicU64,
    pub started_ms: u64,
}

impl Registry {
    pub fn new() -> Arc<Self> {
        let (tx, _) = broadcast::channel(1024);
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
            selections: Mutex::new(HashMap::new()),
            tapes: Mutex::new(HashMap::new()),
            history: Mutex::new(HashMap::new()),
            alerts: Mutex::new(Vec::new()),
            next_alert_id: AtomicU64::new(1),
            started_ms: now_ms(),
        })
    }

    pub fn subscribe(&self) -> broadcast::Receiver<Broadcast> {
        self.tx.subscribe()
    }

    fn broadcast(&self, kind: BcKind, market: Market, ev: &WireEvent) {
        if let Ok(json) = serde_json::to_string(ev) {
            let _ = self.tx.send(Broadcast { kind, market, json });
        }
    }

    // -- socket selection ------------------------------------------------------

    pub fn select_market(&self, m: Market) {
        *self.selections.lock().unwrap().entry(m).or_insert(0) += 1;
    }

    pub fn deselect_market(&self, m: Market) {
        let mut sel = self.selections.lock().unwrap();
        if let Some(c) = sel.get_mut(&m) {
            *c = c.saturating_sub(1);
            if *c == 0 {
                sel.remove(&m);
            }
        }
    }

    // -- connector write path --------------------------------------------------

    pub fn replace_hl(
        &self,
        market: Market,
        bids: Vec<(Decimal, Decimal, Option<u32>)>,
        asks: Vec<(Decimal, Decimal, Option<u32>)>,
    ) {
        let mut books = self.books.lock().unwrap();
        if let Some(mb) = books.get_mut(&market) {
            mb.hl
                .get_or_insert_with(VenueState::default)
                .replace(bids, asks);
            mb.dirty = true;
        }
    }

    pub fn replace_lt(
        &self,
        market: Market,
        bids: Vec<(Decimal, Decimal, Option<u32>)>,
        asks: Vec<(Decimal, Decimal, Option<u32>)>,
    ) {
        let mut books = self.books.lock().unwrap();
        if let Some(mb) = books.get_mut(&market) {
            mb.lt
                .get_or_insert_with(VenueState::default)
                .replace(bids, asks);
            mb.dirty = true;
        }
    }

    pub fn apply_lt_side(
        &self,
        market: Market,
        side: ob_core::Side,
        levels: &[(Decimal, Decimal, Option<u32>)],
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

    // -- trade tape --------------------------------------------------------------

    /// Push venue trades into the tape (dedup by venue id, keep a backlog).
    pub fn push_trades(&self, market: Market, trades: Vec<Trade>) {
        let mut tapes = self.tapes.lock().unwrap();
        let st = tapes.entry(market).or_default();
        for t in trades {
            if st.seen.contains(&t.id) {
                continue;
            }
            st.seen.insert(t.id.clone());
            st.seen_order.push_back(t.id.clone());
            // Bound the dedup set to ~2x the backlog (evict in arrival order).
            while st.seen_order.len() > 2 * TAPE_BACKLOG {
                if let Some(old) = st.seen_order.pop_front() {
                    st.seen.remove(&old);
                }
            }
            st.trades.push_back(t.clone());
            while st.trades.len() > TAPE_BACKLOG {
                st.trades.pop_front();
            }
            st.pending.push(t);
        }
    }

    /// Drain trades pending publication (called by the publish loop at 10 Hz).
    fn take_pending(&self) -> Vec<(Market, Vec<Trade>)> {
        let mut tapes = self.tapes.lock().unwrap();
        tapes
            .iter_mut()
            .filter(|(_, st)| !st.pending.is_empty())
            .map(|(m, st)| (*m, std::mem::take(&mut st.pending)))
            .collect()
    }

    /// Recent trades for a market, oldest first (for new sockets).
    pub fn tape_backlog(&self, market: Market) -> Vec<Trade> {
        self.tapes
            .lock()
            .unwrap()
            .get(&market)
            .map(|st| st.trades.iter().cloned().collect())
            .unwrap_or_default()
    }

    // -- depth history -----------------------------------------------------------

    /// Sample depth bands for every market with live book data. Called every
    /// `SAMPLE_SECS` by a dedicated task; keeps a `SAMPLE_WINDOW` ring buffer.
    pub fn sample_all(&self) {
        let now = now_ms();
        // Band thresholds: 0.1%, 0.5%, 1%, 2%.
        let ths = [
            Decimal::new(1, 3),
            Decimal::new(5, 3),
            Decimal::new(1, 2),
            Decimal::new(2, 2),
        ];
        let mut samples: Vec<(Market, DepthSample)> = Vec::new();
        {
            let books = self.books.lock().unwrap();
            for (m, mb) in books.iter() {
                let (hl, lt) = (mb.hl.as_ref(), mb.lt.as_ref());
                if hl.is_none() && lt.is_none() {
                    continue;
                }
                let Some(mid) = cross_mid(hl, lt) else { continue };

                let mut bands = [Decimal::ZERO; 4];
                let mut asks = [Decimal::ZERO; 4];
                for st in [hl, lt].into_iter().flatten() {
                    let b = st.band_notionals(mid, &ths, ob_core::Side::Bid);
                    let a = st.band_notionals(mid, &ths, ob_core::Side::Ask);
                    for i in 0..4 {
                        bands[i] += b[i];
                        asks[i] += a[i];
                    }
                }

                // Spread over the cross-venue touch.
                let bb = [hl.and_then(|b| b.best_bid()), lt.and_then(|b| b.best_bid())]
                    .into_iter()
                    .flatten()
                    .max_by(|x, y| x.0.cmp(&y.0));
                let ba = [hl.and_then(|b| b.best_ask()), lt.and_then(|b| b.best_ask())]
                    .into_iter()
                    .flatten()
                    .min_by(|x, y| x.0.cmp(&y.0));
                let sb = match (bb, ba) {
                    (Some(bb), Some(ba)) => {
                        if mid.is_zero() {
                            Decimal::ZERO
                        } else {
                            (ba.0 - bb.0) / mid * Decimal::from(10_000u64)
                        }
                    }
                    _ => Decimal::ZERO,
                };

                let total = bands[1] + asks[1];
                let imb = if total.is_zero() {
                    Decimal::ZERO
                } else {
                    (bands[1] - asks[1]) / total
                };

                let f = |d: Decimal| d.to_f64().unwrap_or(0.0);
                samples.push((
                    *m,
                    DepthSample {
                        t: now,
                        mid: f(mid),
                        sb: f(sb.round_dp(2)),
                        b1: f(bands[0]),
                        a1: f(asks[0]),
                        b5: f(bands[1]),
                        a5: f(asks[1]),
                        b10: f(bands[2]),
                        a10: f(asks[2]),
                        b20: f(bands[3]),
                        a20: f(asks[3]),
                        im: f(imb.round_dp(4)),
                    },
                ));
            }
        }
        if samples.is_empty() {
            return;
        }
        let mut hist = self.history.lock().unwrap();
        for (m, s) in samples {
            let q = hist.entry(m).or_default();
            // Skip duplicate timestamps (same sample window).
            if q.back().map(|b| b.t == s.t).unwrap_or(false) {
                continue;
            }
            q.push_back(s);
            while q.len() > SAMPLE_WINDOW {
                q.pop_front();
            }
        }
    }

    /// Full history window for a market, oldest first.
    pub fn history_snapshot(&self, market: Market) -> Vec<DepthSample> {
        self.history
            .lock()
            .unwrap()
            .get(&market)
            .map(|q| q.iter().copied().collect())
            .unwrap_or_default()
    }

    /// History samples strictly newer than `t`.
    pub fn history_after(&self, market: Market, t: u64) -> Vec<DepthSample> {
        self.history
            .lock()
            .unwrap()
            .get(&market)
            .map(|q| q.iter().filter(|s| s.t > t).copied().collect())
            .unwrap_or_default()
    }

    // -- alerts ------------------------------------------------------------------

    /// Create an alert (validates the price parses as a positive decimal).
    pub fn create_alert(&self, market: Market, dir: AlertDir, price: String) -> Result<Alert, String> {
        let th: Decimal = price
            .parse()
            .map_err(|_| format!("price {price:?} is not a valid decimal"))?;
        if !th.is_sign_positive() {
            return Err("price must be positive".into());
        }
        let alert = Alert {
            id: self.next_alert_id.fetch_add(1, Ordering::Relaxed),
            market,
            dir,
            price: th.normalize().to_string(),
            created_ms: now_ms(),
            triggered_ms: None,
            triggered_px: None,
        };
        let mut alerts = self.alerts.lock().unwrap();
        alerts.push(alert.clone());
        drop(alerts);
        self.broadcast(
            BcKind::AlertSet,
            alert.market,
            &WireEvent::AlertSet {
                alerts: self.alerts_snapshot(),
            },
        );
        Ok(alert)
    }

    pub fn delete_alert(&self, id: u64) -> bool {
        let removed = {
            let mut alerts = self.alerts.lock().unwrap();
            let before = alerts.len();
            alerts.retain(|a| a.id != id);
            before != alerts.len()
        };
        if removed {
            self.broadcast(
                BcKind::AlertSet,
                Market::Eth,
                &WireEvent::AlertSet {
                    alerts: self.alerts_snapshot(),
                },
            );
        }
        removed
    }

    pub fn alerts_snapshot(&self) -> Vec<Alert> {
        self.alerts.lock().unwrap().clone()
    }

    /// Check every active alert against the live cross-venue mid. Returns the
    /// alerts that fired during this pass (already mutated in place).
    fn check_alerts(&self) -> Vec<Alert> {
        let now = now_ms();
        let mut fired = Vec::new();
        {
            let books = self.books.lock().unwrap();
            let mut alerts = self.alerts.lock().unwrap();
            for a in alerts.iter_mut() {
                if a.triggered_ms.is_some() {
                    continue;
                }
                let Ok(th) = a.price.parse::<Decimal>() else {
                    continue;
                };
                let Some(mb) = books.get(&a.market) else {
                    continue;
                };
                let Some(mid) = cross_mid(mb.hl.as_ref(), mb.lt.as_ref()) else {
                    continue;
                };
                let hit = match a.dir {
                    AlertDir::Above => mid >= th,
                    AlertDir::Below => mid <= th,
                };
                if hit {
                    a.triggered_ms = Some(now);
                    a.triggered_px = Some(mid.normalize().to_string());
                    fired.push(a.clone());
                }
            }
        }
        for a in &fired {
            tracing::info!(
                "[alerts] #{} {} {} {} crossed (mid {})",
                a.id,
                a.market.label(),
                a.dir.label(),
                a.price,
                a.triggered_px.as_deref().unwrap_or("?")
            );
        }
        fired
    }

    // -- read path ---------------------------------------------------------------

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

    /// Compact ticker JSON for one market (nil when no book yet).
    pub fn ticker_json(&self, market: Market) -> Option<String> {
        let stats = {
            let books = self.books.lock().unwrap();
            let mb = books.get(&market)?;
            compute_stats(mb.hl.as_ref(), mb.lt.as_ref())
        };
        let ev = WireEvent::Ticker {
            market,
            mid: stats.mid,
            spread_bps: stats.spread_bps,
            imbalance: stats.imbalance,
            ts: now_ms(),
        };
        serde_json::to_string(&ev).ok()
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
        let tapes = self.tapes.lock().unwrap();
        let history = self.history.lock().unwrap();
        let selections = self.selections.lock().unwrap();
        let markets: Vec<serde_json::Value> = Market::ALL
            .iter()
            .map(|m| {
                let mb = books.get(m);
                json!({
                    "market": m.label(),
                    "hyperliquid_levels": mb.and_then(|b| b.hl.as_ref()).map(|s| s.total_levels()).unwrap_or(0),
                    "lighter_levels": mb.and_then(|b| b.lt.as_ref()).map(|s| s.total_levels()).unwrap_or(0),
                    "tape_trades": tapes.get(m).map(|t| t.trades.len()).unwrap_or(0),
                    "history_samples": history.get(m).map(|q| q.len()).unwrap_or(0),
                    "viewers": selections.get(m).copied().unwrap_or(0),
                })
            })
            .collect();
        json!({
            "uptime_ms": now.saturating_sub(self.started_ms),
            "hyperliquid": feed(&self.hl),
            "lighter": feed(&self.lt),
            "active_alerts": self.alerts.lock().unwrap().iter().filter(|a| a.triggered_ms.is_none()).count(),
            "markets": markets,
        })
    }

    /// Coalescing publisher:
    /// - 10 Hz: full `Book` for selected markets, `Trades` batches, alert checks
    /// - 2 Hz:  compact `Ticker` for every dirty market
    /// - 0.5 Hz: `Status` heartbeat
    pub async fn publish_loop(self: Arc<Self>) {
        let mut tick100 =
            tokio::time::interval(std::time::Duration::from_millis(100));
        tick100.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        let mut tick500 =
            tokio::time::interval(std::time::Duration::from_millis(500));
        tick500.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        let mut hl_s = 0u64;
        let mut hl_t = now_ms();
        let mut lt_s = 0u64;
        let mut lt_t = hl_t;
        let mut heartbeat = 0u32;
        // (mid, spread_bps, imbalance) waiting for the 2 Hz ticker flush.
        let mut ticker_accum: HashMap<Market, (String, String, String)> = HashMap::new();

        loop {
            tokio::select! {
                _ = tick100.tick() => {
                    // EMA bookkeeping (10 Hz samples).
                    self.hl.tick_rate(&mut hl_s, &mut hl_t);
                    self.lt.tick_rate(&mut lt_s, &mut lt_t);

                    // 1) Dirty books -> full Book events (selected markets only)
                    //    + accumulate tickers for everyone.
                    let mut built: Vec<(Market, WireEvent)> = Vec::with_capacity(4);
                    {
                        let mut books = self.books.lock().unwrap();
                        let selections = self.selections.lock().unwrap();
                        for (m, mb) in books.iter_mut() {
                            if !mb.dirty {
                                continue;
                            }
                            mb.dirty = false;
                            let stats = compute_stats(mb.hl.as_ref(), mb.lt.as_ref());
                            ticker_accum.insert(
                                *m,
                                (stats.mid.clone(), stats.spread_bps.clone(), stats.imbalance.clone()),
                            );
                            if selections.get(m).copied().unwrap_or(0) > 0 {
                                if let Some(ev) = build_book_event(*m, mb, &stats, &self.hl, &self.lt) {
                                    built.push((*m, ev));
                                }
                            }
                        }
                    }
                    for (_, ev) in built {
                        self.broadcast(BcKind::Book, market_of(&ev), &ev);
                    }

                    // 2) Flush trade batches.
                    for (market, trades) in self.take_pending() {
                        self.broadcast(BcKind::Trades, market, &WireEvent::Trades { market, trades });
                    }

                    // 3) Alert engine.
                    for a in self.check_alerts() {
                        self.broadcast(BcKind::AlertFired, a.market, &WireEvent::AlertFired { alert: a });
                    }
                }
                _ = tick500.tick() => {
                    // Ticker flush.
                    if !ticker_accum.is_empty() {
                        let now = now_ms();
                        let entries: Vec<(Market, (String, String, String))> =
                            ticker_accum.drain().collect();
                        for (market, (mid, spread_bps, imbalance)) in entries {
                            let ev = WireEvent::Ticker { market, mid, spread_bps, imbalance, ts: now };
                            self.broadcast(BcKind::Ticker, market, &ev);
                        }
                    }

                    // Heartbeat.
                    heartbeat += 1;
                    if heartbeat % 4 == 0 {
                        let st = WireEvent::Status {
                            hyperliquid: self.hl.status(),
                            lighter: self.lt.status(),
                            ts: now_ms(),
                        };
                        self.broadcast(BcKind::Status, Market::Eth, &st);
                    }
                }
            }
        }
    }

    /// History sampler loop (spawned by main).
    pub async fn history_loop(self: Arc<Self>) {
        let mut tick = tokio::time::interval(std::time::Duration::from_secs(SAMPLE_SECS));
        tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            tick.tick().await;
            self.sample_all();
        }
    }
}

/// Extract the market tag from a Book wire event (for routing).
fn market_of(ev: &WireEvent) -> Market {
    match ev {
        WireEvent::Book { market, .. } => *market,
        WireEvent::Ticker { market, .. }
        | WireEvent::Trades { market, .. }
        | WireEvent::History { market, .. } => *market,
        WireEvent::AlertFired { alert } => alert.market,
        _ => Market::Eth,
    }
}

/// Build the full Book event from an already-locked MarketBook + precomputed
/// stats (shares the health snapshot with the caller).
fn build_book_event(
    market: Market,
    mb: &MarketBook,
    stats: &BookStats,
    hl_feed: &VenueFeed,
    lt_feed: &VenueFeed,
) -> Option<WireEvent> {
    let now = now_ms();
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
    Some(WireEvent::Book {
        market,
        hyperliquid: venue_book(&mb.hl, hl_feed),
        lighter: venue_book(&mb.lt, lt_feed),
        consolidated,
        stats: stats.clone(),
        ts: now,
    })
}
