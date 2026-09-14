//! # ob-core
//!
//! Shared, platform-agnostic types for the HyperLight orderbook terminal.
//!
//! This crate is compiled into BOTH the Axum backend (native) and the Leptos
//! frontend (wasm32), so it must stay free of any I/O or runtime dependencies:
//! only `serde`, `serde_json` and `rust_decimal`.
//!
//! Everything price/size related uses `rust_decimal::Decimal` so that no
//! floating point error can ever creep into displayed numbers.

use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// A venue identifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Venue {
    Hyperliquid,
    Lighter,
}

impl Venue {
    pub const fn label(self) -> &'static str {
        match self {
            Venue::Hyperliquid => "Hyperliquid",
            Venue::Lighter => "Lighter",
        }
    }
}

/// A tradable market, mapped on both venues.
///
/// Lighter indices are live-verified against
/// `GET https://mainnet.zklighter.elliot.ai/api/v1/orderBooks`
/// (0:ETH 1:BTC 2:SOL 3:DOGE 4:1000PEPE 5:WIF 6:WLD 7:XRP 8:LINK 9:AVAX
///  10:NEAR 11:DOT). Hyperliquid coins are live-verified against
/// `POST /info {"type":"meta"}` (kPEPE = 1000PEPE on Hyperliquid).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Market {
    Eth,
    Btc,
    Sol,
    Doge,
    Pepe1000,
    Wif,
    Wld,
    Xrp,
    Link,
    Avax,
    Near,
    Dot,
}

impl Market {
    /// Ticker used on Hyperliquid.
    pub const fn hyperliquid_coin(self) -> &'static str {
        match self {
            Market::Eth => "ETH",
            Market::Btc => "BTC",
            Market::Sol => "SOL",
            Market::Doge => "DOGE",
            Market::Pepe1000 => "kPEPE",
            Market::Wif => "WIF",
            Market::Wld => "WLD",
            Market::Xrp => "XRP",
            Market::Link => "LINK",
            Market::Avax => "AVAX",
            Market::Near => "NEAR",
            Market::Dot => "DOT",
        }
    }

    /// Numeric market index used on Lighter mainnet (live-verified).
    pub const fn lighter_market_index(self) -> u16 {
        match self {
            Market::Eth => 0,
            Market::Btc => 1,
            Market::Sol => 2,
            Market::Doge => 3,
            Market::Pepe1000 => 4,
            Market::Wif => 5,
            Market::Wld => 6,
            Market::Xrp => 7,
            Market::Link => 8,
            Market::Avax => 9,
            Market::Near => 10,
            Market::Dot => 11,
        }
    }

    pub const fn label(self) -> &'static str {
        match self {
            Market::Eth => "ETH-USD",
            Market::Btc => "BTC-USD",
            Market::Sol => "SOL-USD",
            Market::Doge => "DOGE-USD",
            Market::Pepe1000 => "1000PEPE-USD",
            Market::Wif => "WIF-USD",
            Market::Wld => "WLD-USD",
            Market::Xrp => "XRP-USD",
            Market::Link => "LINK-USD",
            Market::Avax => "AVAX-USD",
            Market::Near => "NEAR-USD",
            Market::Dot => "DOT-USD",
        }
    }

    /// Parse from the serde slug used on the wire ("eth", "pepe1000", ...).
    pub fn from_slug(s: &str) -> Option<Self> {
        Some(match s {
            "eth" => Market::Eth,
            "btc" => Market::Btc,
            "sol" => Market::Sol,
            "doge" => Market::Doge,
            "pepe1000" | "pepe" => Market::Pepe1000,
            "wif" => Market::Wif,
            "wld" => Market::Wld,
            "xrp" => Market::Xrp,
            "link" => Market::Link,
            "avax" => Market::Avax,
            "near" => Market::Near,
            "dot" => Market::Dot,
            _ => return None,
        })
    }

    /// Compact short name for tight UI surfaces.
    pub const fn short(self) -> &'static str {
        match self {
            Market::Eth => "ETH",
            Market::Btc => "BTC",
            Market::Sol => "SOL",
            Market::Doge => "DOGE",
            Market::Pepe1000 => "1000PEPE",
            Market::Wif => "WIF",
            Market::Wld => "WLD",
            Market::Xrp => "XRP",
            Market::Link => "LINK",
            Market::Avax => "AVAX",
            Market::Near => "NEAR",
            Market::Dot => "DOT",
        }
    }

    pub const ALL: [Market; 12] = [
        Market::Eth,
        Market::Btc,
        Market::Sol,
        Market::Doge,
        Market::Pepe1000,
        Market::Wif,
        Market::Wld,
        Market::Xrp,
        Market::Link,
        Market::Avax,
        Market::Near,
        Market::Dot,
    ];
}

