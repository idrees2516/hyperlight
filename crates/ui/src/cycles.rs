//! Multi-hop swap arbitrage panel.
//!
//! Renders the graph telemetry (nodes / edges / Bellman-Ford negative-cycle
//! detection), live multi-hop cycle opportunities as route chip chains
//! (`usd → ETH@Kraken → USDT@Binance → SOL@Bybit → usd`), and the cycle
//! execution log (fills + latency expirations).

use crate::components::*;
use crate::model::fmt_num;
use crate::ws::Signals;
use leptos::prelude::*;
use ob_core::{CycleFill, CycleHop, CycleOpportunity, Venue};

/// How many recent fills the log renders.
const FILL_ROWS: usize = 24;

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

/// One hop chip: `ETH@KR` with the venue's accent color.
fn hop_chip(h: &CycleHop) -> impl IntoView {
    let label = format!("{}@{}", h.asset, h.venue.short());
    let class = venue_chip_class(h.venue);
    let dir_arrow = if h.dir == "buy" { "\u{2192}" } else { "\u{2192}" };
    view! {
        <span class=format!("rounded border px-1.5 py-px text-[10px] leading-tight font-mono font-medium {class}")>
            {label}
        </span>
        <span class="text-muted-foreground text-[10px]">{dir_arrow}</span>
    }
}

/// The full route chain: `usd → hops… → usd`.
fn route_chain(hops: &[CycleHop]) -> impl IntoView {
    view! {
        <span class="flex items-center gap-1 flex-wrap">
            <span class="rounded border border-zinc-700/60 bg-zinc-800/60 px-1.5 py-px text-[10px] leading-tight font-mono text-zinc-300 font-medium">
                "usd"
            </span>
            <span class="text-muted-foreground text-[10px]">"\u{2192}"</span>
            {hops.iter().map(|h| hop_chip(h).into_any()).collect::<Vec<_>>()}
            <span class="rounded border border-zinc-700/60 bg-zinc-800/60 px-1.5 py-px text-[10px] leading-tight font-mono text-zinc-300 font-medium">
                "usd"
            </span>
        </span>
    }
}

/// One rendered row of the opportunities table (plain fn: called directly).
fn cycle_opp_row(opp: CycleOpportunity) -> impl IntoView {
    let net_bps: f64 = opp.net_bps.parse().unwrap_or(0.0);
    let edge_class = if net_bps >= 8.0 {
        "text-emerald-300 font-semibold"
    } else if net_bps >= 4.0 {
        "text-emerald-400"
    } else {
        "text-zinc-300"
    };
    let profit: f64 = opp.profit_usd.parse().unwrap_or(0.0);
    let hops_count = opp.hops.len();
    let entry = fmt_num(&opp.entry_usd);
    let net = fmt_num(&opp.net_bps);
    view! {
        <div class="grid grid-cols-[minmax(0,1fr)_84px_74px_74px_54px] items-center gap-2 px-3 py-1.5 text-xs font-mono tabular-nums border-b border-border/40 hover:bg-muted/50">
            {route_chain(&opp.hops)}
            <span class="text-right text-zinc-300">{entry}</span>
            <span class=move || format!("text-right {edge_class}")>{net}</span>
            <span class="text-right text-emerald-300">{format!("${:.2}", profit)}</span>
            <span class="text-right text-muted-foreground">{hops_count}</span>
        </div>
    }
}

fn cycle_fill_row(fill: CycleFill) -> impl IntoView {
    let filled = fill.status == "filled";
    let status_class = if filled { "text-emerald-300" } else { "text-amber-300/80" };
    let hops: Vec<String> = fill
        .hops
        .iter()
        .map(|h| format!("{}@{}", h.asset, h.venue.short()))
        .collect();
    let route_disp = if hops.is_empty() {
        fill.route.clone()
    } else {
        format!("usd \u{2192} {} \u{2192} usd", hops.join(" \u{2192} "))
    };
    let detected = fmt_num(&fill.detected_net_bps);
    let pnl_disp = format!("${:.2}", fill.profit_usd.parse::<f64>().unwrap_or(0.0));
    let pnl_class = if filled { "text-right text-emerald-300" } else { "text-right text-zinc-500" };
    let time = crate::model::fmt_hms(fill.ts);
    view! {
        <div class="grid grid-cols-[52px_minmax(0,1fr)_70px_70px_66px] items-center gap-2 px-3 py-1 text-[11px] font-mono tabular-nums border-b border-border/40">
            <span class=status_class>{if filled { "fill" } else { "expired" }}</span>
            <span class="truncate text-zinc-300">{route_disp}</span>
            <span class="text-right text-muted-foreground">{detected}</span>
            <span class=pnl_class>{pnl_disp}</span>
            <span class="text-right text-muted-foreground">{time}</span>
        </div>
    }
}

