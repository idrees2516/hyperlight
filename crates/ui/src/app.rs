//! Application shell: header, market selector, live stats, venue tabs,
//! ladder, trade tape, alerts, arbitrage engine, depth history and footer.

use crate::components::*;
use crate::ladder::{build_rows, Ladder, Row, DISPLAY_DEPTH};
use crate::model::{BookData, LinkStatus, TabView};
use crate::ws::Signals;
use leptos::prelude::*;
use ob_core::{BookStats, FeedStatus, Market, Venue, VENUES};
use std::sync::Arc;

#[component]
pub fn App() -> impl IntoView {
    let sig = Signals::new();
    crate::ws::connect(sig);

    let market = sig.selected;
    let tab = RwSignal::new(TabView::Consolidated);

    // Keep the server-side socket selection in sync with the UI selection.
    // (Also fires on reconnect: when the link reopens, re-select.)
    Effect::new(move |_| {
        let m = market.get();
        if sig.link.get() == LinkStatus::Open {
            sig.select_market(m);
        }
    });

    // Most recent snapshot for the selected market.
    let book: Memo<Option<Arc<BookData>>> =
        Memo::new(move |_| sig.books.get().get(market.get()));

    // Mid-price direction indicator: (last mid, direction).
    let tick = RwSignal::new((0f64, 0i8));
    Effect::new(move |_| {
        if let Some(d) = book.get() {
            if let Ok(mid) = d.stats.mid.parse::<f64>() {
                tick.update(|(last, dir)| {
                    if *last != 0.0 && (mid - *last).abs() > f64::EPSILON {
                        *dir = if mid > *last { 1 } else { -1 };
                    }
                    *last = mid;
                });
            }
        }
    });

    // Venue tabs available for the current market (those with live data),
    // always with Consolidated first.
    let venue_tabs: Memo<Vec<TabView>> = Memo::new(move |_| {
        let d = book.get();
        let mut out = vec![TabView::Consolidated];
        if let Some(d) = &d {
            for v in VENUES {
                if d.venues.contains_key(&v) {
                    out.push(TabView::Venue(v));
                }
            }
        }
        out
    });

    // Ladder rows (reactive on book + tab).
    let ask_rows: Memo<Vec<Row>> = Memo::new(move |_| {
        let d = book.get();
        let asks = levels_for(d.as_deref(), tab.get(), false);
        build_rows(&asks, true)
    });
    let bid_rows: Memo<Vec<Row>> = Memo::new(move |_| {
        let d = book.get();
        let bids = levels_for(d.as_deref(), tab.get(), true);
        build_rows(&bids, false)
    });
    let stats: Memo<BookStats> =
        Memo::new(move |_| book.get().map(|d| d.stats.clone()).unwrap_or_else(dash_stats));
    let loaded: Memo<bool> = Memo::new(move |_| book.get().is_some());

    view! {
        <div class="min-h-screen flex flex-col bg-background">
            <Header sig market />
            <main class="flex-1 w-full max-w-7xl mx-auto px-4 lg:px-6 py-5 flex flex-col gap-5">
                <StatsStrip book tick />

                // Ladder + right rail (tape, alerts).
                <div class="grid grid-cols-1 lg:grid-cols-3 gap-4 items-start">
                    <div class="lg:col-span-2 min-w-0">
                        {move || {
                            let current_tab = tab.get();
                            let show_venue = current_tab == TabView::Consolidated;
                            view! {
                                <LadderCard
                                    book=book
                                    tab_sig=tab
                                    tab=current_tab
                                    venue_tabs=venue_tabs
                                    ask_rows=ask_rows
                                    bid_rows=bid_rows
                                    stats=stats
                                    show_venue=show_venue
                                    loaded=loaded
                                />
                            }
                        }}
                    </div>
                    <div class="flex flex-col gap-4 min-w-0">
                        <crate::tape::TradeTape sig />
                        <crate::alerts::AlertsPanel sig />
                    </div>
                </div>

                // Cross-venue arbitrage engine (full width).
                <crate::arb::ArbCard sig />

                <crate::chart::DepthHistoryCard sig />

                <VenueStrip book />
            </main>
            <Footer sig />
            <Toasts toasts=sig.toasts />
        </div>
    }
}

fn levels_for(d: Option<&BookData>, tab: TabView, bids: bool) -> Vec<ob_core::Level> {
    match tab {
        TabView::Consolidated => d
            .and_then(|d| d.consolidated.as_ref())
            .map(|c| {
                if bids {
                    &c.bids
                } else {
                    &c.asks
                }
            })
            .cloned()
            .unwrap_or_default(),
        TabView::Venue(v) => d
            .and_then(|d| d.venues.get(&v))
            .map(|vb| {
                if bids {
                    &vb.bids
                } else {
                    &vb.asks
                }
            })
            .cloned()
            .unwrap_or_default(),
    }
}

