//! The orderbook ladder: asks, spread, bids, with cumulative depth bars.
//!
//! Rows are keyed by their full content (price, size, cumulative, depth %) so
//! Leptos' `For` re-renders ONLY the levels that actually changed and moves
//! the rest in place — smooth 10 Hz updates with minimal DOM churn.

use crate::components::{Badge, Skeleton};
use leptos::prelude::*;
use ob_core::{BookStats, Level, Venue};

/// Rows rendered per side.
pub const DISPLAY_DEPTH: usize = 22;

/// A display row with precomputed cumulative depth.
#[derive(Clone, PartialEq)]
pub struct Row {
    pub px: String,
    pub sz: String,
    pub n: Option<u32>,
    pub v: Option<Venue>,
    pub cum: String,
    pub pct: f32,
}

impl Row {
    fn key(&self) -> (String, String, String, u32) {
        (self.px.clone(), self.sz.clone(), self.cum.clone(), self.pct.to_bits())
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

/// Build display rows for one side. Input: best-first levels.
/// For asks the display order is reversed (worst at top, best near spread).
pub fn build_rows(levels: &[Level], reverse: bool) -> Vec<Row> {
    let mut rows: Vec<Row> = Vec::with_capacity(levels.len().min(DISPLAY_DEPTH));
    let mut cum = 0f64;
    for l in levels.iter().take(DISPLAY_DEPTH) {
        cum += l.sz.parse::<f64>().unwrap_or(0.0);
        rows.push(Row {
            px: l.px.clone(),
            sz: l.sz.clone(),
            n: l.n,
            v: l.v,
            cum: abbreviate(cum),
            pct: cum as f32,
        });
    }
    let total = rows.last().map(|r| r.pct).unwrap_or(0.0);
    if total > 0.0 {
        for r in &mut rows {
            r.pct = (r.pct / total * 100.0).clamp(0.0, 100.0);
        }
    }
    if reverse {
        rows.reverse();
    }
    rows
}

fn venue_label(v: &Option<Venue>) -> (&'static str, &'static str) {
    match v {
        Some(Venue::Hyperliquid) => ("HL", "text-amber-300/90 border-amber-900/60 bg-amber-950/40"),
        Some(Venue::Lighter) => ("LT", "text-teal-300/90 border-teal-900/60 bg-teal-950/40"),
        None => ("HL+LT", "text-zinc-300 border-zinc-700/60 bg-zinc-800/60"),
    }
}

fn view_ladder_row(row: Row, is_bid: bool, show_venue: bool) -> impl IntoView {
    let bar_color = if is_bid { "bg-emerald-500/15" } else { "bg-rose-500/15" };
    let px_color = if is_bid { "text-emerald-300" } else { "text-rose-300" };
    let bar_anchor = if is_bid { "left-0" } else { "right-0" };
    let (v_label, v_class) = venue_label(&row.v);
    let n_badge = row.n.map(|n| format!("x{n}")).unwrap_or_default();
    view! {
        <div class="relative grid grid-cols-[minmax(0,1.05fr)_minmax(0,0.95fr)_minmax(0,0.8fr)_auto] items-center gap-2 px-3 h-[22px] text-[13px] font-mono tabular-nums hover:bg-muted/60">
            <div
                class=format!("depth-bar absolute inset-y-[3px] {bar_anchor} rounded-sm {bar_color}")
                style=format!("width: {:.1}%", row.pct)
            ></div>
            <span class=format!("relative z-10 text-right {px_color}")>{row.px.clone()}</span>
            <span class="relative z-10 text-right text-zinc-300">{row.sz.clone()}</span>
            <span class="relative z-10 text-right text-muted-foreground">{row.cum.clone()}</span>
            <span class="relative z-10 w-14 flex justify-end">
                {move || {
                    if show_venue {
                        view! {
                            <span class=format!("rounded border px-1 py-px text-[10px] leading-none font-sans {v_class}")>
                                {v_label}
                            </span>
                        }.into_any()
                    } else if !n_badge.is_empty() {
                        view! {
                            <span class="text-[10px] text-muted-foreground">{n_badge.clone()}</span>
                        }.into_any()
                    } else {
                        view! { <span></span> }.into_any()
                    }
                }}
            </span>
        </div>
    }
}

/// Column headers.
#[component]
fn LadderHeader(show_venue: bool) -> impl IntoView {
    view! {
        <div class="grid grid-cols-[minmax(0,1.05fr)_minmax(0,0.95fr)_minmax(0,0.8fr)_auto] items-center gap-2 px-3 h-7 text-[11px] uppercase tracking-wider text-muted-foreground border-b border-border">
            <span class="text-right">Price</span>
            <span class="text-right">Size</span>
            <span class="text-right">Sum</span>
            <span class="w-14 text-right">{if show_venue { "Venue" } else { "Orders" }}</span>
        </div>
    }
}

/// The spread strip between asks and bids.
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

/// One side of the ladder.
#[component]
fn LadderSide(rows: Memo<Vec<Row>>, is_bid: bool, show_venue: bool) -> impl IntoView {
    view! {
        <div class="flex flex-col">
            <For each=move || rows.get() key=|r| r.key() let: row>
                {view_ladder_row(row, is_bid, show_venue)}
            </For>
        </div>
    }
}

/// Full ladder (asks + spread + bids) with skeletons while loading.
#[component]
pub fn Ladder(
    ask_rows: Memo<Vec<Row>>,
    bid_rows: Memo<Vec<Row>>,
    stats: Memo<BookStats>,
    show_venue: bool,
    loaded: Memo<bool>,
) -> impl IntoView {
    view! {
        <div class="flex flex-col">
            <LadderHeader show_venue />
            <Show
                when=move || loaded.get()
                fallback=move || {
                    view! {
                        <div class="flex flex-col gap-1.5 p-3">
                            <Skeleton class="h-[22px] w-full" />
                            <Skeleton class="h-[22px] w-11/12" />
                            <Skeleton class="h-[22px] w-full" />
                            <Skeleton class="h-[22px] w-10/12" />
                            <Skeleton class="h-[22px] w-full" />
                            <Skeleton class="h-[22px] w-11/12" />
                            <Skeleton class="h-[22px] w-full" />
                        </div>
                    }
                }
            >
                <div class="flex flex-col">
                    <LadderSide rows=ask_rows is_bid=false show_venue />
                    <SpreadStrip stats />
                    <LadderSide rows=bid_rows is_bid=true show_venue />
                </div>
            </Show>
        </div>
    }
}
