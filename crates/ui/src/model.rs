//! Client-side data model derived from the backend's `WireEvent` stream.

use ob_core::{BookStats, ConsolidatedBook, DepthSample, FeedStatus, Market, Trade, VenueBook};
use std::collections::HashMap;
use std::sync::Arc;

/// One market's most recent full snapshot.
#[derive(Clone, PartialEq)]
pub struct BookData {
    pub market: Market,
    pub hyperliquid: Option<VenueBook>,
    pub lighter: Option<VenueBook>,
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

/// Which ladder is displayed.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum TabView {
    Consolidated,
    Hyperliquid,
    Lighter,
}

impl TabView {
    pub const fn label(self) -> &'static str {
        match self {
            TabView::Consolidated => "Consolidated",
            TabView::Hyperliquid => "Hyperliquid",
            TabView::Lighter => "Lighter",
        }
    }

    pub const ALL: [TabView; 3] = [
        TabView::Consolidated,
        TabView::Hyperliquid,
        TabView::Lighter,
    ];
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

/// Venue status pair as pushed by the backend Status heartbeat.
#[derive(Clone, Copy, Default)]
pub struct VenuePair {
    pub hyperliquid: Option<FeedStatus>,
    pub lighter: Option<FeedStatus>,
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

/// A transient notification (alert fired, alert created, ...).
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