fn abbrev_money(s: &str) -> String {
    let v: f64 = s.parse().unwrap_or(0.0);
    if v.abs() >= 1_000_000.0 {
        format!("{:.2}M", v / 1_000_000.0)
    } else if v.abs() >= 1_000.0 {
        format!("{:.1}K", v / 1_000.0)
    } else {
        format!("{v:.0}")
    }
}

/// Placeholder stats before the first snapshot arrives.
fn dash_stats() -> BookStats {
    BookStats {
        best_bid: "\u{2014}".into(),
        best_ask: "\u{2014}".into(),
        bb_v: None,
        ba_v: None,
        mid: "\u{2014}".into(),
        spread: "\u{2014}".into(),
        spread_bps: "\u{2014}".into(),
        near_bid_notional: "0".into(),
        near_ask_notional: "0".into(),
        imbalance: "0".into(),
        bid_levels: 0,
        ask_levels: 0,
    }
}

// ---------------------------------------------------------------------------
// Header
// ---------------------------------------------------------------------------

#[component]
fn Header(sig: Signals, market: RwSignal<Market>) -> impl IntoView {
    view! {
        <header class="sticky top-0 z-40 border-b border-border bg-background/80 backdrop-blur supports-[backdrop-filter]:bg-background/60">
            <div class="max-w-7xl mx-auto px-4 lg:px-6 h-16 flex items-center justify-between gap-3">
                <div class="flex items-center gap-3 min-w-0">
                    <div class="flex h-9 w-9 items-center justify-center rounded-lg border border-border bg-card shrink-0">
                        <span class="font-mono text-sm font-bold text-emerald-400">"\u{25B2}"</span>
                    </div>
                    <div class="flex flex-col min-w-0">
                        <span class="font-semibold text-[15px] leading-tight tracking-tight truncate">
                            "HyperLight"
                            <span class="text-muted-foreground font-normal">" Terminal"</span>
                        </span>
                        <span class="text-[11px] text-muted-foreground font-mono truncate hidden sm:block">
                            "9-venue consolidated books + arbitrage engine"
                        </span>
                    </div>
                </div>

                // Market selector (12 markets, live tickers).
                <MarketSelect selected=market tickers=sig.tickers />

                <div class="flex items-center gap-2 shrink-0">
                    {move || {
                        let l = sig.link.get();
                        if l == LinkStatus::Open {
                            view! {
                                <Badge variant=BadgeVariant::Success class="uppercase">
                                    <PulseDot />
                                    {format!("ws {}", l.label())}
                                </Badge>
                            }.into_any()
                        } else {
                            view! {
                                <Badge variant=BadgeVariant::Warning class="uppercase">
                                    {format!("ws {}", l.label())}
                                </Badge>
                            }.into_any()
                        }
                    }}
                </div>
            </div>
            <div class="max-w-7xl mx-auto px-4 lg:px-6 pb-2 -mt-1 flex items-center gap-2 flex-wrap">
                <For each=move || VENUES.to_vec() key=|v| v.index() let: v>
                    <VenueBadge statuses=sig.venues venue=v />
                </For>
                <Badge variant=BadgeVariant::Outline class="font-mono text-[10px]">
                    {move || format!("usdt {}", sig.venues.get().usdt)}
                </Badge>
                <Badge variant=BadgeVariant::Outline class="font-mono text-[10px]">
                    {format!("{} markets", Market::ALL.len())}
                </Badge>
                <span class="text-[11px] text-muted-foreground font-mono ml-auto hidden lg:inline">
                    "full-depth L2 — exact decimal aggregation — live USDT/USD normalization"
                </span>
            </div>
        </header>
    }
}

#[component]
fn VenueBadge(statuses: RwSignal<crate::model::VenueStatuses>, venue: Venue) -> impl IntoView {
    view! {
        {move || {
            let st = statuses.get().map.get(&venue).copied();
            let (label, variant, live) = match st {
                Some(FeedStatus::Live) => ("live", BadgeVariant::Success, true),
                Some(FeedStatus::Reconnecting) => ("reconnecting", BadgeVariant::Warning, false),
                Some(FeedStatus::Error) => ("error", BadgeVariant::Destructive, false),
                _ => ("connecting", BadgeVariant::Secondary, false),
            };
            let name = venue.label();
            if live {
                view! {
                    <Badge variant=variant class="uppercase">
                        <PulseDot />
                        {format!("{name} {label}")}
                    </Badge>
                }.into_any()
            } else {
                view! { <Badge variant=variant class="uppercase">{format!("{name} {label}")}</Badge> }.into_any()
            }
        }}
    }
}

