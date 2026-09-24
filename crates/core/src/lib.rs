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
//!
//! Venues: Hyperliquid + Lighter (perp DEXs) and the major ETH/SOL CLOBs
//! (Binance, Bybit, OKX, Kraken, Coinbase, Bitstamp, Gate). USDT-quoted venues
//! are normalized to USD with a live conversion factor (fed from Kraken's
//! USDT/USD book) applied at every comparison / aggregation boundary.

use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub mod cycle;
pub use cycle::{
    enumerate_cycles, probe_negative_cycles, walk_cycle, Asset, CycleFill, CycleHop,
    CycleOpportunity, CycleStats, CycleWalk, GaGenome, GaState, Leg, LegDir, SwapGraph,
};

// ---------------------------------------------------------------------------
// Venues
// ---------------------------------------------------------------------------

/// A venue identifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Venue {
    Hyperliquid,
    Lighter,
    Binance,
    Bybit,
    Okx,
    Kraken,
    Coinbase,
    Bitstamp,
    Gate,
}

/// All venues, in display order.
pub const VENUES: [Venue; Venue::COUNT] = [
    Venue::Hyperliquid,
    Venue::Lighter,
    Venue::Binance,
    Venue::Bybit,
    Venue::Okx,
    Venue::Kraken,
    Venue::Coinbase,
    Venue::Bitstamp,
    Venue::Gate,
];

impl Venue {
    pub const COUNT: usize = 9;

    pub const fn label(self) -> &'static str {
        match self {
            Venue::Hyperliquid => "Hyperliquid",
            Venue::Lighter => "Lighter",
            Venue::Binance => "Binance",
            Venue::Bybit => "Bybit",
            Venue::Okx => "OKX",
            Venue::Kraken => "Kraken",
            Venue::Coinbase => "Coinbase",
            Venue::Bitstamp => "Bitstamp",
            Venue::Gate => "Gate",
        }
    }

    /// Two-letter short code for tight UI surfaces (ladder tags, tape).
    pub const fn short(self) -> &'static str {
        match self {
            Venue::Hyperliquid => "HL",
            Venue::Lighter => "LT",
            Venue::Binance => "BN",
            Venue::Bybit => "BY",
            Venue::Okx => "OK",
            Venue::Kraken => "KR",
            Venue::Coinbase => "CB",
            Venue::Bitstamp => "BS",
            Venue::Gate => "GT",
        }
    }

    /// Stable index into `VENUES` (and into server-side venue tables).
    pub const fn index(self) -> usize {
        match self {
            Venue::Hyperliquid => 0,
            Venue::Lighter => 1,
            Venue::Binance => 2,
            Venue::Bybit => 3,
            Venue::Okx => 4,
            Venue::Kraken => 5,
            Venue::Coinbase => 6,
            Venue::Bitstamp => 7,
            Venue::Gate => 8,
        }
    }

    pub fn from_index(i: usize) -> Option<Self> {
        VENUES.get(i).copied()
    }

    /// Bit for this venue in a consolidated-level venue mask.
    pub const fn bit(self) -> u8 {
        1u8 << self.index()
    }

    /// Decode a venue mask (used by `Level::v` in consolidated ladders).
    pub fn from_mask(mask: u8) -> Vec<Venue> {
        VENUES.iter().copied().filter(|v| mask & v.bit() != 0).collect()
    }

    /// The quote currency this venue prices ETH/SOL in.
    pub const fn quote(self) -> Quote {
        match self {
            Venue::Hyperliquid | Venue::Lighter => Quote::Usd, // USDC-margined
            Venue::Binance | Venue::Bybit | Venue::Okx | Venue::Gate => Quote::Usdt,
            Venue::Kraken | Venue::Coinbase | Venue::Bitstamp => Quote::Usd,
        }
    }

    /// Default taker fee in basis points (entry-tier schedules; editable live
    /// through the arbitrage config panel).
    pub const fn default_taker_fee_bps(self) -> u32 {
        match self {
            Venue::Hyperliquid => 45, // 0.045% perp taker
            Venue::Lighter => 0,      // zero-fee zk-rollup
            Venue::Binance => 10,     // 0.10% spot taker
            Venue::Bybit => 10,       // 0.10% spot taker
            Venue::Okx => 10,         // 0.10% spot taker
            Venue::Kraken => 26,      // 0.26% Kraken Pro entry taker
            Venue::Coinbase => 60,    // 0.60% Advanced entry taker
            Venue::Bitstamp => 40,    // 0.40% entry taker
            Venue::Gate => 20,        // 0.20% entry taker
        }
    }

    /// Does this venue list the given market? The seven CLOBs are wired for
    /// the ETH and SOL books; Hyperliquid + Lighter cover all twelve.
    pub fn supports(self, m: Market) -> bool {
        match self {
            Venue::Hyperliquid | Venue::Lighter => true,
            _ => matches!(m, Market::Eth | Market::Sol),
        }
    }
}

