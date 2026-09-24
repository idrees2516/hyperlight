//! The orderbook: a classic pro-terminal mirrored layout with the best bid
//! and best ask displayed BIG, side by side, around a center seam.
//!
//! ```text
//! ┌──────────── BIDS ────────────┬────────────┬─────────── ASKS ───────────┐
//! │  BEST BID   3,000.10  ▮▮▮▮▮  │   mid      │  ▮▮▮▮▮   BEST ASK  3,000.40 │
//! │  3,000.00   size  cum        │  spread    │        3,000.50   size  cum │
//! │  ...levels descending...      │  bps       │      ...levels ascending... │
//! └───────────────────────────────┴────────────┴─────────────────────────────┘
//! ```
//!
//! Both sides show their best level at the top as a giant price, then depth
//! rows below (bids descending, asks ascending). Depth bars grow toward the
//! center seam. Venue chips (consolidated view) mark which venues rest at
//! each level. Rows are keyed by full content so Leptos re-renders only
//! changed levels — smooth 10 Hz updates with minimal DOM churn.

use crate::components::{Badge, Skeleton};
use leptos::prelude::*;
use ob_core::{BookStats, Level, Venue};

/// Depth rows rendered per side below the giant best row.
pub const DISPLAY_DEPTH: usize = 14;

/// Total rows used for the depth-bar scale (giant row + table rows).
const ROWS_FOR_SCALE: usize = DISPLAY_DEPTH + 1;

/// A display row with precomputed cumulative depth.
#[derive(Clone, PartialEq)]
pub struct Row {
    pub px: String,
    pub sz: String,
    pub n: Option<u32>,
    /// Venue bitmask (consolidated view only).
    pub v: Option<u8>,
    pub cum: String,
    pub pct: f32,
    /// True when this row is the giant best row.
    pub best: bool,
}

impl Row {
    fn key(&self) -> (String, String, String, u32, bool) {
        (
            self.px.clone(),
            self.sz.clone(),
            self.cum.clone(),
            self.pct.to_bits(),
            self.best,
        )
    }
}

fn abbreviate(v: f64) -> String {
    if v.abs() >= 1_000_000.0 {
        format!("{:.2}M", v / 1_000_000.0)
    } else if v.abs() >= 10_000.0 {
        format!("{:.1}K", v / 1_000.0)
    } else if v.abs() >= 1_000.0 {
        format!("{:.2}K", v / 1_000.0)
    } else {
        format!("{v:.2}")
    }
}

/// Build display rows for one side. Input: best-first levels (bids
/// descending, asks ascending — the caller does NOT reverse; both sides
/// render best-at-top). The first row is flagged as the giant best row.
pub fn build_rows(levels: &[Level]) -> Vec<Row> {
    let mut rows: Vec<Row> = Vec::with_capacity(levels.len().min(DISPLAY_DEPTH + 1));
    let mut cum = 0f64;
    for (i, l) in levels.iter().take(DISPLAY_DEPTH + 1).enumerate() {
        cum += l.sz.parse::<f64>().unwrap_or(0.0);
        rows.push(Row {
            px: l.px.clone(),
            sz: l.sz.clone(),
            n: l.n,
            v: l.v,
            cum: abbreviate(cum),
            pct: cum as f32,
            best: i == 0,
        });
    }
    let total = rows.last().map(|r| r.pct).unwrap_or(0.0);
    if total > 0.0 {
        for r in &mut rows {
            r.pct = (r.pct / total * 100.0).clamp(0.0, 100.0);
        }
    }
    rows
}

/// Venue chip color for a venue short code.
fn venue_chip_class(v: Venue) -> &'static str {
    match v {
        Venue::Hyperliquid => "text-amber-300/90 border-amber-900/60 bg-amber-950/40",
        Venue::Lighter => "text-teal-300/90 border-teal-900/60 bg-teal-950/40",
        Venue::Binance => "text-yellow-300/90 border-yellow-900/60 bg-yellow-950/40",
        Venue::Bybit => "text-orange-300/90 border-orange-900/60 bg-orange-950/40",
        Venue::Okx => "text-sky-300/90 border-sky-900/60 bg-sky-950/40",
        Venue::Kraken => "text-violet-300/90 border-violet-900/60 bg-violet-950/40",
        Venue::Coinbase => "text-blue-300/90 border-blue-900/60 bg-blue-950/40",
        Venue::Bitstamp => "text-emerald-300/90 border-emerald-900/60 bg-emerald-950/40",
        Venue::Gate => "text-rose-300/90 border-rose-900/60 bg-rose-950/40",
    }
}