// ---------------------------------------------------------------------------
// Stats strip
// ---------------------------------------------------------------------------

#[component]
fn StatsStrip(book: Memo<Option<Arc<BookData>>>, tick: RwSignal<(f64, i8)>) -> impl IntoView {
    let stats: Memo<BookStats> =
        Memo::new(move |_| book.get().map(|d| d.stats.clone()).unwrap_or_else(dash_stats));
    let mid = Memo::new(move |_| stats.get().mid.clone());
    let mid_class = {
        let tick = tick.clone();
        Memo::new(move |_| {
            let dir = tick.get().1;
            if dir > 0 { "text-emerald-300".to_string() }
            else if dir < 0 { "text-rose-300".to_string() }
            else { "text-foreground".to_string() }
        })
    };
    let arrow = {
        let tick = tick.clone();
        Memo::new(move |_| {
            let t = tick.get();
            if t.1 > 0 { "\u{25B2}".to_string() } else if t.1 < 0 { "\u{25BC}".to_string() } else { String::new() }
        })
    };
    let best_bid = Memo::new(move |_| stats.get().best_bid.clone());
    let best_ask = Memo::new(move |_| stats.get().best_ask.clone());
    let bid_sub = Memo::new(move |_| {
        stats.get()
            .bb_v
            .map(|v| v.label().to_string())
            .unwrap_or_default()
    });
    let ask_sub = Memo::new(move |_| {
        stats.get()
            .ba_v
            .map(|v| v.label().to_string())
            .unwrap_or_default()
    });
    let spread_sub = Memo::new(move |_| {
        let s = stats.get();
        format!("spread {} · {} bps", s.spread, s.spread_bps)
    });

    view! {
        <div class="grid grid-cols-2 md:grid-cols-4 gap-4">
            <StatTile label="Mid Price (USD)" value=mid value_class=mid_class sub=arrow>
                {move || view! { <span class="text-muted-foreground">{spread_sub.get()}</span> }}
            </StatTile>
            <StatTile
                label="Best Bid (cross-venue)"
                value=best_bid
                value_class=Memo::new(|_| "text-emerald-300".to_string())
                sub=bid_sub
            >
                {move || view! { <span></span> }}
            </StatTile>
            <StatTile
                label="Best Ask (cross-venue)"
                value=best_ask
                value_class=Memo::new(|_| "text-rose-300".to_string())
                sub=ask_sub
            >
                {move || view! { <span></span> }}
            </StatTile>
            <ImbalanceTile stats />
        </div>
    }
}

#[component]
fn ImbalanceTile(stats: Memo<BookStats>) -> impl IntoView {
    let imb = Memo::new(move |_| {
        stats.get().imbalance.parse::<f64>().unwrap_or(0.0).clamp(-1.0, 1.0)
    });
    let bid_notional = Memo::new(move |_| abbrev_money(&stats.get().near_bid_notional));
    let ask_notional = Memo::new(move |_| abbrev_money(&stats.get().near_ask_notional));
    let bid_pct = Memo::new(move |_| (imb.get() + 1.0) / 2.0 * 100.0);
    view! {
        <Card class="p-4 gap-0 flex flex-col justify-between rounded-xl">
            <span class="text-xs font-medium uppercase tracking-wider text-muted-foreground">
                "Depth Imbalance (0.5% band)"
            </span>
            <div class="mt-2 font-mono tabular-nums text-sm">
                <span class="text-emerald-300">{move || format!("${}", bid_notional.get())}</span>
                <span class="text-muted-foreground">" / "</span>
                <span class="text-rose-300">{move || format!("${}", ask_notional.get())}</span>
            </div>
            <div class="mt-2">
                <div class="flex h-2 w-full overflow-hidden rounded-full bg-rose-500/20">
                    <div
                        class="depth-bar h-full bg-emerald-500/70 rounded-l-full"
                        style=move || format!("width: {:.1}%", bid_pct.get())
                    ></div>
                </div>
                <div class="mt-1 flex justify-between text-[10px] font-mono text-muted-foreground">
                    <span class="text-emerald-400/80">{move || format!("{:.0}% bids", bid_pct.get())}</span>
                    <span class="text-rose-400/80">{move || format!("{:.0}% asks", 100.0 - bid_pct.get())}</span>
                </div>
            </div>
        </Card>
    }
}