/// Quote currency of a venue.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Quote {
    Usd,
    Usdt,
}

// ---------------------------------------------------------------------------
// Markets
// ---------------------------------------------------------------------------

/// A tradable market, mapped on both perp venues.
///
/// Lighter indices are live-verified against
/// `GET https://mainnet.zklighter.elliot.ai/api/v1/orderBooks`
/// (0:ETH 1:BTC 2:SOL 3:DOGE 4:1000PEPE 5:WIF 6:WLD 7:XRP 8:LINK 9:AVAX
///  10:NEAR 11:DOT). Hyperliquid coins are live-verified against
/// `POST /info {"type":"meta"}` (kPEPE = 1000PEPE on Hyperliquid).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
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

    /// Markets scanned by the cross-venue arbitrage engine (those listed on
    /// every CLOB).
    pub const ARB: [Market; 2] = [Market::Eth, Market::Sol];
}

// ---------------------------------------------------------------------------
// Books
// ---------------------------------------------------------------------------

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
    /// Venue bitmask in consolidated views (see `Venue::from_mask`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub v: Option<u8>,
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

/// A venue-tagged book snapshot (Book wire events carry one per live venue).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VenueBookAt {
    pub venue: Venue,
    #[serde(flatten)]
    pub book: VenueBook,
}

/// Cross-venue statistics, computed on exact decimals server-side.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BookStats {
    pub best_bid: String,
    pub best_ask: String,
    /// Venue quoting the cross-venue best bid.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bb_v: Option<Venue>,
    /// Venue quoting the cross-venue best ask.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ba_v: Option<Venue>,
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
    /// Venue-unique dedup key (prefixed with the venue short code).
    pub id: String,
}

/// One historical depth sample for a market.
///
/// Bands are cumulative USD notional within ±N% of mid, summed across
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
    /// Ask notional within 0.1%.
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
// Arbitrage engine types
// ---------------------------------------------------------------------------

/// Live engine configuration (all thresholds are exact decimal strings).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ArbConfig {
    /// Paper-executor armed (opportunities are always tracked for display).
    pub enabled: bool,
    /// Minimum net edge (bps, after fees) to list an opportunity.
    pub min_edge_bps: String,
    /// Minimum net edge (bps) for the paper executor to fire.
    pub fire_edge_bps: String,
    /// Maximum notional per simulated fill, USD.
    pub max_notional_usd: String,
    /// Simulated round-trip execution latency, milliseconds.
    pub latency_ms: u64,
    /// Per-route cooldown between paper fills, milliseconds.
    pub cooldown_ms: u64,
    /// Taker fee per venue, basis points.
    pub fees_bps: Vec<(Venue, String)>,
}

impl Default for ArbConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            min_edge_bps: "3".into(),
            fire_edge_bps: "8".into(),
            max_notional_usd: "10000".into(),
            latency_ms: 250,
            cooldown_ms: 3000,
            fees_bps: VENUES
                .iter()
                .map(|v| (*v, v.default_taker_fee_bps().to_string()))
                .collect(),
        }
    }
}

/// A live cross-venue arbitrage opportunity (all values exact decimal strings).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ArbOpportunity {
    pub market: Market,
    pub buy_venue: Venue,
    pub sell_venue: Venue,
    /// Executable VWAP on the buy side (USD-normalized).
    pub buy_px: String,
    /// Executable VWAP on the sell side (USD-normalized).
    pub sell_px: String,
    /// Top-of-book prices (USD-normalized).
    pub buy_touch: String,
    pub sell_touch: String,
    /// Executable base size.
    pub size: String,
    /// Buy-side notional, USD.
    pub notional: String,
    /// Gross edge before fees, bps.
    pub gross_bps: String,
    /// Combined fee drag, bps.
    pub fee_bps: String,
    /// Net edge after fees, bps.
    pub net_bps: String,
    /// Net profit for the executable size, USD.
    pub profit_usd: String,
    pub ts: u64,
}

