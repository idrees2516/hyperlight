//! Genetic-algorithm optimizer panel.
//!
//! Shows the evolution progress (best fitness per generation as a sparkline),
//! the current best genome vs the engine's live parameters, the empirically
//! calibrated latency-survival table, and controls (enable / apply / reset).

use crate::components::*;
use crate::model::fmt_num;
use crate::ws::Signals;
use leptos::prelude::*;
use ob_core::GaState;

/// Sparkline size.
const W: f64 = 520.0;
const H: f64 = 72.0;

/// Fitness history sparkline (SVG).
#[component]
fn FitnessSpark(state: Memo<GaState>) -> impl IntoView {
    let last = Memo::new(move |_| {
        state
            .get()
            .history
            .last()
            .map(|p| p.1)
            .unwrap_or(f64::NAN)
    });
    view! {
        <Show
            when=move || !state.get().history.is_empty()
            fallback=move || {
                view! {
                    <div class="h-[72px] flex items-center justify-center text-[11px] text-muted-foreground font-mono">
                        "waiting for the first generation (evolves every 30 s)"
                    </div>
                }
            }
        >
            {move || {
                let s = state.get();
                let pts = &s.history;
                if pts.len() < 2 {
                    return view! { <span></span> }.into_any();
                }
                let n = pts.len() as f64;
                let min = pts.iter().map(|p| p.1).fold(f64::INFINITY, f64::min);
                let max = pts.iter().map(|p| p.1).fold(f64::NEG_INFINITY, f64::max);
                let span = (max - min).max(1e-9);
                let xf = |i: usize| (i as f64 / (n - 1.0)) * (W - 8.0) + 4.0;
                let yf = |v: f64| H - 6.0 - ((v - min) / span) * (H - 14.0);
                let mut d = format!("M {:.1} {:.1}", xf(0), yf(pts[0].1));
                for (i, pt) in pts.iter().enumerate().skip(1) {
                    d.push_str(&format!(" L {:.1} {:.1}", xf(i), yf(pt.1)));
                }
                let mut a = d.clone();
                a.push_str(&format!(" L {:.1} {:.1} L {:.1} {:.1} Z", W - 4.0, H - 4.0, 4.0, H - 4.0));
                let cx = format!("{:.1}", W - 4.0);
                let cy = format!("{:.1}", yf(pts[pts.len() - 1].1));
                view! {
                    <svg viewBox=format!("0 0 {W} {H}") class="w-full h-[72px]" preserveAspectRatio="none">
                        <path d=a class="fill-emerald-500/10 stroke-none" />
                        <path
                            d=d
                            class="stroke-emerald-400"
                            fill="none"
                            stroke-width="1.5"
                            vector-effect="non-scaling-stroke"
                        />
                        <circle cx=cx cy=cy r="2.5" class="fill-emerald-400" />
                    </svg>
                }.into_any()
            }}
            <div class="flex justify-between text-[10px] font-mono text-muted-foreground">
                <span>"gen 1"</span>
                <span class="text-emerald-300">{move || format!("best fitness {:.2}", last.get())}</span>
                <span>{move || format!("gen {}", state.get().generation)}</span>
            </div>
        </Show>
    }
}

/// One genome parameter row: label, GA best value, live engine value.
#[component]
fn GenomeRow(label: &'static str, best: String, live: String) -> impl IntoView {
    let same = best == live;
    let live_class = if same { "text-right text-zinc-400" } else { "text-right text-amber-300/90" };
    view! {
        <div class="grid grid-cols-[minmax(0,1fr)_110px_110px] items-center gap-2 px-3 py-1.5 text-xs font-mono tabular-nums border-b border-border/40">
            <span class="text-muted-foreground">{label}</span>
            <span class="text-right text-emerald-300 font-medium">{best}</span>
            <span class=live_class>{live}</span>
        </div>
    }
}