// ---------------------------------------------------------------------------
// Ladder card
// ---------------------------------------------------------------------------

#[component]
fn LadderCard(
    book: Memo<Option<Arc<BookData>>>,
    tab_sig: RwSignal<TabView>,
    tab: TabView,
    venue_tabs: Memo<Vec<TabView>>,
    ask_rows: Memo<Vec<Row>>,
    bid_rows: Memo<Vec<Row>>,
    stats: Memo<BookStats>,
    show_venue: bool,
    loaded: Memo<bool>,
) -> impl IntoView {
    let title = Memo::new(move |_| {
        format!(
            "{} — {}",
            tab.label(),
            book.get().map(|d| d.market.label().to_string()).unwrap_or_default()
        )
    });
    view! {
        <Card>
            <CardHeader class="pb-3">
                <div class="flex items-center justify-between gap-3 flex-wrap">
                    <div class="flex flex-col gap-0.5">
                        <CardTitle class="font-mono text-base">{move || title.get()}</CardTitle>
                        <CardDescription>
                            {format!(
                                "{} levels per side streamed — full depth aggregated server-side",
                                DISPLAY_DEPTH,
                            )}
                        </CardDescription>
                    </div>
                    <LadderTabs tab=tab_sig venue_tabs />
                </div>
            </CardHeader>
            <CardContent>
                <Ladder
                    ask_rows=ask_rows
                    bid_rows=bid_rows
                    stats=stats
                    show_venue=show_venue
                    loaded=loaded
                />
            </CardContent>
        </Card>
    }
}

#[component]
fn LadderTabs(tab: RwSignal<TabView>, venue_tabs: Memo<Vec<TabView>>) -> impl IntoView {
    view! {
        <div class="flex items-center rounded-lg border border-border bg-card p-1 gap-1 flex-wrap max-w-[420px]">
            <For each=move || venue_tabs.get() key=|t| t.key() let: t>
                {let tab = tab.clone();
                let active = move || tab.get() == t;
                let label = t.label();
                view! {
                    <button
                        class=move || format!(
                            "h-7 rounded-md px-2.5 text-[12px] font-medium transition-colors cursor-pointer {}",
                            if active() { "bg-secondary text-secondary-foreground" } else { "text-muted-foreground hover:text-foreground hover:bg-muted" },
                        )
                        on:click=move |_| tab.set(t)
                    >
                        {label}
                    </button>
                }}
            </For>
        </div>
    }
}

// ---------------------------------------------------------------------------
// Venue strip
// ---------------------------------------------------------------------------

#[component]
fn VenueStrip(book: Memo<Option<Arc<BookData>>>) -> impl IntoView {
    let count: Memo<usize> = Memo::new(move |_| {
        book.get().map(|d| d.venues.len()).unwrap_or(0)
    });
    view! {
        <div class="flex flex-col gap-3">
            <div class="flex items-center justify-between px-1">
                <span class="text-xs font-medium uppercase tracking-wider text-muted-foreground">
                    {move || format!("Venue books — {} live", count.get())}
                </span>
                <span class="text-[11px] text-muted-foreground font-mono">
                    "native quotes · USD-normalized consolidation"
                </span>
            </div>
            <div class="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-3 gap-3">
                <For each=move || VENUES.to_vec() key=|v| v.index() let: v>
                    <VenueCard book venue=v />
                </For>
            </div>
        </div>
    }
}

/// Compact per-venue summary for the venue cards.
#[derive(Clone, PartialEq)]
struct VenueSummary {
    market: String,
    status: FeedStatus,
    msg_per_sec: f32,
    levels: usize,
    bid_px: String,
    ask_px: String,
    bid_sz: String,
    live: bool,
}

