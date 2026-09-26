//! Global sweep optimizer panel.
//!
//! Renders the exact profit-maximizing multi-venue execution plans produced
//! by `ob_core::sweep`: per-market plans showing the full venue split
//! (buy legs on the cheap venues, sell legs on the rich venues), the
//! survival-weighted expected value, and the paper execution log.

use crate::components::*;
use crate::model::fmt_num;
use crate::ws::Signals;
use leptos::prelude::*;
use ob_core::{Market, SweepFill, SweepLeg, SweepPlan, Venue};

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

/// Compact notional for leg chips: `$4.2k` / `$980`.
fn fmt_k(usd: &str) -> String {
    let v: f64 = usd.parse().unwrap_or(0.0);
    if v >= 1000.0 {
        format!("${:.1}k", v / 1000.0)
    } else {
        format!("${:.0}", v)
    }
}

/// One leg chip: `buy BN $4.2k` with side color + venue accent.
fn leg_chip(l: &SweepLeg) -> impl IntoView {
    let class = venue_chip_class(l.venue);
    let side_class = if l.side == "buy" {
        "text-emerald-300"
    } else {
        "text-rose-300"
    };
    let label = format!("{} {}", l.side, l.venue.short());
    let notional = fmt_k(&l.notional);
    view! {
        <span class=format!("rounded border px-1.5 py-px text-[10px] leading-tight font-mono font-medium {class}")>
            <span class=side_class>{label}</span>
            {notional}
        </span>
    }
}

/// The full split: buy legs, arrow, sell legs.
fn split_chain(legs: &[SweepLeg]) -> impl IntoView {
    let buys: Vec<&SweepLeg> = legs.iter().filter(|l| l.side == "buy").collect();
    let sells: Vec<&SweepLeg> = legs.iter().filter(|l| l.side == "sell").collect();
    view! {
        <span class="flex items-center gap-1 flex-wrap">
            {buys.iter().map(|l| leg_chip(l).into_any()).collect::<Vec<_>>()}
            <span class="text-muted-foreground text-[10px]">"\u{2192}"</span>
            {sells.iter().map(|l| leg_chip(l).into_any()).collect::<Vec<_>>()}
        </span>
    }
}

/// One rendered plan row.
fn plan_row(p: SweepPlan) -> impl IntoView {
    let net_bps: f64 = p.net_bps.parse().unwrap_or(0.0);
    let edge_class = if net_bps >= 8.0 {
        "text-emerald-300 font-semibold"
    } else if net_bps >= 4.0 {
        "text-emerald-400"
    } else {
        "text-zinc-300"
    };
    let profit: f64 = p.profit_usd.parse().unwrap_or(0.0);
    let ev: f64 = p.ev_usd.parse().unwrap_or(0.0);
    let notional = fmt_k(&p.notional);
    let market_label = market_short(p.market);
    let net = fmt_num(&p.net_bps);
    view! {
        <div class="grid grid-cols-[64px_minmax(0,1fr)_74px_74px_74px_70px] items-center gap-2 px-3 py-1.5 text-xs font-mono tabular-nums border-b border-border/40 hover:bg-muted/50">
            <span class="uppercase text-zinc-400">{market_label}</span>
            {split_chain(&p.legs)}
            <span class=move || format!("text-right {edge_class}")>{net}</span>
            <span class="text-right text-sky-300">{format!("${:.2}", ev)}</span>
            <span class="text-right text-emerald-300">{format!("${:.2}", profit)}</span>
            <span class="text-right text-muted-foreground">{notional}</span>
        </div>
    }
}

fn market_short(m: Market) -> &'static str {
    m.short()
}

fn fill_row(f: SweepFill) -> impl IntoView {
    let filled = f.status == "filled";
    let status_class = if filled { "text-emerald-300" } else { "text-amber-300/80" };
    let market_label = market_short(f.market);
    let detected = fmt_num(&f.detected_net_bps);
    let pnl_disp = format!("${:.2}", f.profit_usd.parse::<f64>().unwrap_or(0.0));
    let pnl_class = if filled { "text-right text-emerald-300" } else { "text-right text-zinc-500" };
    let time = crate::model::fmt_hms(f.ts);
    let legs_n = f.legs.len();
    view! {
        <div class="grid grid-cols-[64px_52px_minmax(0,1fr)_70px_70px_54px_66px] items-center gap-2 px-3 py-1 text-[11px] font-mono tabular-nums border-b border-border/40">
            <span class="uppercase text-zinc-400">{market_label}</span>
            <span class=status_class>{if filled { "fill" } else { "expired" }}</span>
            {split_chain(&f.legs)}
            <span class="text-right text-muted-foreground">{detected}</span>
            <span class=pnl_class>{pnl_disp}</span>
            <span class="text-right text-muted-foreground">{legs_n}</span>
            <span class="text-right text-muted-foreground">{time}</span>
        </div>
    }
}