#[component]
pub fn CyclesCard(sig: Signals) -> impl IntoView {
    let stats = Memo::new(move |_| sig.cycles.get().stats.clone());
    let opps: Memo<Vec<CycleOpportunity>> =
        Memo::new(move |_| sig.cycles.get().opportunities.clone());
    let fills: Memo<Vec<CycleFill>> = Memo::new(move |_| {
        let mut f = sig.cycles.get().fills.clone();
        f.reverse(); // newest first
        f.truncate(FILL_ROWS);
        f
    });

    let open_cycles = Memo::new(move |_| stats.get().open_cycles.to_string());
    let pnl = Memo::new(move |_| stats.get().pnl_usd.clone());
    let fills_n = Memo::new(move |_| stats.get().fills);
    let expired_n = Memo::new(move |_| stats.get().expired);
    let best_bps = Memo::new(move |_| stats.get().best_net_bps.clone());
    let graph = Memo::new(move |_| {
        let s = stats.get();
        format!("{} nodes · {} edges", s.graph_nodes, s.graph_edges)
    });
    let bf = Memo::new(move |_| {
        let s = stats.get();
        if s.negative_cycles {
            format!("negative cycle detected · best touch rate {}x", s.graph_best_rate)
        } else {
            "no negative cycle (Bellman-Ford)".to_string()
        }
    });

    view! {
        <Card>
            <CardHeader class="pb-3">
                <div class="flex items-center justify-between gap-3 flex-wrap">
                    <div class="flex flex-col gap-0.5">
                        <CardTitle class="font-mono text-base">
                            "Multi-Hop Swap Arbitrage"
                            <span class="text-muted-foreground font-normal">" — negative-cycle graph engine"</span>
                        </CardTitle>
                        <CardDescription>
                            "Bellman-Ford detection on the full venue/asset swap graph · bounded simple-cycle DFS · exact marginal depth-walks around every leg · latency-modeled paper execution"
                        </CardDescription>
                    </div>
                    <div class="flex flex-col items-end gap-1">
                        <Badge variant=BadgeVariant::Outline class="font-mono text-[10px]">
                            {move || graph.get()}
                        </Badge>
                        {move || {
                            let s = stats.get();
                            let (v, label) = if s.negative_cycles {
                                (BadgeVariant::Success, format!("negative cycle detected · best touch rate {}x", s.graph_best_rate))
                            } else {
                                (BadgeVariant::Secondary, "no negative cycle (Bellman-Ford)".to_string())
                            };
                            view! {
                                <Badge variant=v class="font-mono text-[10px]">
                                    {label}
                                </Badge>
                            }.into_any()
                        }}
                    </div>
                </div>
            </CardHeader>
            <CardContent class="flex flex-col gap-4">
                <div class="grid grid-cols-2 md:grid-cols-5 gap-3">
                    <StatTile
                        label="Open Cycles"
                        value=open_cycles
                        value_class=Memo::new(|_| "text-foreground".to_string())
                        sub=Memo::new(|_| "live multi-hop routes".to_string())
                    >
                        {move || view! { <span></span> }}
                    </StatTile>
                    <StatTile
                        label="Cycle P&L (paper)"
                        value=pnl
                        value_class=Memo::new(|_| "text-emerald-300".to_string())
                        sub=Memo::new(move |_| format!("{} fills · {} expired", fills_n.get(), expired_n.get()))
                    >
                        {move || view! { <span></span> }}
                    </StatTile>
                    <StatTile
                        label="Best Net Edge"
                        value=best_bps
                        value_class=Memo::new(|_| "text-emerald-300".to_string())
                        sub=Memo::new(|_| "bps after fees".to_string())
                    >
                        {move || view! { <span></span> }}
                    </StatTile>
                </div>

                // Opportunities table.
                <div class="rounded-lg border border-border overflow-hidden">
                    <div class="grid grid-cols-[minmax(0,1fr)_84px_74px_74px_54px] items-center gap-2 px-3 h-8 text-[10px] uppercase tracking-wider text-muted-foreground bg-muted/40 border-b border-border">
                        <span>"Route (usd → hops → usd)"</span>
                        <span class="text-right">"Entry"</span>
                        <span class="text-right">"Net bps"</span>
                        <span class="text-right">"Profit"</span>
                        <span class="text-right">"Hops"</span>
                    </div>
                    <Show
                        when=move || !opps.get().is_empty()
                        fallback=move || {
                            view! {
                                <div class="px-3 py-6 text-center text-xs text-muted-foreground font-mono">
                                    "no profitable multi-hop cycles at current books (scanning at 2 Hz)"
                                </div>
                            }
                        }
                    >
                        <For each=move || opps.get() key=|o| o.route.clone() let: opp>
                            {cycle_opp_row(opp).into_any()}
                        </For>
                    </Show>
                </div>

                // Execution log.
                <div class="rounded-lg border border-border overflow-hidden">
                    <div class="grid grid-cols-[52px_minmax(0,1fr)_70px_70px_66px] items-center gap-2 px-3 h-7 text-[10px] uppercase tracking-wider text-muted-foreground bg-muted/40 border-b border-border">
                        <span>"Status"</span>
                        <span>"Route"</span>
                        <span class="text-right">"Detected"</span>
                        <span class="text-right">"P&L"</span>
                        <span class="text-right">"Time"</span>
                    </div>
                    <Show
                        when=move || !fills.get().is_empty()
                        fallback=move || {
                            view! {
                                <div class="px-3 py-5 text-center text-xs text-muted-foreground font-mono">
                                    "no cycle fills yet"
                                </div>
                            }
                        }
                    >
                        <div class="max-h-64 overflow-y-auto">
                            <For each=move || fills.get() key=|f| f.id let: fill>
                                {cycle_fill_row(fill).into_any()}
                            </For>
                        </div>
                    </Show>
                </div>
            </CardContent>
        </Card>
    }
}