#[component]
fn VenueCard(book: Memo<Option<Arc<BookData>>>, venue: Venue) -> impl IntoView {
    let view_data: Memo<Option<VenueSummary>> = Memo::new(move |_| {
        let d = book.get()?;
        let vb = d.venues.get(&venue)?;
        Some(VenueSummary {
            market: d.market.label().to_string(),
            status: vb.health.status,
            msg_per_sec: vb.health.msg_per_sec,
            levels: vb.health.levels,
            bid_px: vb.bids.first().map(|l| l.px.clone()).unwrap_or("\u{2014}".into()),
            ask_px: vb.asks.first().map(|l| l.px.clone()).unwrap_or("\u{2014}".into()),
            bid_sz: vb.bids.first().map(|l| l.sz.clone()).unwrap_or("\u{2014}".into()),
            live: true,
        })
    });
    let dot = venue_dot(venue);
    let name = venue.label();
    let short = venue.short();
    let quote = match venue.quote() {
        ob_core::Quote::Usdt => "USDT",
        ob_core::Quote::Usd => "USD",
    };
    view! {
        <Card class="p-4 rounded-xl">
            <div class="flex items-center justify-between">
                <div class="flex items-center gap-2 min-w-0">
                    <span class=format!("h-2 w-2 rounded-full {dot}")></span>
                    <span class="text-sm font-semibold truncate">{name}</span>
                    <span class="rounded border border-border px-1 text-[9px] font-mono text-muted-foreground shrink-0">
                        {short}
                    </span>
                    <span class="text-[10px] font-mono text-muted-foreground shrink-0 hidden sm:inline">
                        {quote}
                    </span>
                </div>
                <Show
                    when=move || view_data.get().map(|v| v.status == FeedStatus::Live).unwrap_or(false)
                    fallback=move || view! { <Badge variant=BadgeVariant::Warning class="uppercase">"no feed"</Badge> }
                >
                    <Badge variant=BadgeVariant::Secondary class="font-mono">
                        {move || format!("{:.1} msg/s", view_data.get().map(|v| v.msg_per_sec).unwrap_or(0.0))}
                    </Badge>
                </Show>
            </div>
            <div class="mt-3 grid grid-cols-4 gap-2 text-xs font-mono tabular-nums">
                <div class="flex flex-col gap-1">
                    <span class="text-[10px] uppercase text-muted-foreground tracking-wider">"Bid"</span>
                    <span class="text-emerald-300 font-medium">
                        {move || view_data.get().map(|v| v.bid_px.clone()).unwrap_or_default()}
                    </span>
                </div>
                <div class="flex flex-col gap-1">
                    <span class="text-[10px] uppercase text-muted-foreground tracking-wider">"Ask"</span>
                    <span class="text-rose-300 font-medium">
                        {move || view_data.get().map(|v| v.ask_px.clone()).unwrap_or_default()}
                    </span>
                </div>
                <div class="flex flex-col gap-1">
                    <span class="text-[10px] uppercase text-muted-foreground tracking-wider">"Bid Sz"</span>
                    <span class="text-zinc-300 truncate">
                        {move || view_data.get().map(|v| v.bid_sz.clone()).unwrap_or_default()}
                    </span>
                </div>
                <div class="flex flex-col gap-1">
                    <span class="text-[10px] uppercase text-muted-foreground tracking-wider">"Levels"</span>
                    <span class="text-zinc-300">
                        {move || format!("{}", view_data.get().map(|v| v.levels).unwrap_or(0))}
                    </span>
                </div>
            </div>
            <div class="mt-2 text-[10px] font-mono text-muted-foreground truncate">
                {move || view_data.get().map(|v| v.market).unwrap_or_else(|| "waiting\u{2026}".into())}
            </div>
        </Card>
    }
}

/// Accent dot color per venue.
fn venue_dot(v: Venue) -> &'static str {
    match v {
        Venue::Hyperliquid => "bg-amber-400",
        Venue::Lighter => "bg-teal-400",
        Venue::Binance => "bg-yellow-400",
        Venue::Bybit => "bg-orange-400",
        Venue::Okx => "bg-sky-400",
        Venue::Kraken => "bg-violet-400",
        Venue::Coinbase => "bg-blue-400",
        Venue::Bitstamp => "bg-emerald-400",
        Venue::Gate => "bg-rose-400",
    }
}

// ---------------------------------------------------------------------------
// Footer
// ---------------------------------------------------------------------------

#[component]
fn Footer(sig: Signals) -> impl IntoView {
    view! {
        <footer class="border-t border-border bg-background mt-auto">
            <div class="max-w-7xl mx-auto px-4 lg:px-6 h-12 flex items-center justify-between text-[11px] font-mono text-muted-foreground gap-4">
                <div class="flex items-center gap-3 min-w-0">
                    <span class="shrink-0">"100% Rust — Axum backend + Leptos/WASM frontend"</span>
                    <span class="hidden md:inline shrink-0 text-border">"|"</span>
                    <span class="hidden md:inline truncate">
                        "9 venues · 12 markets · arbitrage engine · trade tape · alerts · depth history"
                    </span>
                </div>
                <div class="flex items-center gap-3 shrink-0">
                    <span class="uppercase">{move || format!("link: {}", sig.link.get().label())}</span>
                </div>
            </div>
        </footer>
    }
}
