//! Client-side data model derived from the backend's `WireEvent` stream.

use ob_core::{BookStats, ConsolidatedBook, FeedStatus, Market, VenueBook};
use std::sync::Arc;

/// One market's most recent snapshot.
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
#[derive(Clone, Default)]
pub struct Books {
    pub eth: Option<Arc<BookData>>,
    pub btc: Option<Arc<BookData>>,
    pub sol: Option<Arc<BookData>>,
}

impl Books {
    pub fn get(&self, m: Market) -> Option<Arc<BookData>> {
        match m {
            Market::Eth => self.eth.clone(),
            Market::Btc => self.btc.clone(),
            Market::Sol => self.sol.clone(),
        }
    }
}

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
