//! Historical depth chart: bid/ask resting notional within a distance band
//! over time, with a mid-price overlay on the right axis. Pure SVG rendered
//! from `DepthSample` history — no JS chart library, no canvas.

use crate::components::*;
use crate::ws::Signals;
use leptos::prelude::*;
use ob_core::DepthSample;

const W: f64 = 720.0;
const H: f64 = 240.0;
const PAD_T: f64 = 12.0;
const PAD_B: f64 = 22.0;
/// Right-side price axis width.
const PAD_R: f64 = 62.0;
/// Left-side notional labels are drawn inside the plot.
const PAD_L: f64 = 8.0;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Band {
    B1,
    B5,
    B10,
    B20,
}

impl Band {
    const ALL: [Band; 4] = [Band::B1, Band::B5, Band::B10, Band::B20];
    fn label(self) -> &'static str {
        match self {
            Band::B1 => "0.1%",
            Band::B5 => "0.5%",
            Band::B10 => "1%",
            Band::B20 => "2%",
        }
    }
    fn get(self, s: &DepthSample) -> (f64, f64) {
        match self {
            Band::B1 => (s.b1, s.a1),
            Band::B5 => (s.b5, s.a5),
            Band::B10 => (s.b10, s.a10),
            Band::B20 => (s.b20, s.a20),
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Win {
    M5,
    M15,
    M30,
    M60,
}

impl Win {
    const ALL: [Win; 4] = [Win::M5, Win::M15, Win::M30, Win::M60];
    fn label(self) -> &'static str {
        match self {
            Win::M5 => "5m",
            Win::M15 => "15m",
            Win::M30 => "30m",
            Win::M60 => "60m",
        }
    }
    fn ms(self) -> u64 {
        match self {
            Win::M5 => 5 * 60_000,
            Win::M15 => 15 * 60_000,
            Win::M30 => 30 * 60_000,
            Win::M60 => 60 * 60_000,
        }
    }
}

fn abbrev_usd(v: f64) -> String {
    if v.abs() >= 1_000_000.0 {
        format!("${:.2}M", v / 1_000_000.0)
    } else if v.abs() >= 1_000.0 {
        format!("${:.0}K", v / 1_000.0)
    } else {
        format!("${v:.0}")
    }
}

fn fmt_hm(ms: u64) -> String {
    let d = js_sys::Date::new(&wasm_bindgen::JsValue::from_f64(ms as f64));
    format!("{:02}:{:02}", d.get_hours(), d.get_minutes())
}

fn now_ms() -> u64 {
    js_sys::Date::now().max(0.0) as u64
}

#[component]
pub fn DepthHistoryCard(sig: Signals) -> impl IntoView {
    let market = sig.selected;
    let band = RwSignal::new(Band::B10);
    let win = RwSignal::new(Win::M15);

    // Samples for the selected market + window.
    let samples: Memo<Vec<DepthSample>> = Memo::new(move |_| {
        let m = market.get();
        let cutoff = now_ms().saturating_sub(win.get().ms());
        sig.histories
            .get()
            .get(&m)
            .map(|v| v.iter().filter(|s| s.t >= cutoff).copied().collect())
            .unwrap_or_default()
    });

    // Latest band values + mid delta over the window (footer stats).
    let footer = Memo::new(move |_| {
        let s = samples.get();
        let b = band.get();
        let (last_bid, last_ask, last_mid) = match s.last() {
            Some(l) => {
                let (bid, ask) = b.get(l);
                (bid, ask, l.mid)
            }
            None => (0.0, 0.0, 0.0),
        };
        let delta_pct = match (s.first(), s.last()) {
            (Some(f), Some(l)) if f.mid > 0.0 => (l.mid - f.mid) / f.mid * 100.0,
            _ => 0.0,
        };
        (last_bid, last_ask, last_mid, delta_pct)
    });

    // Enough data to draw?
    let has_data: Memo<bool> = Memo::new(move |_| samples.get().len() > 1);

    view! {
        <Card>
            <CardHeader class="pb-2">
                <div class="flex items-start justify-between gap-3 flex-wrap">
                    <div class="flex flex-col gap-0.5">
                        <CardTitle class="font-mono text-base">
                            "Depth History — "
                            <span class="text-muted-foreground">{move || market.get().label()}</span>
                        </CardTitle>
                        <CardDescription>
                            "resting notional within the band, summed across both venues · mid on the right axis"
                        </CardDescription>
                    </div>
                    <div class="flex items-center gap-2 flex-wrap">
                        // Band tabs.
                        <div class="flex items-center rounded-lg border border-border bg-card p-0.5 gap-0.5">
                            <For each=move || Band::ALL.to_vec() key=|b| format!("{b:?}") let: b>
                                {let band = band.clone();
                                view! {
                                    <button
                                        class=move || format!(
                                            "h-6 rounded-md px-2 text-[11px] font-mono transition-colors cursor-pointer {}",
                                            if band.get() == b { "bg-secondary text-secondary-foreground" } else { "text-muted-foreground hover:text-foreground" },
                                        )
                                        on:click=move |_| band.set(b)
                                    >{b.label()}</button>
                                }}
                            </For>
                        </div>
                        // Window tabs.
                        <div class="flex items-center rounded-lg border border-border bg-card p-0.5 gap-0.5">
                            <For each=move || Win::ALL.to_vec() key=|w| format!("{w:?}") let: w>
                                {let win = win.clone();
                                view! {
                                    <button
                                        class=move || format!(
                                            "h-6 rounded-md px-2 text-[11px] font-mono transition-colors cursor-pointer {}",
                                            if win.get() == w { "bg-secondary text-secondary-foreground" } else { "text-muted-foreground hover:text-foreground" },
                                        )
                                        on:click=move |_| win.set(w)
                                    >{w.label()}</button>
                                }}
                            </For>
                        </div>
                    </div>
                </div>
            </CardHeader>
            <CardContent>
                <Show
                    when=move || has_data.get()
                    fallback=move || {
                        view! {
                            <div class="flex flex-col gap-2 py-6">
                                <Skeleton class="h-[200px] w-full rounded-lg" />
                                <span class="text-center text-[11px] font-mono text-muted-foreground">
                                    {move || format!(
                                        "collecting depth history for {} — sampling every 5s\u{2026}",
                                        market.get().label(),
                                    )}
                                </span>
                            </div>
                        }
                    }
                >
                    <DepthSvg samples band />
                </Show>

                // Footer stats + legend.
                <div class="mt-3 flex items-center justify-between gap-3 flex-wrap">
                    <div class="flex items-center gap-4 text-xs font-mono tabular-nums">
                        <span class="flex items-center gap-1.5">
                            <span class="h-2 w-2 rounded-sm bg-emerald-500/70"></span>
                            <span class="text-emerald-300">{move || abbrev_usd(footer.get().0)}</span>
                            <span class="text-muted-foreground">"bid"</span>
                        </span>
                        <span class="flex items-center gap-1.5">
                            <span class="h-2 w-2 rounded-sm bg-rose-500/70"></span>
                            <span class="text-rose-300">{move || abbrev_usd(footer.get().1)}</span>
                            <span class="text-muted-foreground">"ask"</span>
                        </span>
                        <span class="flex items-center gap-1.5">
                            <span class="h-0.5 w-3 bg-amber-400/80"></span>
                            <span class="text-amber-200">{move || {
                                let m = footer.get().2;
                                if m >= 1000.0 { format!("{m:.1}") } else { format!("{m:.4}") }
                            }}</span>
                            <span class="text-muted-foreground">"mid"</span>
                        </span>
                    </div>
                    <span class=move || {
                        let d = footer.get().3;
                        let cls = if d > 0.0 {
                            "text-emerald-300"
                        } else if d < 0.0 {
                            "text-rose-300"
                        } else {
                            "text-muted-foreground"
                        };
                        format!("text-xs font-mono tabular-nums {cls}")
                    }>
                        {move || {
                            let d = footer.get().3;
                            let glyph = if d > 0.0 { "\u{25B2}" } else if d < 0.0 { "\u{25BC}" } else { "" };
                            format!("{glyph} {:.2}% over window", d)
                        }}
                    </span>
                </div>
            </CardContent>
        </Card>
    }
}

/// The SVG itself: shared y-scale for bid/ask depth, right axis for mid.
#[component]
fn DepthSvg(samples: Memo<Vec<DepthSample>>, band: RwSignal<Band>) -> impl IntoView {
    // Precomputed geometry, rebuilt only when data or band changes.
    let geom = Memo::new(move |_| {
        let s = samples.get();
        let b = band.get();
        let plot_w = W - PAD_L - PAD_R;
        let plot_h = H - PAD_T - PAD_B;

        let t0 = s.first().map(|x| x.t).unwrap_or(0);
        let t1 = s.last().map(|x| x.t).unwrap_or(1).max(t0 + 1);
        let span = (t1 - t0) as f64;

        let mut dmax = f64::MIN_POSITIVE;
        let (mut pmin, mut pmax) = (f64::MAX, f64::MIN);
        for x in &s {
            let (bid, ask) = b.get(x);
            dmax = dmax.max(bid).max(ask);
            pmin = pmin.min(x.mid);
            pmax = pmax.max(x.mid);
        }
        dmax *= 1.1;
        if (pmax - pmin).abs() < pmax.abs() * 0.0005 || pmin == pmax {
            let pad = pmax.abs().max(1e-8) * 0.004;
            pmin -= pad;
            pmax += pad;
        }

        let x_of = |t: u64| PAD_L + (t - t0) as f64 / span * plot_w;
        let y_d = |v: f64| PAD_T + (1.0 - (v / dmax).clamp(0.0, 1.0)) * plot_h;
        let y_p = |p: f64| PAD_T + (1.0 - ((p - pmin) / (pmax - pmin)).clamp(0.0, 1.0)) * plot_h;

        let bid_line = polyline(&s, |x| b.get(x).0, x_of, y_d);
        let ask_line = polyline(&s, |x| b.get(x).1, x_of, y_d);
        let mid_line = polyline(&s, |x| x.mid, x_of, y_p);

        let base = PAD_T + plot_h;
        let area = |l: &str| -> String {
            if l.is_empty() {
                String::new()
            } else {
                // Insert baseline at both ends.
                let first_x = PAD_L;
                let last_x = PAD_L + plot_w;
                let (_, rest) = l.split_once(' ').unwrap_or(("", l));
                format!("M {first_x:.1},{base:.1} L {rest} L {last_x:.1},{base:.1} Z")
            }
        };
        let bid_area = area(&bid_line);
        let ask_area = area(&ask_line);

        // Axis labels.
        let d_ticks: Vec<(f64, String)> = (0..=2)
            .map(|i| {
                let v = dmax * (0.25 * i as f64 + 0.25);
                (y_d(v), abbrev_usd(v))
            })
            .collect();
        let p_ticks: Vec<(f64, String)> = (0..=2)
            .map(|i| {
                let p = pmin + (pmax - pmin) * (0.25 * i as f64 + 0.25);
                (y_p(p), if p >= 1000.0 { format!("{p:.0}") } else { format!("{p:.4}") })
            })
            .collect();
        let t_ticks: Vec<(f64, String)> = (0..4)
            .map(|i| {
                let t = t0 + span as u64 * (i + 1) / 5;
                (x_of(t), fmt_hm(t))
            })
            .collect();

        ChartGeom {
            bid_line,
            ask_line,
            mid_line,
            bid_area,
            ask_area,
            d_ticks,
            p_ticks,
            t_ticks,
        }
    });

    view! {
        <svg
            viewBox=format!("0 0 {W} {H}")
            class="w-full h-auto select-none"
            preserveAspectRatio="none"
            role="img"
            aria-label="Historical depth chart"
        >
            // Grid.
            <For each=move || geom.get().d_ticks.clone() key=|t| format!("{:.0}_{}", t.0, t.1) let: t>
                <line x1=PAD_L x2=W - PAD_R y1=t.0 y2=t.0 class="stroke-border" stroke-dasharray="3 5"></line>
            </For>

            // Ask (red) then bid (green) areas.
            <path d=move || geom.get().ask_area.clone() class="fill-rose-500/30"></path>
            <path d=move || geom.get().bid_area.clone() class="fill-emerald-500/30"></path>
            <path d=move || geom.get().ask_line.clone() class="stroke-rose-400/80" fill="none" stroke-width="1.5"></path>
            <path d=move || geom.get().bid_line.clone() class="stroke-emerald-400/80" fill="none" stroke-width="1.5"></path>

            // Mid overlay (right axis).
            <path
                d=move || geom.get().mid_line.clone()
                class="stroke-amber-400/80"
                fill="none"
                stroke-width="1.25"
                stroke-dasharray="5 4"
            ></path>

            // Left labels: notional (drawn inside plot, left-aligned).
            <For each=move || geom.get().d_ticks.clone() key=|t| format!("{:.0}_{}", t.0, t.1) let: t>
                <text x=PAD_L + 2.0 y=t.0 - 3.0 class="fill-muted-foreground font-mono" font-size="9">{t.1.clone()}</text>
            </For>

            // Right labels: price.
            <For each=move || geom.get().p_ticks.clone() key=|t| format!("{:.0}_{}", t.0, t.1) let: t>
                <text x=W - PAD_R + 6.0 y=t.0 + 3.0 class="fill-amber-200/70 font-mono" font-size="9">{t.1.clone()}</text>
            </For>

            // Bottom axis.
            <line x1=PAD_L x2=W - PAD_R y1=H - PAD_B y2=H - PAD_B class="stroke-border"></line>
            <For each=move || geom.get().t_ticks.clone() key=|t| format!("{:.0}_{}", t.0, t.1) let: t>
                <text x=t.0 y=H - 7.0 class="fill-muted-foreground font-mono" font-size="9" text-anchor="middle">{t.1.clone()}</text>
            </For>
        </svg>
    }
}

#[derive(Clone, PartialEq)]
struct ChartGeom {
    bid_line: String,
    ask_line: String,
    mid_line: String,
    bid_area: String,
    ask_area: String,
    d_ticks: Vec<(f64, String)>,
    p_ticks: Vec<(f64, String)>,
    t_ticks: Vec<(f64, String)>,
}

/// Build an SVG polyline path `M x,y L x,y ...` from the samples.
fn polyline<F, X, Y>(s: &[DepthSample], f: F, x_of: X, y: Y) -> String
where
    F: Fn(&DepthSample) -> f64,
    X: Fn(u64) -> f64,
    Y: Fn(f64) -> f64,
{
    let mut d = String::with_capacity(s.len() * 16);
    for (i, x) in s.iter().enumerate() {
        let (cmd, sep) = if i == 0 { ("M", "") } else { ("L", " ") };
        d.push_str(&format!("{sep}{cmd} {:.1},{:.1}", x_of(x.t), y(f(x))));
    }
    d
}