/// One aggregated price level on a single venue.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Level {
    /// Price, rendered as an exact decimal string (e.g. "2519.83").
    pub px: String,
    /// Size, rendered as an exact decimal string.
    pub sz: String,
    /// Number of aggregated orders at this level (Hyperliquid only).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub n: Option<u32>,
    /// Which venue this level came from (set in consolidated view).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub v: Option<Venue>,
}

impl Level {
    pub fn px_dec(&self) -> Option<Decimal> {
        self.px.parse().ok()
    }

    pub fn sz_dec(&self) -> Option<Decimal> {
        self.sz.parse().ok()
    }
}

/// Connection status of one venue's feed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FeedStatus {
    Connecting,
    Live,
    Reconnecting,
    Error,
}

impl FeedStatus {
    pub const fn label(self) -> &'static str {
        match self {
            FeedStatus::Connecting => "connecting",
            FeedStatus::Live => "live",
            FeedStatus::Reconnecting => "reconnecting",
            FeedStatus::Error => "error",
        }
    }
}

/// Live per-venue feed telemetry.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VenueHealth {
    pub status: FeedStatus,
    /// Messages received from the venue in the last second (EMA).
    pub msg_per_sec: f32,
    /// Age of the most recent venue message, milliseconds.
    pub last_msg_age_ms: u64,
    /// Total number of levels currently held (bids+asks) for this venue.
    pub levels: usize,
}

/// A full snapshot of one venue's book for one market, trimmed for the wire.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VenueBook {
    pub health: VenueHealth,
    /// Best-first bids (descending price).
    pub bids: Vec<Level>,
    /// Best-first asks (ascending price).
    pub asks: Vec<Level>,
}

/// Cross-venue statistics, computed on exact decimals server-side.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BookStats {
    pub best_bid: String,
    pub best_ask: String,
    pub mid: String,
    pub spread: String,
    /// Spread in basis points relative to mid.
    pub spread_bps: String,
    /// Bid-side notional within 0.5% of mid (USD), exact string.
    pub near_bid_notional: String,
    /// Ask-side notional within 0.5% of mid (USD), exact string.
    pub near_ask_notional: String,
    /// Order-flow imbalance in [-1, 1] over near-touch notional.
    pub imbalance: String,
    /// Number of levels served either side (trimmed to wire depth).
    pub bid_levels: usize,
    pub ask_levels: usize,
}

/// A consolidated cross-venue ladder (levels carry venue attribution).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ConsolidatedBook {
    /// Best-first bids (descending price).
    pub bids: Vec<Level>,
    /// Best-first asks (ascending price).
    pub asks: Vec<Level>,
}

/// The single wire message streamed to browsers.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum WireEvent {
    /// Full orderbook state for the socket's selected market.
    Book {
        market: Market,
        hyperliquid: Option<VenueBook>,
        lighter: Option<VenueBook>,
        consolidated: Option<ConsolidatedBook>,
        stats: BookStats,
        /// Server unix-millis timestamp of this snapshot.
        ts: u64,
    },
    /// Compact per-market ticker (streamed for every market at 2 Hz).
    Ticker {
        market: Market,
        mid: String,
        spread_bps: String,
        imbalance: String,
        ts: u64,
    },
    /// New trades since the last flush (batched at 10 Hz).
    Trades {
        market: Market,
        trades: Vec<Trade>,
    },
    /// Historical depth samples for one market (full window on request,
    /// appended incrementally every 5 s for the selected market).
    History {
        market: Market,
        samples: Vec<DepthSample>,
    },
    /// Full alert list (on connect and after any change).
    AlertSet {
        alerts: Vec<Alert>,
    },
    /// A single alert just crossed its threshold.
    AlertFired {
        alert: Alert,
    },
    /// Backend heartbeat with connection summary.
    Status {
        hyperliquid: FeedStatus,
        lighter: FeedStatus,
        ts: u64,
    },
}

/// One executed trade on a venue (taker-side convention, like a tape).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Trade {
    pub venue: Venue,
    /// Taker side: `"B"` bought, `"A"` sold.
    pub side: String,
    /// Price, exact decimal string.
    pub px: String,
    /// Size, exact decimal string.
    pub sz: String,
    /// Venue timestamp, unix milliseconds.
    pub t: u64,
    /// Venue-unique dedup key (HL `tid`, Lighter `trade_id`).
    pub id: String,
}

