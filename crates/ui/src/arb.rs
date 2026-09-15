//! Cross-venue arbitrage engine panel: live opportunities, paper-execution
//! fills log, equity curve, engine configuration and per-venue fee editor.
//!
//! Pure SVG equity chart (no canvas, no chart lib) rendered from the
//! `EquityPt` series pushed by the backend at 2 Hz plus every fill.

use crate::components::*;
use crate::model::fmt_hms;
use crate::ws::Signals;
use leptos::prelude::*;
use ob_core::{ArbConfigUpdate, ArbFill, ArbOpportunity, Venue, VENUES};

const EQ_W: f64 = 560.0;
const EQ_H: f64 = 132.0;

fn abbrev_usd(v: f64) -> String {
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

fn venue_chip(v: Venue) -> &'static str {
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

#[component]
fn VenueChip(v: Venue) -> impl IntoView {
    let short = v.short();
    let cls = venue_chip(v);
    view! {
        <span class=format!("rounded border px-1.5 py-px text-[10px] leading-none font-sans {cls}")>
            {short}
        </span>
    }
}

#[component]
pub fn ArbCard(sig: Signals) -> impl IntoView {
    // Engine state.
    let opps: Memo<Vec<ArbOpportunity>> =
        Memo::new(move |_| sig.arb.get().opportunities.clone());
    let stats_lite: Memo<(String, u64, u64, u64, u64, String, String, usize)> = Memo::new(
        move |_| {
            let a = sig.arb.get();
            let s = &a.stats;
            (
                s.pnl_usd.clone(),
                s.fills,
                s.expired,
                s.wins,
                s.losses,
                s.best_net_bps.clone(),
                s.avg_net_bps.clone(),
                s.open_ops,
            )
        },
    );
    let fills: Memo<Vec<ArbFill>> = Memo::new(move |_| {
        let mut f = sig.arb.get().fills.clone();
        f.reverse(); // newest first
        f
    });
    let enabled: Memo<bool> = Memo::new(move |_| sig.arb.get().config.enabled);

    // Local editable config (initialized/refreshed from server config).
    let cfg_min_edge = RwSignal::new(String::new());
    let cfg_fire_edge = RwSignal::new(String::new());
    let cfg_max_notional = RwSignal::new(String::new());
    let cfg_latency = RwSignal::new(String::new());
    let cfg_cooldown = RwSignal::new(String::new());
    let cfg_fees: RwSignal<Vec<(Venue, String)>> = RwSignal::new(Vec::new());
    let cfg_loaded = RwSignal::new(false);
    Effect::new(move |_| {
        if !cfg_loaded.get() {
            let c = sig.arb.get().config.clone();
            cfg_min_edge.set(c.min_edge_bps.clone());
            cfg_fire_edge.set(c.fire_edge_bps.clone());
            cfg_max_notional.set(c.max_notional_usd.clone());
            cfg_latency.set(c.latency_ms.to_string());
            cfg_cooldown.set(c.cooldown_ms.to_string());
            cfg_fees.set(c.fees_bps.clone());
            cfg_loaded.set(true);
        }
    });

    let pnl_class = Memo::new(move |_| {
        let (pnl, ..) = stats_lite.get();
        match pnl.parse::<f64>() {
            Ok(v) if v > 0.0 => "text-emerald-300".to_string(),
            Ok(v) if v < 0.0 => "text-rose-300".to_string(),
            _ => "text-foreground".to_string(),
        }
    });
    let pnl_value = Memo::new(move |_| {
        let (pnl, ..) = stats_lite.get();
        format!("${}", abbrev_usd(pnl.parse::<f64>().unwrap_or(0.0)))
    });
    let fills_value = Memo::new(move |_| {
        let (_, f, e, w, l, ..) = stats_lite.get();
        format!("{f} / {e} exp")
    });
    let win_value = Memo::new(move |_| {
        let (_, _, _, w, l, ..) = stats_lite.get();
        format!("{w}W\u{2013}{l}L")
    });
    let best_value = Memo::new(move |_| {
        let t = stats_lite.get();
        format!("{} bps", t.5)
    });
    let avg_value = Memo::new(move |_| {
        let a = sig.arb.get();
        format!("{} bps", a.stats.avg_net_bps.clone())
    });
    let open_value = Memo::new(move |_| {
        let t = stats_lite.get();
        format!("{}", t.7)
    });

    // Equity curve path from the series.
    let equity: Memo<Vec<ob_core::EquityPt>> = Memo::new(move |_| sig.arb.get().equity.clone());
    let eq_path: Memo<(String, String, f64, f64)> = Memo::new(move |_| {
        let pts = equity.get();
        if pts.len() < 2 {
            return (String::new(), String::new(), 0.0, 0.0);
        }
        let t0 = pts.first().map(|p| p.t).unwrap_or(0) as f64;
        let t1 = pts.last().map(|p| p.t).unwrap_or(1) as f64;
        let span = (t1 - t0).max(1.0);
        let mut min_eq = f64::INFINITY;
        let mut max_eq = f64::NEG_INFINITY;
        for p in pts.iter() {
            min_eq = min_eq.min(p.eq);
            max_eq = max_eq.max(p.eq);
        }
        // Always include zero in the y range for context.
        min_eq = min_eq.min(0.0);
        max_eq = max_eq.max(0.0);
        let range = (max_eq - min_eq).max(1e-9);
        let x = |p: &ob_core::EquityPt| (p.t as f64 - t0) / span * EQ_W;
        let y = |p: &ob_core::EquityPt| EQ_H - 4.0 - (p.eq - min_eq) / range * (EQ_H - 12.0);
        let mut line = String::with_capacity(pts.len() * 16);
        let mut area = String::with_capacity(pts.len() * 16 + 32);
        for (i, p) in pts.iter().enumerate() {
            let s = format!(
                "{}{:.1} {:.1}",
                if i == 0 { "M" } else { " L" },
                x(p),
                y(p)
            );
            line.push_str(&s);
            area.push_str(&s);
        }
        area.push_str(&format!(" L{EQ_W:.1} {EQ_H:.1} L0.0 {EQ_H:.1} Z"));
        (line, area, min_eq, max_eq)
    });

    let toggle = move || {
        let next = !sig.arb.get().config.enabled;
        sig.arb_config(ArbConfigUpdate {
            enabled: Some(next),
            ..Default::default()
        });
    };
    let reset = move || sig.arb_reset();
    let apply = move || {
        let fees = cfg_fees.get();
        sig.arb_config(ArbConfigUpdate {
            min_edge_bps: Some(cfg_min_edge.get()),
            fire_edge_bps: Some(cfg_fire_edge.get()),
            max_notional_usd: Some(cfg_max_notional.get()),
            latency_ms: cfg_latency.get().parse().ok(),
            cooldown_ms: cfg_cooldown.get().parse().ok(),
            fees_bps: Some(fees),
            enabled: None,
        });
    };

    view! {
        <Card>
            <CardHeader class="pb-3">
                <div class="flex items-start justify-between gap-3 flex-wrap">
                    <div class="flex flex-col gap-0.5">
                        <div class="flex items-center gap-2.5">
                            <CardTitle class="font-mono text-base">"Cross-Venue Arbitrage Engine"</CardTitle>
                            {move || {
                                if enabled.get() {
                                    view! {
                                        <Badge variant=BadgeVariant::Success class="uppercase">
                                            <PulseDot />
                                            "armed"
                                        </Badge>
                                    }.into_any()
                                } else {
                                    view! {
                                        <Badge variant=BadgeVariant::Warning class="uppercase">"paused"</Badge>
                                    }.into_any()
                                }
                            }}
                        </div>
                        <CardDescription>
                            "ETH + SOL across 9 CLOBs · fee-aware depth-walking · latency-modeled paper execution · live USDT/USD normalization"
                        </CardDescription>
                    </div>
                    <div class="flex items-center gap-2">
                        {move || {
                            if enabled.get() {
                                view! {
                                    <Button variant=ButtonVariant::Secondary class="h-8 px-3" on_click=toggle>
                                        "Pause engine"
                                    </Button>
                                }.into_any()
                            } else {
                                view! {
                                    <Button variant=ButtonVariant::Default class="h-8 px-3" on_click=toggle>
                                        "Arm engine"
                                    </Button>
                                }.into_any()
                            }
                        }}
                        <Button variant=ButtonVariant::Outline class="h-8 px-3" on_click=reset>
                            "Reset P&L"
                        </Button>
                    </div>
                </div>

                // Stat tiles.
                <div class="grid grid-cols-3 md:grid-cols-6 gap-3 pt-1">
                    <ArbTile label="Net P&L" value=pnl_value value_class=pnl_class sub="paper" />
                    <ArbTile label="Fills" value=fills_value sub="filled / expired" />
                    <ArbTile label="Record" value=win_value sub="win\u{2013}loss" />
                    <ArbTile label="Best Edge" value=best_value sub="net bps" />
                    <ArbTile label="Avg Edge" value=avg_value sub="per fill" />
                    <ArbTile label="Open Opps" value=open_value sub="live routes" />
                </div>
            </CardHeader>
            <CardContent class="flex flex-col gap-5">

                // Opportunities + equity curve.
                <div class="grid grid-cols-1 lg:grid-cols-5 gap-4 items-stretch">
                    <div class="lg:col-span-3 min-w-0 flex flex-col">
                        <div class="flex items-center justify-between px-1 h-7">
                            <span class="text-xs font-medium uppercase tracking-wider text-muted-foreground">
                                "Live Opportunities"
                            </span>
                            <span class="text-[10px] font-mono text-muted-foreground">
                                "net of taker fees · executable depth"
                            </span>
                        </div>
                        <div class="rounded-lg border border-border overflow-hidden">
                            <div class="grid grid-cols-[minmax(0,0.8fr)_minmax(0,1.5fr)_minmax(0,0.7fr)_minmax(0,0.8fr)_minmax(0,0.6fr)] items-center gap-2 px-3 h-8 text-[10px] uppercase tracking-wider text-muted-foreground bg-muted/40 border-b border-border font-medium">
                                <span>"Market"</span>
                                <span>"Route (buy \u{2192} sell)"</span>
                                <span class="text-right">"Notional"</span>
                                <span class="text-right">"Net bps"</span>
                                <span class="text-right">"Profit"</span>
                            </div>
                            <Show
                                when=move || !opps.get().is_empty()
                                fallback=move || view! {
                                    <div class="px-3 py-6 text-center text-[12px] font-mono text-muted-foreground">
                                        "scanning 9 venues\u{2026} no route above the threshold right now"
                                    </div>
                                }
                            >
                                <div class="max-h-[264px] overflow-y-auto">
                                    <For each=move || opps.get() key=|o| {
                                        format!("{}-{}-{}-{}", o.market.label(), o.buy_venue.label(), o.sell_venue.label(), o.ts)
                                    } let: o>
                                        {let net: f64 = o.net_bps.parse().unwrap_or(0.0);
                                        let notional_disp = abbrev_usd(o.notional.parse().unwrap_or(0.0));
                                        let profit_disp = abbrev_usd(o.profit_usd.parse().unwrap_or(0.0));
                                        let (buy_px, sell_px, size, net_disp) = (
                                            o.buy_touch.clone(),
                                            o.sell_touch.clone(),
                                            o.size.clone(),
                                            o.net_bps.clone(),
                                        );
                                        view! {
                                            <div class="grid grid-cols-[minmax(0,0.8fr)_minmax(0,1.5fr)_minmax(0,0.7fr)_minmax(0,0.8fr)_minmax(0,0.6fr)] items-center gap-2 px-3 h-[34px] text-[12px] font-mono tabular-nums hover:bg-muted/50 border-b border-border/40 last:border-0">
                                                <span class="font-sans font-medium text-[12px]">{o.market.short().to_string()}</span>
                                                <span class="flex items-center gap-1.5 min-w-0">
                                                    <VenueChip v=o.buy_venue />
                                                    <span class="text-emerald-300 truncate">{buy_px}</span>
                                                    <span class="text-muted-foreground">"\u{2192}"</span>
                                                    <VenueChip v=o.sell_venue />
                                                    <span class="text-rose-300 truncate">{sell_px}</span>
                                                </span>
                                                <span class="text-right text-zinc-300">
                                                    {move || format!("${}", notional_disp.clone())}
                                                </span>
                                                <span class=move || {
                                                    if net >= 0.0 { "text-right text-emerald-300 font-medium".to_string() } else { "text-right text-rose-300 font-medium".to_string() }
                                                }>
                                                    {move || net_disp.clone()}
                                                </span>
                                                <span class="text-right text-emerald-300/90">
                                                    {move || format!("${}", profit_disp.clone())}
                                                </span>
                                            </div>
                                        }}
                                    </For>
                                </div>
                            </Show>
                        </div>
                    </div>

                    // Equity curve.
                    <div class="lg:col-span-2 min-w-0 flex flex-col">
                        <div class="flex items-center justify-between px-1 h-7">
                            <span class="text-xs font-medium uppercase tracking-wider text-muted-foreground">
                                "Paper Equity (USD)"
                            </span>
                            {move || {
                                let (_, _, min_eq, max_eq) = eq_path.get();
                                let (lo, hi) = if min_eq == 0.0 && max_eq == 0.0 {
                                    (0.0, 0.0)
                                } else {
                                    (min_eq, max_eq)
                                };
                                view! {
                                    <span class="text-[10px] font-mono text-muted-foreground">
                                        {format!("${:.2} \u{2013} ${:.2}", lo, hi)}
                                    </span>
                                }.into_any()
                            }}
                        </div>
                        <div class="rounded-lg border border-border bg-card p-2 flex-1 flex items-center min-h-[132px]">
                            <svg viewBox=format!("0 0 {EQ_W} {EQ_H}") class="w-full h-[132px]" preserveAspectRatio="none">
                                <Show when=move || !eq_path.get().0.is_empty()>
                                    {move || {
                                        let (line, area, _, _) = eq_path.get();
                                        view! {
                                            <path d=area.clone() class="fill-emerald-500/30"></path>
                                            <path d=line.clone() class="stroke-emerald-400" stroke-width="1.5" fill="none"></path>
                                        }.into_any()
                                    }}
                                </Show>
                            </svg>
                        </div>
                        <div class="mt-1.5 text-[10px] font-mono text-muted-foreground text-center">
                            "cumulative net P&L \u{00b7} last 30 min"
                        </div>
                    </div>
                </div>

                // Fills log + config.
                <div class="grid grid-cols-1 lg:grid-cols-5 gap-4 items-start">
                    <div class="lg:col-span-3 min-w-0">
                        <div class="flex items-center justify-between px-1 h-7">
                            <span class="text-xs font-medium uppercase tracking-wider text-muted-foreground">
                                "Execution Log"
                            </span>
                            <span class="text-[10px] font-mono text-muted-foreground">
                                "paper fills + latency expirations"
                            </span>
                        </div>
                        <div class="rounded-lg border border-border overflow-hidden">
                            <div class="grid grid-cols-[auto_minmax(0,1.6fr)_minmax(0,0.8fr)_minmax(0,0.7fr)_minmax(0,0.6fr)_minmax(0,0.7fr)] items-center gap-2 px-3 h-8 text-[10px] uppercase tracking-wider text-muted-foreground bg-muted/40 border-b border-border font-medium">
                                <span>"Time"</span>
                                <span>"Route"</span>
                                <span class="text-right">"Size"</span>
                                <span class="text-right">"Net bps"</span>
                                <span class="text-right">"P&L"</span>
                                <span class="text-right">"Status"</span>
                            </div>
                            <Show
                                when=move || !fills.get().is_empty()
                                fallback=move || view! {
                                    <div class="px-3 py-6 text-center text-[12px] font-mono text-muted-foreground">
                                        "waiting for the first paper fill\u{2026}"
                                    </div>
                                }
                            >
                                <div class="max-h-[220px] overflow-y-auto">
                                    <For each=move || fills.get() key=|f| f.id let: f>
                                        {let expired = f.status != "filled";
                                        let (time, size_disp, net_disp, pnl_disp, pnl_num) = (
                                            fmt_hms(f.ts),
                                            f.size.clone(),
                                            f.net_bps.clone(),
                                            f.pnl_usd.clone(),
                                            f.pnl_usd.parse::<f64>().unwrap_or(0.0),
                                        );
                                        view! {
                                            <div class=move || format!(
                                                "grid grid-cols-[auto_minmax(0,1.6fr)_minmax(0,0.8fr)_minmax(0,0.7fr)_minmax(0,0.6fr)_minmax(0,0.7fr)] items-center gap-2 px-3 h-[30px] text-[12px] font-mono tabular-nums border-b border-border/40 last:border-0 {}",
                                                if expired { "opacity-60" } else { "hover:bg-muted/50" },
                                            )>
                                                <span class="text-muted-foreground">{time}</span>
                                                <span class="flex items-center gap-1.5 min-w-0">
                                                    <VenueChip v=f.buy_venue />
                                                    <span class="text-emerald-300 truncate">{f.buy_px.clone()}</span>
                                                    <span class="text-muted-foreground">"\u{2192}"</span>
                                                    <VenueChip v=f.sell_venue />
                                                    <span class="text-rose-300 truncate">{f.sell_px.clone()}</span>
                                                </span>
                                                <span class="text-right text-zinc-300 truncate">{move || size_disp.clone()}</span>
                                                <span class="text-right text-zinc-300">{move || net_disp.clone()}</span>
                                                <span class=move || {
                                                    if pnl_num >= 0.0 { "text-right text-emerald-300".to_string() } else { "text-right text-rose-300".to_string() }
                                                }>
                                                    {move || format!("${}", pnl_disp.clone())}
                                                </span>
                                                <span class="text-right">
                                                    {move || {
                                                        if expired {
                                                            view! {
                                                                <span class="rounded border border-zinc-700/60 bg-zinc-800/60 px-1.5 py-px text-[9px] font-sans text-zinc-300">
                                                                    "expired"
                                                                </span>
                                                            }.into_any()
                                                        } else {
                                                            view! {
                                                                <span class="rounded border border-emerald-900/60 bg-emerald-950/50 px-1.5 py-px text-[9px] font-sans text-emerald-300">
                                                                    "filled"
                                                                </span>
                                                            }.into_any()
                                                        }
                                                    }}
                                                </span>
                                            </div>
                                        }}
                                    </For>
                                </div>
                            </Show>
                        </div>
                    </div>

                    // Config panel.
                    <div class="lg:col-span-2 min-w-0">
                        <div class="flex items-center justify-between px-1 h-7">
                            <span class="text-xs font-medium uppercase tracking-wider text-muted-foreground">
                                "Engine Configuration"
                            </span>
                            <span class="text-[10px] font-mono text-muted-foreground">"applies live"</span>
                        </div>
                        <div class="rounded-lg border border-border p-3 flex flex-col gap-3">
                            <div class="grid grid-cols-2 gap-2.5">
                                <div class="flex flex-col gap-1">
                                    <span class="text-[10px] uppercase tracking-wider text-muted-foreground">"Min edge (bps)"</span>
                                    <Input value=cfg_min_edge placeholder="3" />
                                </div>
                                <div class="flex flex-col gap-1">
                                    <span class="text-[10px] uppercase tracking-wider text-muted-foreground">"Fire edge (bps)"</span>
                                    <Input value=cfg_fire_edge placeholder="8" />
                                </div>
                                <div class="flex flex-col gap-1">
                                    <span class="text-[10px] uppercase tracking-wider text-muted-foreground">"Max notional ($)"</span>
                                    <Input value=cfg_max_notional placeholder="10000" />
                                </div>
                                <div class="flex flex-col gap-1">
                                    <span class="text-[10px] uppercase tracking-wider text-muted-foreground">"Latency (ms)"</span>
                                    <Input value=cfg_latency placeholder="250" />
                                </div>
                                <div class="flex flex-col gap-1">
                                    <span class="text-[10px] uppercase tracking-wider text-muted-foreground">"Cooldown (ms)"</span>
                                    <Input value=cfg_cooldown placeholder="3000" />
                                </div>
                            </div>
                            <Separator />
                            <div class="flex flex-col gap-1.5">
                                <span class="text-[10px] uppercase tracking-wider text-muted-foreground">
                                    "Taker fees (bps)"
                                </span>
                                <div class="grid grid-cols-3 gap-1.5">
                                    <For each=move || cfg_fees.get() key=|(v, _)| v.index() let: fee>
                                        {let idx = fee.0.index();
                                        view! {
                                            <div class="flex items-center gap-1.5">
                                                <span class="w-8 shrink-0 text-[10px] font-mono text-muted-foreground">
                                                    {fee.0.short()}
                                                </span>
                                                <FeeInput fees=cfg_fees idx=idx />
                                            </div>
                                        }}
                                    </For>
                                </div>
                            </div>
                            <Button variant=ButtonVariant::Default class="w-full h-9" on_click=apply>
                                "Apply configuration"
                            </Button>
                        </div>
                    </div>
                </div>
            </CardContent>
        </Card>
    }
}

/// Compact stat tile for the arbitrage header.
#[component]
fn ArbTile(
    label: &'static str,
    value: Memo<String>,
    #[prop(optional)] value_class: Option<Memo<String>>,
    sub: &'static str,
) -> impl IntoView {
    view! {
        <div class="rounded-lg border border-border bg-card px-3 py-2 flex flex-col justify-between">
            <span class="text-[10px] font-medium uppercase tracking-wider text-muted-foreground truncate">
                {label}
            </span>
            <span class=move || {
                format!(
                    "font-mono tabular-nums text-[15px] font-semibold leading-tight truncate {}",
                    value_class.as_ref().map(|m| m.get()).unwrap_or_default(),
                )
            }>
                {move || value.get()}
            </span>
            <span class="text-[10px] font-mono text-muted-foreground truncate">{sub}</span>
        </div>
    }
}

/// One-venue fee input bound into the cfg_fees signal.
#[component]
fn FeeInput(fees: RwSignal<Vec<(Venue, String)>>, idx: usize) -> impl IntoView {
    let val = RwSignal::new(String::new());
    // Two-way: signal -> input (on init) and input -> signal (on change).
    Effect::new(move |_| {
        let f = fees.get();
        if let Some((_, bps)) = f.get(idx) {
            if val.get_untracked().is_empty() {
                val.set(bps.clone());
            }
        }
    });
    Effect::new(move |_| {
        let v = val.get();
        if v.is_empty() {
            return;
        }
        fees.update(|f| {
            if let Some(e) = f.get_mut(idx) {
                e.1 = v.clone();
            }
        });
    });
    view! {
        <input
            type="text"
            inputmode="decimal"
            prop:value=move || val.get()
            on:input=move |ev| val.set(event_target_value(&ev))
            class="flex h-7 w-full min-w-0 rounded-md border border-input bg-background px-2 py-0.5 text-[12px] font-mono tabular-nums shadow-sm transition-colors placeholder:text-muted-foreground/60 focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring"
        />
    }
}

/// Silence unused VENUES import when needed.
#[allow(dead_code)]
fn _all_venues() -> &'static [Venue] {
    &VENUES
}