/// A paper fill (or an opportunity that expired during the latency window).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ArbFill {
    pub id: u64,
    pub market: Market,
    pub buy_venue: Venue,
    pub sell_venue: Venue,
    pub buy_px: String,
    pub sell_px: String,
    pub size: String,
    pub notional: String,
    pub fees_usd: String,
    pub pnl_usd: String,
    pub net_bps: String,
    /// `"filled"` or `"expired"` (edge vanished before the latency elapsed).
    pub status: String,
    /// Net edge in bps at detection time.
    pub detected_net_bps: String,
    pub ts: u64,
}

/// One point on the paper-trading equity curve.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct EquityPt {
    pub t: u64,
    pub eq: f64,
}

/// Per-route aggregate statistics.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ArbPairStat {
    pub market: Market,
    pub buy: Venue,
    pub sell: Venue,
    pub fills: u64,
    pub pnl: String,
    pub best_bps: String,
}

/// Aggregate arbitrage stats. `equity` is only populated in full snapshots;
/// updates carry scalars plus `eq_now` for the live curve.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct ArbStats {
    pub ops_tracked: u64,
    pub fills: u64,
    pub expired: u64,
    pub wins: u64,
    pub losses: u64,
    pub pnl_usd: String,
    pub fees_usd: String,
    pub best_net_bps: String,
    pub avg_net_bps: String,
    /// Live cumulative P&L as a float (curve feed).
    pub eq_now: f64,
    pub open_ops: usize,
    pub equity: Vec<EquityPt>,
    pub pairs: Vec<ArbPairStat>,
}

/// Partial config update (any field optional) — shared by the WS client
/// command and the REST PUT endpoint.
#[derive(Debug, Default, Clone, serde::Serialize, serde::Deserialize)]
pub struct ArbConfigUpdate {
    #[serde(default)]
    pub enabled: Option<bool>,
    #[serde(default)]
    pub min_edge_bps: Option<String>,
    #[serde(default)]
    pub fire_edge_bps: Option<String>,
    #[serde(default)]
    pub max_notional_usd: Option<String>,
    #[serde(default)]
    pub latency_ms: Option<u64>,
    #[serde(default)]
    pub cooldown_ms: Option<u64>,
    #[serde(default)]
    pub fees_bps: Option<Vec<(Venue, String)>>,
}