#[component]
pub fn SweepCard(sig: Signals) -> impl IntoView {
    let stats = Memo::new(move |_| sig.sweep.get().stats.clone());
    let plans: Memo<Vec<SweepPlan>> = Memo::new(move |_| sig.sweep.get().plans.clone());
    let fills: Memo<Vec<SweepFill>> = Memo::new(move |_| {
        let mut f = sig.sweep.get().fills.clone();
        f.reverse(); // newest first
        f.truncate(FILL_ROWS);
        f
    });

    let open_plans = Memo::new(move |_| stats.get().open_plans.to_string());
    let pnl = Memo::new(move |_| stats.get().pnl_usd.clone());
    let fills_n = Memo::new(move |_| stats.get().fills);
    let expired_n = Memo::new(move |_| stats.get().expired);
    let best_bps = Memo::new(move |_| stats.get().best_net_bps.clone());
    let avg_venues = Memo::new(move |_| stats.get().avg_venues.clone());
    let avg_bps = Memo::new(move |_| stats.get().avg_net_bps.clone());

    view! {
        <Card>
            <CardHeader class="pb-3">
                <div class="flex items-center justify-between gap-3 flex-wrap">
                    <div class="flex flex-col gap-0.5">
                        <CardTitle class="font-mono text-base">
                            "Global Sweep Optimizer"
                            <span class="text-muted-foreground font-normal">" — provably optimal multi-venue execution"</span>
                        </CardTitle>
                        <CardDescription>
                            "Merges all 9 venue books into fee-adjusted executable curves and crosses them greedily — every positive marginal pair is captured, splitting buys and sells across as many venues as depth requires · EV-gated firing with empirically calibrated latency-survival probabilities"
                        </CardDescription>
                    </div>
                    <div class="flex flex-col items-end gap-1">
                        <Badge variant=BadgeVariant::Outline class="font-mono text-[10px]">
                            "greedy curve crossing — exact optimum"
                        </Badge>
                        <Badge variant=BadgeVariant::Secondary class="font-mono text-[10px]">
                            {move || format!("avg {:.1} venues / plan", avg_venues.get().parse::<f64>().unwrap_or(0.0))}
                        </Badge>
                    </div>
                </div>
            </CardHeader>
            <CardContent class="flex flex-col gap-4">
                <div class="grid grid-cols-2 md:grid-cols-5 gap-3">
                    <StatTile
                        label="Open Plans"
                        value=open_plans
                        value_class=Memo::new(|_| "text-foreground".to_string())
                        sub=Memo::new(|_| "live optimal sweeps".to_string())
                    >
                        {move || view! { <span></span> }}
                    </StatTile>
                    <StatTile
                        label="Sweep P&L (paper)"
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
                    <StatTile
                        label="Avg Net Edge"
                        value=avg_bps
                        value_class=Memo::new(|_| "text-foreground".to_string())
                        sub=Memo::new(move |_| "bps over filled plans".to_string())
                    >
                        {move || view! { <span></span> }}
                    </StatTile>
                    <StatTile
                        label="Avg Venues"
                        value=avg_venues
                        value_class=Memo::new(|_| "text-foreground".to_string())
                        sub=Memo::new(|_| "per filled plan".to_string())
                    >
                        {move || view! { <span></span> }}
                    </StatTile>
                </div>

                // Live plans table.
                <div class="rounded-lg border border-border overflow-hidden">
                    <div class="grid grid-cols-[64px_minmax(0,1fr)_74px_74px_74px_70px] items-center gap-2 px-3 h-8 text-[10px] uppercase tracking-wider text-muted-foreground bg-muted/40 border-b border-border">
                        <span>"Mkt"</span>
                        <span>"Optimal venue split (buy → sell)"</span>
                        <span class="text-right">"Net bps"</span>
                        <span class="text-right">"EV"</span>
                        <span class="text-right">"Profit"</span>
                        <span class="text-right">"Notional"</span>
                    </div>
                    <Show
                        when=move || !plans.get().is_empty()
                        fallback=move || {
                            view! {
                                <div class="px-3 py-6 text-center text-xs text-muted-foreground font-mono">
                                    "no profitable sweep at current books (scanning all 9 venues at 2 Hz)"
                                </div>
                            }
                        }
                    >
                        <For each=move || plans.get() key=|p| (p.market, p.ts) let: p>
                            {plan_row(p).into_any()}
                        </For>
                    </Show>
                </div>

                // Execution log.
                <div class="rounded-lg border border-border overflow-hidden">
                    <div class="grid grid-cols-[64px_52px_minmax(0,1fr)_70px_70px_54px_66px] items-center gap-2 px-3 h-7 text-[10px] uppercase tracking-wider text-muted-foreground bg-muted/40 border-b border-border">
                        <span>"Mkt"</span>
                        <span>"Status"</span>
                        <span>"Executed split"</span>
                        <span class="text-right">"Detected"</span>
                        <span class="text-right">"P&L"</span>
                        <span class="text-right">"Legs"</span>
                        <span class="text-right">"Time"</span>
                    </div>
                    <Show
                        when=move || !fills.get().is_empty()
                        fallback=move || {
                            view! {
                                <div class="px-3 py-5 text-center text-xs text-muted-foreground font-mono">
                                    "no sweep fills yet"
                                </div>
                            }
                        }
                    >
                        <div class="max-h-64 overflow-y-auto">
                            <For each=move || fills.get() key=|f| f.id let: f>
                                {fill_row(f).into_any()}
                            </For>
                        </div>
                    </Show>
                </div>
            </CardContent>
        </Card>
    }
}