/// One historical depth sample for a market.
///
/// Bands are cumulative USD notional within ±N% of mid, summed across both
/// venues. Field suffixes are tenths of a percent: `b1`=0.1%, `b5`=0.5%,
/// `b10`=1%, `b20`=2%. Floats are fine here — this feeds a chart, not the
/// trading ladder (which stays on exact decimals).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct DepthSample {
    /// Sample time, unix milliseconds.
    pub t: u64,
    /// Consolidated mid price at sample time.
    pub mid: f64,
    /// Spread in basis points.
    pub sb: f64,
    /// Bid notional within 0.1% of mid.
    pub b1: f64,
    /// Ask notional within 0.1% of mid.
    pub a1: f64,
    /// Bid notional within 0.5%.
    pub b5: f64,
    /// Ask notional within 0.5%.
    pub a5: f64,
    /// Bid notional within 1%.
    pub b10: f64,
    /// Ask notional within 1%.
    pub a10: f64,
    /// Bid notional within 2%.
    pub b20: f64,
    /// Ask notional within 2%.
    pub a20: f64,
    /// Order-flow imbalance over the 0.5% band, [-1, 1].
    pub im: f64,
}

/// Alert direction.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AlertDir {
    Above,
    Below,
}

impl AlertDir {
    pub const fn label(self) -> &'static str {
        match self {
            AlertDir::Above => "above",
            AlertDir::Below => "below",
        }
    }
}

/// A server-side price alert.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Alert {
    pub id: u64,
    pub market: Market,
    pub dir: AlertDir,
    /// Threshold price, exact decimal string.
    pub price: String,
    pub created_ms: u64,
    /// When the threshold was crossed, if it has been.
    pub triggered_ms: Option<u64>,
    /// Mid price at the moment of the trigger.
    pub triggered_px: Option<String>,
}

// ---------------------------------------------------------------------------
// Aggregation engine
// ---------------------------------------------------------------------------

/// Which side of the book a level belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Side {
    Bid,
    Ask,
}

/// Fixed internal tick scale. Every price is keyed as a `Decimal` mantissa at
/// 12 decimal places — always lossless for real venue data (max ~6 dp), and it
/// gives exact O(log n) BTreeMap ordering across mixed-tick venues.
const SCALE: u32 = 12;

fn tick_key(px: &Decimal) -> i128 {
    let mut p = if px.scale() > SCALE {
        px.round_dp(SCALE)
    } else {
        *px
    };
    p.rescale(SCALE);
    p.mantissa()
}

fn unkey(k: i128) -> Decimal {
    Decimal::from_i128_with_scale(k, SCALE)
}

/// Normalized in-memory book for a single (venue, market) pair.
///
/// Levels are keyed by integer price ticks inside a `BTreeMap`, giving
/// O(log n) ordered best-bid / best-ask access without re-parsing strings.
#[derive(Debug, Default, Clone)]
pub struct VenueState {
    /// price-tick -> (size, aggregated order count; None when the venue
    /// does not report one)
    bids: BTreeMap<i128, (Decimal, Option<u32>)>,
    asks: BTreeMap<i128, (Decimal, Option<u32>)>,
}

impl VenueState {
    /// Replace the whole book with an exact snapshot (Hyperliquid behaviour).
    pub fn replace(
        &mut self,
        bids: Vec<(Decimal, Decimal, Option<u32>)>,
        asks: Vec<(Decimal, Decimal, Option<u32>)>,
    ) {
        self.bids.clear();
        self.asks.clear();
        for (px, sz, n) in bids {
            if !sz.is_zero() {
                self.bids.insert(tick_key(&px), (sz, n));
            }
        }
        for (px, sz, n) in asks {
            if !sz.is_zero() {
                self.asks.insert(tick_key(&px), (sz, n));
            }
        }
    }

    /// Apply a venue-labelled delta for one side (Lighter behaviour):
    /// upsert by price; a size of zero removes the level.
    pub fn apply_side(&mut self, side: Side, levels: &[(Decimal, Decimal, Option<u32>)]) {
        let book = match side {
            Side::Bid => &mut self.bids,
            Side::Ask => &mut self.asks,
        };
        for (px, sz, n) in levels {
            let key = tick_key(px);
            if sz.is_zero() {
                book.remove(&key);
            } else {
                book.insert(key, (*sz, *n));
            }
        }
    }