/// The single wire message streamed to browsers.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum WireEvent {
    /// Full orderbook state for the socket's selected market.
    Book {
        market: Market,
        /// One entry per venue with live data, in venue order.
        venues: Vec<VenueBookAt>,
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
    /// Full arbitrage state (on connect and after config changes / reset).
    ArbSnapshot {
        config: ArbConfig,
        stats: ArbStats,
        /// Currently-live opportunities, best net edge first.
        opportunities: Vec<ArbOpportunity>,
        /// Recent paper fills / expirations, newest last.
        fills: Vec<ArbFill>,
    },
    /// Live arbitrage deltas at 2 Hz.
    ArbUpdate {
        /// Currently-live opportunities, best net edge first.
        opportunities: Vec<ArbOpportunity>,
        /// Scalar stats (no equity series / pair table).
        stats: ArbStats,
    },
    /// One paper fill or latency-expired opportunity.
    ArbFillEvent {
        fill: ArbFill,
    },
    /// Full multi-hop cycle engine state (on connect / reset).
    CycleSnapshot {
        stats: CycleStats,
        opportunities: Vec<CycleOpportunity>,
        fills: Vec<CycleFill>,
    },
    /// Live cycle deltas at 2 Hz.
    CycleUpdate {
        stats: CycleStats,
        opportunities: Vec<CycleOpportunity>,
    },
    /// One cycle paper fill or latency expiry.
    CycleFillEvent {
        fill: CycleFill,
    },
    /// Genetic-algorithm optimizer state (0.5 Hz).
    GaUpdate {
        state: GaState,
    },
    /// Backend heartbeat with connection summary.
    Status {
        /// One entry per venue, in venue order.
        venues: Vec<(Venue, FeedStatus)>,
        /// Live USDT/USD conversion rate (Kraken).
        usdt: String,
        ts: u64,
    },
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
/// Prices are stored exactly as the venue quotes them (USDT venues keep
/// native ticks); conversion to USD happens at every aggregation boundary
/// via the venue factor passed in by the caller.
#[derive(Debug, Default, Clone)]
pub struct VenueState {
    /// price-tick -> (size, aggregated order count; None when the venue
    /// does not report one)
    bids: BTreeMap<i128, (Decimal, Option<u32>)>,
    asks: BTreeMap<i128, (Decimal, Option<u32>)>,
}

impl VenueState {
    /// Replace the whole book with an exact snapshot (Hyperliquid, Binance,
    /// OKX, Bitstamp, Gate, Coinbase behaviour).
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

    /// Apply a venue-labelled delta for one side (Lighter, Bybit, Kraken,
    /// Coinbase behaviour): upsert by price; a size of zero removes the level.
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

    /// Capture the top `depth` levels of both sides as exact decimal pairs,
    /// best-first (bids descending, asks ascending). Used by the multi-hop
    /// cycle engine to snapshot legs under the books lock.
    pub fn capture(&self, depth: usize) -> (Vec<(Decimal, Decimal)>, Vec<(Decimal, Decimal)>) {
        let bids = self
            .bids
            .iter()
            .rev()
            .take(depth)
            .map(|(k, (sz, _))| (unkey(*k), *sz))
            .collect();
        let asks = self
            .asks
            .iter()
            .take(depth)
            .map(|(k, (sz, _))| (unkey(*k), *sz))
            .collect();
        (bids, asks)
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

/// A venue book plus its USD conversion factor, ready for aggregation.
/// `factor` is 1 for USD venues and the live USDT/USD rate for USDT venues.
#[derive(Clone, Copy)]
pub struct Sourced<'a> {
    pub venue: Venue,
    pub state: &'a VenueState,
    pub factor: Decimal,
}

/// Cross-venue mid price (USD-normalized) from whichever venues have data.
pub fn cross_mid(books: &[Sourced<'_>]) -> Option<Decimal> {
    let bb = books
        .iter()
        .filter_map(|s| s.state.best_bid().map(|(px, _)| px * s.factor))
        .max()?;
    let ba = books
        .iter()
        .filter_map(|s| s.state.best_ask().map(|(px, _)| px * s.factor))
        .min()?;
    Some((bb + ba) / Decimal::from(2u64))
}

/// Merge any number of venue books into a consolidated USD ladder.
///
/// Prices are normalized by each venue's factor (USDT venues get the live
/// USDT/USD rate) and keyed on 4-decimal ticks so shared levels merge.
/// `Level::v` carries a bitmask of the venues resting at that price.
pub fn consolidate(books: &[Sourced<'_>], depth: usize) -> (Vec<Level>, Vec<Level>) {
    let merge_side = |is_bid: bool| {
        let mut merged: BTreeMap<i128, (Decimal, u8)> = BTreeMap::new();
        for s in books {
            let (book, descending) = match is_bid {
                true => (&s.state.bids, true),
                false => (&s.state.asks, false),
            };
            let iter: Box<dyn Iterator<Item = (&i128, &(Decimal, Option<u32>))>> = if descending {
                Box::new(book.iter().rev().take(depth))
            } else {
                Box::new(book.iter().take(depth))
            };
            for (k, (sz, _)) in iter {
                let px = unkey(*k) * s.factor;
                let key = tick_key(&px.round_dp(4));
                let e = merged.entry(key).or_insert((Decimal::ZERO, 0u8));
                e.0 += *sz;
                e.1 |= s.venue.bit();
            }
        }
        let mut out: Vec<Level> = merged
            .into_iter()
            .map(|(k, (sz, mask))| Level {
                px: unkey(k).normalize().to_string(),
                sz: sz.normalize().to_string(),
                n: None,
                v: Some(mask),
            })
            .collect();
        if is_bid {
            out.reverse(); // best (highest) first
        }
        out.truncate(depth);
        out
    };
    (merge_side(true), merge_side(false))
}

/// Compute cross-venue stats on exact decimals (USD-normalized).
pub fn compute_stats(books: &[Sourced<'_>]) -> BookStats {
    // Best bid: maximize px * factor; best ask: minimize px * factor.
    let mut bb: Option<(Decimal, Venue)> = None;
    let mut ba: Option<(Decimal, Venue)> = None;
    for s in books {
        if let Some((px, _)) = s.state.best_bid() {
            let adj = px * s.factor;
            if bb.as_ref().map(|(x, _)| adj > *x).unwrap_or(true) {
                bb = Some((adj, s.venue));
            }
        }
        if let Some((px, _)) = s.state.best_ask() {
            let adj = px * s.factor;
            if ba.as_ref().map(|(x, _)| adj < *x).unwrap_or(true) {
                ba = Some((adj, s.venue));
            }
        }
    }

    let (bb, ba, bb_v, ba_v) = match (bb, ba) {
        (Some((bp, bv)), Some((ap, av))) => (bp, ap, Some(bv), Some(av)),
        _ => {
            return BookStats {
                best_bid: "0".into(),
                best_ask: "0".into(),
                bb_v: None,
                ba_v: None,
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

    let mid = (bb + ba) / Decimal::from(2u64);
    let spread = ba - bb;
    let bps = if mid.is_zero() {
        Decimal::ZERO
    } else {
        spread / mid * Decimal::from(10_000u64)
    };
    let half_pct = Decimal::new(5, 3); // 0.005 => 0.5%

    // Near-touch notionals, scaled per-venue to USD.
    let mut bid_notional = Decimal::ZERO;
    let mut ask_notional = Decimal::ZERO;
    for s in books {
        bid_notional += s.state.near_notional(mid, half_pct, Side::Bid) * s.factor;
        ask_notional += s.state.near_notional(mid, half_pct, Side::Ask) * s.factor;
    }
    let total = bid_notional + ask_notional;
    let imbalance = if total.is_zero() {
        Decimal::ZERO
    } else {
        (bid_notional - ask_notional) / total
    };

    let mut bid_levels = 0usize;
    let mut ask_levels = 0usize;
    for s in books {
        bid_levels += s.state.top_bids(WIRE_DEPTH).len();
        ask_levels += s.state.top_asks(WIRE_DEPTH).len();
    }

    BookStats {
        best_bid: bb.round_dp(4).normalize().to_string(),
        best_ask: ba.round_dp(4).normalize().to_string(),
        bb_v,
        ba_v,
        mid: mid.round_dp(4).normalize().to_string(),
        spread: spread.round_dp(4).normalize().to_string(),
        spread_bps: bps.round_dp(2).normalize().to_string(),
        near_bid_notional: bid_notional.round_dp(2).normalize().to_string(),
        near_ask_notional: ask_notional.round_dp(2).normalize().to_string(),
        imbalance: imbalance.round_dp(4).normalize().to_string(),
        bid_levels,
        ask_levels,
    }
}

// ---------------------------------------------------------------------------
// Arbitrage walk
// ---------------------------------------------------------------------------

/// Result of walking a buy-side book against a sell-side book.
#[derive(Debug, Clone)]
pub struct ArbWalk {
    /// Executable base size (already capped by `max_notional`).
    pub size: Decimal,
    /// USD-normalized VWAP paid on the buy side.
    pub buy_vwap: Decimal,
    /// USD-normalized VWAP received on the sell side.
    pub sell_vwap: Decimal,
    /// Buy-side cost for `size`, USD.
    pub cost: Decimal,
    /// Sell-side proceeds for `size`, USD.
    pub proceeds: Decimal,
    /// Combined fee drag, USD.
    pub fees: Decimal,
    /// Net profit, USD.
    pub profit: Decimal,
    pub gross_bps: Decimal,
    pub fee_bps: Decimal,
    pub net_bps: Decimal,
    /// Top-of-book (USD-normalized) for display.
    pub buy_touch: Decimal,
    pub sell_touch: Decimal,
}

/// Walk two venue books and find the profit-maximizing executable size.
///
/// The books are piecewise-linear, so the exact optimum is found greedily:
/// keep filling while the marginal ask (times the buy venue's factor and
/// grossed up by its taker fee) is below the marginal bid (times the sell
/// venue's factor, net of its taker fee). `fees` are decimals (0.001 = 10 bps).
pub fn arb_walk(
    buy: &VenueState,
    sell: &VenueState,
    f_buy: Decimal,
    f_sell: Decimal,
    fee_buy: Decimal,
    fee_sell: Decimal,
    max_notional: Decimal,
) -> Option<ArbWalk> {
    // Effectively pay  px * f_buy * (1 + fee_buy) per unit on the buy side.
    // Effectively get  px * f_sell * (1 - fee_sell) per unit on the sell side.
    let mut size = Decimal::ZERO;
    let mut cost = Decimal::ZERO; // ex-fee, USD
    let mut proceeds = Decimal::ZERO; // ex-fee, USD

    let mut ask = buy.asks.iter(); // ascending
    let mut bid = sell.bids.iter().rev(); // descending
    let (mut ask_key, mut ask_rem) = {
        let (k, (sz, _)) = ask.next()?;
        (*k, *sz)
    };
    let (mut bid_key, mut bid_rem) = {
        let (k, (sz, _)) = bid.next()?;
        (*k, *sz)
    };

    loop {
        let ask_px = unkey(ask_key) * f_buy;
        let bid_px = unkey(bid_key) * f_sell;
        let pay = ask_px * (Decimal::ONE + fee_buy);
        let recv = bid_px * (Decimal::ONE - fee_sell);
        if recv <= pay {
            break;
        }
        let mut q = ask_rem.min(bid_rem);
        // Cap by remaining notional budget.
        if max_notional > Decimal::ZERO {
            let room = (max_notional - cost) / ask_px;
            if room <= Decimal::ZERO {
                break;
            }
            if q > room {
                q = room;
            }
        }
        if q <= Decimal::ZERO {
            break;
        }
        size += q;
        cost += q * ask_px;
        proceeds += q * bid_px;
        ask_rem -= q;
        bid_rem -= q;

        if ask_rem <= Decimal::ZERO {
            match ask.next() {
                Some((k, (sz, _))) => {
                    ask_key = *k;
                    ask_rem = *sz;
                }
                None => break,
            }
        }
        if bid_rem <= Decimal::ZERO {
            match bid.next() {
                Some((k, (sz, _))) => {
                    bid_key = *k;
                    bid_rem = *sz;
                }
                None => break,
            }
        }
    }

    if size <= Decimal::ZERO {
        return None;
    }
    let fees = cost * fee_buy + proceeds * fee_sell;
    let profit = proceeds - cost - fees;
    let gross = proceeds - cost;
    let (gross_bps, fee_bps, net_bps) = if cost.is_zero() {
        (Decimal::ZERO, Decimal::ZERO, Decimal::ZERO)
    } else {
        (
            gross / cost * Decimal::from(10_000u64),
            fees / cost * Decimal::from(10_000u64),
            profit / cost * Decimal::from(10_000u64),
        )
    };
    Some(ArbWalk {
        size,
        buy_vwap: (cost / size).round_dp(4),
        sell_vwap: (proceeds / size).round_dp(4),
        cost: cost.round_dp(2),
        proceeds: proceeds.round_dp(2),
        fees: fees.round_dp(4),
        profit: profit.round_dp(4),
        gross_bps: gross_bps.round_dp(2),
        fee_bps: fee_bps.round_dp(2),
        net_bps: net_bps.round_dp(2),
        buy_touch: (unkey(*buy.asks.iter().next()?.0) * f_buy).round_dp(4),
        sell_touch: (unkey(*sell.bids.iter().next_back()?.0) * f_sell).round_dp(4),
    })
}

/// How many levels per side are streamed to browsers.
pub const WIRE_DEPTH: usize = 26;

/// How many trades per market are kept server-side for the tape backlog.
pub const TAPE_BACKLOG: usize = 120;

/// Depth sampler cadence (seconds) and window (samples => minutes).
pub const SAMPLE_SECS: u64 = 5;
pub const SAMPLE_WINDOW: usize = 720; // 5s * 720 = 60 minutes

/// Minimum buy-side notional (USD) for an opportunity to be listed (dust guard).
pub const ARB_MIN_NOTIONAL: f64 = 100.0;

/// How long a tracked opportunity survives without being re-observed.
pub const ARB_OP_TTL_MS: u64 = 2500;

/// Equity curve ring buffer (1 Hz cadence => 30 minutes).
pub const ARB_EQUITY_WINDOW: usize = 1800;

/// Recent fills kept for the wire.
pub const ARB_FILL_LOG: usize = 60;