/// Venue chips from a consolidated mask (cap at 3 + overflow count).
fn chips_of(v: Option<u8>) -> Vec<(String, &'static str)> {
    v.map(|mask| {
            let vs = Venue::from_mask(mask);
            let overflow = vs.len().saturating_sub(3);
            let mut out: Vec<(String, &'static str)> = vs
                .iter()
                .take(3)
                .map(|v| (v.short().to_string(), venue_chip_class(*v)))
                .collect();
            if overflow > 0 {
                out.push((format!("+{overflow}"), "text-zinc-300 border-zinc-700/60 bg-zinc-800/60"));
            }
            out
        })
        .unwrap_or_default()
}

// ---------------------------------------------------------------------------
// One side of the mirrored book
// ---------------------------------------------------------------------------

#[component]
fn BigSide(rows: Memo<Vec<Row>>, is_bid: bool, show_venue: bool) -> impl IntoView {
    view! {
        <div class="flex-1 min-w-0 flex flex-col">
            // Column headers (mirrored layout: price near the seam).
            <div class=move || format!(
                "grid {} items-center gap-2 px-3 h-7 text-[11px] uppercase tracking-wider text-muted-foreground border-b border-border",
                if is_bid { "grid-cols-[minmax(0,0.9fr)_minmax(0,1fr)_minmax(0,0.7fr)_minmax(0,64px)]" } else { "grid-cols-[minmax(0,64px)_minmax(0,0.7fr)_minmax(0,1fr)_minmax(0,0.9fr)]" },
            )>
                <Show when=move || !is_bid fallback=move || view! { <span class="text-right"></span> }>
                    <span class="text-center text-[9px] font-sans">""</span>
                </Show>
                {move || if is_bid {
                    view! {
                        <span class="text-right">"Size"</span>
                        <span class="text-right">"Sum"</span>
                        <span class="text-right">"Price"</span>
                    }.into_any()
                } else {
                    view! {
                        <span class="text-left">"Price"</span>
                        <span class="text-left">"Sum"</span>
                        <span class="text-left">"Size"</span>
                    }.into_any()
                }}
                <Show when=move || is_bid fallback=move || view! { <span></span> }>
                    <span></span>
                </Show>
            </div>
            <For each=move || rows.get() key=|r| r.key() let: row>
                {big_side_row(row, is_bid, show_venue)}
            </For>
        </div>
    }
}

fn big_side_row(row: Row, is_bid: bool, show_venue: bool) -> impl IntoView {
    let bar_color = if is_bid { "bg-emerald-500/15" } else { "bg-rose-500/15" };
    let px_color = if is_bid { "text-emerald-300" } else { "text-rose-300" };
    // Bars grow toward the center seam: bids anchor right, asks anchor left.
    let bar_anchor = if is_bid { "right-0" } else { "left-0" };
    let chips = chips_of(row.v);
    let n_badge = row.n.map(|n| format!("x{n}")).unwrap_or_default();

    if row.best {
        // The giant best bid / best ask row.
        let big_color = if is_bid { "text-emerald-400" } else { "text-rose-400" };
        let side_label = if is_bid { "BEST BID" } else { "BEST ASK" };
        let align = if is_bid {
            "flex flex-col items-end text-right"
        } else {
            "flex flex-col items-start text-left"
        };
        let big_px_class = format!(
            "text-[30px] leading-none font-mono tabular-nums font-bold tracking-tight {big_color} truncate"
        );
        let bar_class = format!("depth-bar absolute inset-y-1 {bar_anchor} rounded-sm {bar_color}");
        let bar_style = format!("width: {:.1}%", row.pct.max(12.0));
        let chip_views: Vec<leptos::prelude::AnyView> = chips
            .iter()
            .map(|(label, class)| {
                let cls = format!(
                    "rounded border px-1 py-px text-[9px] leading-none font-sans {class}"
                );
                let l = label.clone();
                view! { <span class=cls>{l}</span> }.into_any()
            })
            .collect();
        let badge_view: leptos::prelude::AnyView = if show_venue && !chip_views.is_empty() {
            view! { <span class="flex gap-0.5 justify-end flex-wrap">{chip_views}</span> }.into_any()
        } else if !n_badge.is_empty() {
            let b = n_badge.clone();
            view! { <span class="text-[10px] text-muted-foreground">{b}</span> }.into_any()
        } else {
            view! { <span class="h-[12px]"></span> }.into_any()
        };
        let px = row.px.clone();
        let sz = crate::model::fmt_num(&row.sz);
        view! {
            <div class="relative flex items-center gap-3 px-3 py-2.5 border-b border-border/70">
                <div class=bar_class style=bar_style></div>
                <div class=format!("relative z-10 flex-1 min-w-0 {align}")>
                    <span class="text-[10px] uppercase tracking-widest text-muted-foreground font-sans font-medium">
                        {side_label}
                    </span>
                    <span class=big_px_class>{px}</span>
                </div>
                <div class="relative z-10 flex flex-col items-end gap-0.5 w-[118px] shrink-0">
                    <span class="text-[11px] uppercase tracking-wider text-muted-foreground font-sans">"sz"</span>
                    <span class="text-base font-mono tabular-nums text-zinc-100 font-semibold">
                        {sz}
                    </span>
                    {badge_view}
                </div>
            </div>
        }
        .into_any()
    } else {
        // Regular depth row (mirrored column order). Children are built as a
        // Vec so the grid's direct spans stay direct children of the row.
        let h = "h-[21px] text-[12.5px] font-mono tabular-nums";
        let cols = if is_bid {
            "grid-cols-[minmax(0,0.9fr)_minmax(0,1fr)_minmax(0,0.7fr)_minmax(0,64px)]"
        } else {
            "grid-cols-[minmax(0,64px)_minmax(0,0.7fr)_minmax(0,1fr)_minmax(0,0.9fr)]"
        };
        let row_class = format!("relative grid {cols} items-center gap-2 px-3 {h} hover:bg-muted/60");
        let bar_class = format!("depth-bar absolute inset-y-[3px] {bar_anchor} rounded-sm {bar_color}");
        let bar_style = format!("width: {:.1}%", row.pct);
        let chip_views: Vec<leptos::prelude::AnyView> = chips
            .iter()
            .map(|(label, class)| {
                let cls = format!(
                    "rounded border px-1 py-px text-[9px] leading-none font-sans {class}"
                );
                let l = label.clone();
                view! { <span class=cls>{l}</span> }.into_any()
            })
            .collect();
        let px_cls = format!("relative z-10 {}", if is_bid { "text-right" } else { "text-left" });
        let px_cls = format!("{px_cls} {px_color}");
        let cum_cls = format!(
            "relative z-10 {} text-muted-foreground",
            if is_bid { "text-right" } else { "text-left" }
        );
        let sz_cls = format!(
            "relative z-10 {} text-zinc-300",
            if is_bid { "text-right" } else { "text-left" }
        );
        let chips_cls = format!(
            "relative z-10 flex {} gap-0.5",
            if is_bid { "justify-end" } else { "justify-start" }
        );
        let px = row.px.clone();
        let cum = row.cum.clone();
        let sz = row.sz.clone();
        let cells: Vec<leptos::prelude::AnyView> = if is_bid {
            vec![
                view! { <span class=sz_cls>{sz}</span> }.into_any(),
                view! { <span class=cum_cls>{cum}</span> }.into_any(),
                view! { <span class=px_cls>{px}</span> }.into_any(),
                view! { <span class=chips_cls>{chip_views}</span> }.into_any(),
            ]
        } else {
            vec![
                view! { <span class=chips_cls>{chip_views}</span> }.into_any(),
                view! { <span class=px_cls>{px}</span> }.into_any(),
                view! { <span class=cum_cls>{cum}</span> }.into_any(),
                view! { <span class=sz_cls>{sz}</span> }.into_any(),
            ]
        };
        view! {
            <div class=row_class>
                <div class=bar_class style=bar_style></div>
                {cells}
            </div>
        }
        .into_any()
    }
}

// ---------------------------------------------------------------------------
// Center seam: mid / spread / imbalance
// ---------------------------------------------------------------------------

#[component]
fn CenterSeam(stats: Memo<BookStats>) -> impl IntoView {
    let crossed = Memo::new(move |_| {
        let s = stats.get();
        s.best_bid
            .parse::<f64>()
            .ok()
            .zip(s.best_ask.parse::<f64>().ok())
            .map(|(b, a)| b > a)
            .unwrap_or(false)
    });
    let imb = Memo::new(move |_| {
        stats
            .get()
            .imbalance
            .parse::<f64>()
            .unwrap_or(0.0)
            .clamp(-1.0, 1.0)
    });
    view! {
        <div class="w-[112px] shrink-0 flex flex-col items-center justify-center gap-2 border-x border-border bg-muted/20 px-2 py-3">
            <div class="flex flex-col items-center gap-0.5">
                <span class="text-[9px] uppercase tracking-widest text-muted-foreground">"mid"</span>
                <span class="text-lg font-mono tabular-nums font-semibold leading-none">
                    {move || stats.get().mid.clone()}
                </span>
            </div>
            <div class="flex flex-col items-center gap-0.5">
                <span class="text-[9px] uppercase tracking-widest text-muted-foreground">"spread"</span>
                <span class="text-xs font-mono tabular-nums">
                    {move || format!("{} bps", stats.get().spread_bps)}
                </span>
            </div>
            {move || {
                if crossed.get() {
                    view! {
                        <Badge variant=crate::components::BadgeVariant::Warning class="text-[9px] uppercase">
                            "crossed"
                        </Badge>
                    }.into_any()
                } else {
                    view! { <span class="h-[22px]"></span> }.into_any()
                }
            }}
            <div class="w-full px-1">
                <div class="flex h-1.5 w-full overflow-hidden rounded-full bg-rose-500/20">
                    <div
                        class="depth-bar h-full bg-emerald-500/70 rounded-l-full"
                        style=move || format!("width: {:.1}%", (imb.get() + 1.0) / 2.0 * 100.0)
                    ></div>
                </div>
                <div class="mt-1 flex justify-between text-[8px] font-mono text-muted-foreground">
                    <span class="text-emerald-400/80">"bid"</span>
                    <span class="text-rose-400/80">"ask"</span>
                </div>
            </div>
        </div>
    }
}

// ---------------------------------------------------------------------------
// Full mirrored book
// ---------------------------------------------------------------------------

/// Full big book (bids | seam | asks) with skeletons while loading.
#[component]
pub fn Ladder(
    bid_rows: Memo<Vec<Row>>,
    ask_rows: Memo<Vec<Row>>,
    stats: Memo<BookStats>,
    show_venue: bool,
    loaded: Memo<bool>,
) -> impl IntoView {
    view! {
        <div class="flex flex-col">
            <Show
                when=move || loaded.get()
                fallback=move || {
                    view! {
                        <div class="flex flex-col gap-1.5 p-3">
                            <Skeleton class="h-[54px] w-full" />
                            <Skeleton class="h-[21px] w-11/12" />
                            <Skeleton class="h-[21px] w-full" />
                            <Skeleton class="h-[21px] w-10/12" />
                            <Skeleton class="h-[21px] w-full" />
                            <Skeleton class="h-[21px] w-11/12" />
                        </div>
                    }
                }
            >
                <div class="flex items-stretch">
                    <BigSide rows=bid_rows is_bid=true show_venue />
                    <CenterSeam stats />
                    <BigSide rows=ask_rows is_bid=false show_venue />
                </div>
            </Show>
        </div>
    }
}

/// The spread strip (kept for the stats strip / compact surfaces).
#[component]
pub fn SpreadStrip(stats: Memo<BookStats>) -> impl IntoView {
    let crossed = Memo::new(move |_| {
        let s = stats.get();
        s.best_bid
            .parse::<f64>()
            .ok()
            .zip(s.best_ask.parse::<f64>().ok())
            .map(|(b, a)| b > a)
            .unwrap_or(false)
    });
    view! {
        <div class="flex items-center justify-between gap-2 px-3 h-10 border-y border-border bg-muted/30">
            <div class="flex items-baseline gap-2">
                <span class="text-lg font-mono tabular-nums font-semibold">
                    {move || stats.get().mid}
                </span>
                <span class="text-[11px] uppercase tracking-wider text-muted-foreground">mid</span>
            </div>
            <div class="flex items-center gap-2 text-xs font-mono tabular-nums">
                <span class="text-muted-foreground">spread</span>
                <span class=move || {
                    if crossed.get() { "text-amber-300".to_string() } else { "text-zinc-200".to_string() }
                }>
                    {move || format!("{} ({} bps)", stats.get().spread, stats.get().spread_bps)}
                </span>
                {move || {
                    if crossed.get() {
                        view! { <Badge variant=crate::components::BadgeVariant::Warning>"CROSSED"</Badge> }
                            .into_any()
                    } else {
                        view! { <span class="hidden"></span> }.into_any()
                    }
                }}
            </div>
        </div>
    }
}