    pub fn best_bid(&self) -> Option<(Decimal, Decimal)> {
        self.bids
            .iter()
            .next_back()
            .map(|(k, (sz, _))| (unkey(*k), *sz))
    }

    pub fn best_ask(&self) -> Option<(Decimal, Decimal)> {
        self.asks.iter().next().map(|(k, (sz, _))| (unkey(*k), *sz))
    }

    pub fn total_levels(&self) -> usize {
        self.bids.len() + self.asks.len()
    }

    /// Top `depth` levels, best first.
    pub fn top_bids(&self, depth: usize) -> Vec<Level> {
        self.bids
            .iter()
            .rev()
            .take(depth)
            .map(|(k, (sz, n))| Level {
                px: unkey(*k).normalize().to_string(),
                sz: sz.normalize().to_string(),
                n: *n,
                v: None,
            })
            .collect()
    }

    pub fn top_asks(&self, depth: usize) -> Vec<Level> {
        self.asks
            .iter()
            .take(depth)
            .map(|(k, (sz, n))| Level {
                px: unkey(*k).normalize().to_string(),
                sz: sz.normalize().to_string(),
                n: *n,
                v: None,
            })
            .collect()
    }

    /// Notional (sum of price * size) for levels within `pct` of `mid`.
    pub fn near_notional(&self, mid: Decimal, pct: Decimal, side: Side) -> Decimal {
        let band = mid * pct;
        let book = match side {
            Side::Bid => &self.bids,
            Side::Ask => &self.asks,
        };
        let mut total = Decimal::ZERO;
        for (k, (sz, _)) in book {
            let px = unkey(*k);
            let in_band = match side {
                Side::Bid => px >= mid - band && px <= mid,
                Side::Ask => px <= mid + band && px >= mid,
            };
            if in_band {
                total += px * sz;
            }
        }
        total
    }

    /// Cumulative USD notional within each of `ths` distance-from-mid bands,
    /// for one side, in a single pass over the book. `ths` must be sorted
    /// ascending (e.g. [0.001, 0.005, 0.01, 0.02]).
    pub fn band_notionals(
        &self,
        mid: Decimal,
        ths: &[Decimal],
        side: Side,
    ) -> Vec<Decimal> {
        let mut out = vec![Decimal::ZERO; ths.len()];
        let book = match side {
            Side::Bid => &self.bids,
            Side::Ask => &self.asks,
        };
        for (k, (sz, _)) in book {
            let px = unkey(*k);
            let notional = px * sz;
            for (i, th) in ths.iter().enumerate() {
                let band = mid * th;
                let in_band = match side {
                    Side::Bid => px >= mid - band,
                    Side::Ask => px <= mid + band,
                };
                if in_band {
                    out[i] += notional;
                }
            }
        }
        out
    }
}

/// Cross-venue mid price from whichever venues have data.
pub fn cross_mid(hl: Option<&VenueState>, lt: Option<&VenueState>) -> Option<Decimal> {
    let bb = [hl.and_then(|b| b.best_bid()), lt.and_then(|b| b.best_bid())]
        .into_iter()
        .flatten()
        .max_by(|x, y| x.0.cmp(&y.0))?;
    let ba = [hl.and_then(|b| b.best_ask()), lt.and_then(|b| b.best_ask())]
        .into_iter()
        .flatten()
        .min_by(|x, y| x.0.cmp(&y.0))?;
    Some((bb.0 + ba.0) / Decimal::from(2u64))
}

/// Merge two venue books into a consolidated ladder.
///
/// Levels at identical prices have their sizes summed. `v` records which
/// venue(s) rest at that price: `Some(Hyperliquid)`, `Some(Lighter)` or `None`
/// when both venues share the price level. Returns (bids, asks) best-first.
pub fn consolidate(a: &VenueState, b: &VenueState, depth: usize) -> (Vec<Level>, Vec<Level>) {
    let merge_side =
        |amap: &BTreeMap<i128, (Decimal, Option<u32>)>,
         bmap: &BTreeMap<i128, (Decimal, Option<u32>)>,
         is_bid: bool| {
            let mut merged: BTreeMap<i128, (Decimal, Option<Venue>)> = BTreeMap::new();
            for (k, (sz, _)) in amap {
                merged.insert(*k, (*sz, Some(Venue::Hyperliquid)));
            }
            for (k, (sz, _)) in bmap {
                let e = merged.entry(*k).or_insert((Decimal::ZERO, Some(Venue::Lighter)));
                if e.1 == Some(Venue::Hyperliquid) {
                    e.1 = None; // both venues rest at this price
                }
                e.0 += *sz;
            }
            let mut out: Vec<Level> = merged
                .into_iter()
                .map(|(k, (sz, v))| Level {
                    px: unkey(k).normalize().to_string(),
                    sz: sz.normalize().to_string(),
                    n: None,
                    v,
                })
                .collect();
            if is_bid {
                out.reverse(); // best (highest) first
            }
            out.truncate(depth);
            out
        };
    (
        merge_side(&a.bids, &b.bids, true),
        merge_side(&a.asks, &b.asks, false),
    )
}

