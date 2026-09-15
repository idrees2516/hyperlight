//! Client-side data model derived from the backend's `WireEvent` stream.

use ob_core::{
    ArbConfig, ArbFill, ArbOpportunity, ArbStats, BookStats, ConsolidatedBook, DepthSample,
    EquityPt, FeedStatus, Market, Trade, Venue, VenueBook,
};
use std::collections::HashMap;
use std::sync::Arc;

/// One market's most recent full snapshot.
#[derive(Clone, PartialEq)]
pub struct BookData {
    pub market: Market,
    /// Live venue books (per-venue native prices).
    pub venues: HashMap<Venue, VenueBook>,
    pub consolidated: Option<ConsolidatedBook>,
    pub stats: BookStats,
    pub ts: u64,
}

/// Everything the UI knows about, cheap to clone.
/// Full books arrive only for the selected market (server-side routing).
#[derive(Clone, Default)]
pub struct Books(pub HashMap<Market, Arc<BookData>>);

impl Books {
    pub fn get(&self, m: Market) -> Option<Arc<BookData>> {
        self.0.get(&m).cloned()
    }
}

/// Compact per-market ticker (streamed for every market at 2 Hz).
#[derive(Clone, PartialEq)]
pub struct TickerData {
    pub mid: String,
    pub spread_bps: String,
    pub imbalance: String,
    /// Direction of the last mid move: 1 up, -1 down, 0 unknown.
    pub dir: i8,
    pub ts: u64,
}

/// Trade tape state: newest-first Vec per market.
pub type Tapes = HashMap<Market, Vec<Trade>>;

/// Depth history per market (oldest-first, capped to the server window).
pub type Histories = HashMap<Market, Vec<DepthSample>>;

/// Which ladder is displayed: consolidated, or one venue's native book.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum TabView {
    Consolidated,
    Venue(Venue),
}

impl TabView {
    pub fn label(self) -> String {
        match self {
            TabView::Consolidated => "Consolidated".into(),
            TabView::Venue(v) => v.label().into(),
        }
    }

    pub fn key(self) -> String {
        match self {
            TabView::Consolidated => "cons".into(),
            TabView::Venue(v) => format!("v{}", v.index()),
        }
    }
}

/// Browser<->backend websocket link state.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum LinkStatus {
    Connecting,
    Open,
    Closed,
}

impl LinkStatus {
    pub const fn label(self) -> &'static str {
        match self {
            LinkStatus::Connecting => "connecting",
            LinkStatus::Open => "open",
            LinkStatus::Closed => "retrying",
        }
    }
}

/// Live per-venue feed statuses as pushed by the Status heartbeat.
#[derive(Clone, Default)]
pub struct VenueStatuses {
    pub map: HashMap<Venue, FeedStatus>,
    /// Live USDT/USD conversion rate.
    pub usdt: String,
}

// ---------------------------------------------------------------------------
// Arbitrage panel state
// ---------------------------------------------------------------------------

/// Everything the arbitrage panel renders.
#[derive(Clone)]
pub struct ArbState {
    pub config: ArbConfig,
    pub opportunities: Vec<ArbOpportunity>,
    pub fills: Vec<ArbFill>,
    pub stats: ArbStats,
    /// Equity curve: server series + locally appended update points.
    pub equity: Vec<EquityPt>,
}

impl Default for ArbState {
    fn default() -> Self {
        Self {
            config: ArbConfig::default(),
            opportunities: Vec::new(),
            fills: Vec::new(),
            stats: ArbStats::default(),
            equity: Vec::new(),
        }
    }
}

// ---------------------------------------------------------------------------
// Toasts
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ToastKind {
    Info,
    Success,
    Warning,
}

/// A transient notification (alert fired, alert created, arb fill, ...).
#[derive(Clone, PartialEq)]
pub struct Toast {
    pub id: u64,
    pub kind: ToastKind,
    pub title: String,
    pub body: String,
}

/// Format a unix-ms timestamp as local HH:MM:SS.
pub fn fmt_hms(ms: u64) -> String {
    let d = js_sys::Date::new(&wasm_bindgen::JsValue::from_f64(ms as f64));
    format!(
        "{:02}:{:02}:{:02}",
        d.get_hours(),
        d.get_minutes(),
        d.get_seconds()
    )
}

/// Trim a raw venue decimal string for display: strips trailing zeros and
/// caps at 6 decimal places ("2475.26000000" -> "2475.26", "0.00060000" ->
/// "0.0006"). Falls back to the input when unparsable.
pub fn fmt_num(s: &str) -> String {
    let t = s.trim();
    if let Some(stripped) = t.strip_suffix(".0") {
        return stripped.to_string();
    }
    if t.contains('.') {
        let mut out = t.trim_end_matches('0').trim_end_matches('.').to_string();
        // Cap runaway precision.
        if let Some(dot) = out.find('.') {
            if out.len() - dot - 1 > 6 {
                out.truncate(dot + 7);
                out = out.trim_end_matches('0').trim_end_matches('.').to_string();
            }
        }
        return out;
    }
    t.to_string()
}

/// Two-letter short code for a venue chip.
pub fn venue_short(v: Venue) -> &'static str {
    v.short()
}