#[component]
pub fn GaCard(sig: Signals) -> impl IntoView {
    let state: Memo<GaState> = Memo::new(move |_| sig.ga.get().state.clone());
    let live = Memo::new(move |_| sig.arb.get().config.clone());

    let enabled = Memo::new(move |_| state.get().enabled);
    let generation = Memo::new(move |_| state.get().generation);
    let best_fit = Memo::new(move |_| state.get().best_fitness.clone());
    let cur_fit = Memo::new(move |_| state.get().current_fitness.clone());
    let applied_gen = Memo::new(move |_| state.get().applied_gen);
    let obs = Memo::new(move |_| state.get().observations);

    // GA best genome fields.
    let g_fire = Memo::new(move |_| state.get().best.fire_edge_bps.clone());
    let g_min = Memo::new(move |_| state.get().best.min_edge_bps.clone());
    let g_lat = Memo::new(move |_| state.get().best.latency_ms.to_string());
    let g_cool = Memo::new(move |_| state.get().best.cooldown_ms.to_string());
    let g_not = Memo::new(move |_| state.get().best.max_notional_usd.clone());
    let g_eth = Memo::new(move |_| state.get().best.eth_w.clone());
    let g_sol = Memo::new(move |_| state.get().best.sol_w.clone());

    // Live engine values for comparison.
    let l_fire = Memo::new(move |_| live.get().fire_edge_bps.clone());
    let l_min = Memo::new(move |_| live.get().min_edge_bps.clone());
    let l_lat = Memo::new(move |_| live.get().latency_ms.to_string());
    let l_cool = Memo::new(move |_| live.get().cooldown_ms.to_string());
    let l_not = Memo::new(move |_| live.get().max_notional_usd.clone());

    let survival = Memo::new(move |_| state.get().survival.clone());

    view! {
        <Card>
            <CardHeader class="pb-3">
                <div class="flex items-center justify-between gap-3 flex-wrap">
                    <div class="flex flex-col gap-0.5">
                        <CardTitle class="font-mono text-base">
                            "Genetic Optimizer"
                            <span class="text-muted-foreground font-normal">" — evolving engine parameters online"</span>
                        </CardTitle>
                        <CardDescription>
                            "steady-state GA (pop 48 · tournament k=3 · BLX-\u{3b1} crossover · adaptive Gaussian mutation · elitism + immigrants) · fitness = replayed paper P&L on the live opportunity stream with empirically calibrated latency-survival rates · best genome hot-applied every 5 generations"
                        </CardDescription>
                    </div>
                    <div class="flex items-center gap-2">
                        {move || {
                            let on = enabled.get();
                            let (v, label) = if on {
                                (BadgeVariant::Success, "evolving".to_string())
                            } else {
                                (BadgeVariant::Secondary, "paused".to_string())
                            };
                            view! {
                                <Badge variant=v class="uppercase">
                                    {move || if enabled.get() { view! { <PulseDot /> }.into_any() } else { view! { <span></span> }.into_any() }}
                                    {label}
                                </Badge>
                            }.into_any()
                        }}
                        <Button
                            variant=ButtonVariant::Outline
                            class="h-7 text-xs"
                            on_click=move || sig.ga_toggle(!enabled.get_untracked())
                        >
                            {move || if enabled.get() { "Pause" } else { "Resume" }}
                        </Button>
                        <Button
                            variant=ButtonVariant::Outline
                            class="h-7 text-xs"
                            on_click=move || sig.ga_apply()
                        >
                            "Apply best"
                        </Button>
                        <Button
                            variant=ButtonVariant::Ghost
                            class="h-7 text-xs"
                            on_click=move || sig.ga_reset()
                        >
                            "Reset"
                        </Button>
                    </div>
                </div>
            </CardHeader>
            <CardContent class="flex flex-col gap-4">
                <div class="grid grid-cols-1 lg:grid-cols-[minmax(0,3fr)_minmax(0,2fr)] gap-4">
                    // Fitness evolution.
                    <div class="rounded-lg border border-border p-3 flex flex-col gap-2">
                        <div class="flex items-center justify-between">
                            <span class="text-[11px] font-medium uppercase tracking-wider text-muted-foreground">
                                "Best fitness per generation (simulated net P&L)"
                            </span>
                            <span class="text-[10px] font-mono text-muted-foreground">
                                {move || format!("{} obs replayed · applied gen {}", obs.get(), applied_gen.get())}
                            </span>
                        </div>
                        <FitnessSpark state />
                    </div>

                    // Headline tiles.
                    <div class="grid grid-cols-2 gap-3">
                        <StatTile
                            label="Generation"
                            value=Memo::new(move |_| generation.get().to_string())
                            value_class=Memo::new(|_| "text-foreground".to_string())
                            sub=Memo::new(move |_| "pop 48 · 30 s / gen".to_string())
                        >
                            {move || view! { <span></span> }}
                        </StatTile>
                        <StatTile
                            label="Best vs Current"
                            value=Memo::new(move |_| format!("{} / {}", best_fit.get(), cur_fit.get()))
                            value_class=Memo::new(|_| "text-emerald-300".to_string())
                            sub=Memo::new(|_| "fitness (USD)".to_string())
                        >
                            {move || view! { <span></span> }}
                        </StatTile>
                    </div>
                </div>

                <div class="grid grid-cols-1 lg:grid-cols-[minmax(0,3fr)_minmax(0,2fr)] gap-4">
                    // Genome comparison.
                    <div class="rounded-lg border border-border overflow-hidden">
                        <div class="grid grid-cols-[minmax(0,1fr)_110px_110px] items-center gap-2 px-3 h-8 text-[10px] uppercase tracking-wider text-muted-foreground bg-muted/40 border-b border-border">
                            <span>"Parameter"</span>
                            <span class="text-right text-emerald-400/80">"GA best"</span>
                            <span class="text-right">"Live engine"</span>
                        </div>
                        <GenomeRow label="fire edge (bps)" best=g_fire.get() live=l_fire.get() />
                        <GenomeRow label="min edge (bps)" best=g_min.get() live=l_min.get() />
                        <GenomeRow label="latency (ms)" best=g_lat.get() live=l_lat.get() />
                        <GenomeRow label="cooldown (ms)" best=g_cool.get() live=l_cool.get() />
                        <GenomeRow label="max notional ($)" best=g_not.get() live=l_not.get() />
                        <GenomeRow label="ETH weight" best=g_eth.get() live="\u{2014}".to_string() />
                        <GenomeRow label="SOL weight" best=g_sol.get() live="\u{2014}".to_string() />
                    </div>

                    // Latency-survival calibration table.
                    <div class="rounded-lg border border-border overflow-hidden">
                        <div class="px-3 h-8 flex items-center text-[10px] uppercase tracking-wider text-muted-foreground bg-muted/40 border-b border-border">
                            "Latency-survival by detected edge (live-calibrated)"
                        </div>
                        <For each=move || survival.get() key=|b| b.0.clone() let: pair>
                            {let bucket = pair.0.clone();
                            let rate = pair.1.clone();
                            view! {
                                <div class="grid grid-cols-[minmax(0,1fr)_80px] items-center gap-2 px-3 py-1.5 text-xs font-mono tabular-nums border-b border-border/40">
                                    <span class="text-muted-foreground">{format!("{} bps", bucket)}</span>
                                    <span class="text-right text-zinc-200">{rate}</span>
                                </div>
                            }.into_any()}
                        </For>
                        <div class="px-3 py-2 text-[10px] text-muted-foreground font-mono">
                            "measured from the engine's own fill / expired outcomes"
                        </div>
                    </div>
                </div>
            </CardContent>
        </Card>
    }
}