/// Compute cross-venue stats on exact decimals.
pub fn compute_stats(hl: Option<&VenueState>, lt: Option<&VenueState>) -> BookStats {
    let bb = [hl.and_then(|b| b.best_bid()), lt.and_then(|b| b.best_bid())]
        .into_iter()
        .flatten()
        .max_by(|x, y| x.0.cmp(&y.0));
    let ba = [hl.and_then(|b| b.best_ask()), lt.and_then(|b| b.best_ask())]
        .into_iter()
        .flatten()
        .min_by(|x, y| x.0.cmp(&y.0));

    let (bb, ba) = match (bb, ba) {
        (Some(bb), Some(ba)) => (bb, ba),
        _ => {
            return BookStats {
                best_bid: "0".into(),
                best_ask: "0".into(),
                mid: "0".into(),
                spread: "0".into(),
                spread_bps: "0".into(),
                near_bid_notional: "0".into(),
                near_ask_notional: "0".into(),
                imbalance: "0".into(),
                bid_levels: 0,
                ask_levels: 0,
            };
        }
    };

    let mid = (bb.0 + ba.0) / Decimal::from(2u64);
    let spread = ba.0 - bb.0;
    let bps = if mid.is_zero() {
        Decimal::ZERO
    } else {
        spread / mid * Decimal::from(10_000u64)
    };
    let half_pct = Decimal::new(5, 3); // 0.005 => 0.5%

    let mut bid_notional = Decimal::ZERO;
    let mut ask_notional = Decimal::ZERO;
    if let Some(b) = hl {
        bid_notional += b.near_notional(mid, half_pct, Side::Bid);
        ask_notional += b.near_notional(mid, half_pct, Side::Ask);
    }
    if let Some(b) = lt {
        bid_notional += b.near_notional(mid, half_pct, Side::Bid);
        ask_notional += b.near_notional(mid, half_pct, Side::Ask);
    }
    let total = bid_notional + ask_notional;
    let imbalance = if total.is_zero() {
        Decimal::ZERO
    } else {
        (bid_notional - ask_notional) / total
    };

    let (bl, al) = match (hl, lt) {
        (Some(h), Some(l)) => (
            h.top_bids(WIRE_DEPTH).len() + l.top_bids(WIRE_DEPTH).len(),
            h.top_asks(WIRE_DEPTH).len() + l.top_asks(WIRE_DEPTH).len(),
        ),
        (Some(h), None) => (h.top_bids(WIRE_DEPTH).len(), h.top_asks(WIRE_DEPTH).len()),
        (None, Some(l)) => (l.top_bids(WIRE_DEPTH).len(), l.top_asks(WIRE_DEPTH).len()),
        _ => (0, 0),
    };

    BookStats {
        best_bid: bb.0.normalize().to_string(),
        best_ask: ba.0.normalize().to_string(),
        mid: mid.normalize().to_string(),
        spread: spread.normalize().to_string(),
        spread_bps: bps.round_dp(2).normalize().to_string(),
        near_bid_notional: bid_notional.round_dp(2).normalize().to_string(),
        near_ask_notional: ask_notional.round_dp(2).normalize().to_string(),
        imbalance: imbalance.round_dp(4).normalize().to_string(),
        bid_levels: bl,
        ask_levels: al,
    }
}

/// How many levels per side are streamed to browsers.
pub const WIRE_DEPTH: usize = 40;

/// How many trades per market are kept server-side for the tape backlog.
pub const TAPE_BACKLOG: usize = 120;

/// Depth sampler cadence (seconds) and window (samples => minutes).
pub const SAMPLE_SECS: u64 = 5;
pub const SAMPLE_WINDOW: usize = 720; // 5s * 720 = 60 minutes
