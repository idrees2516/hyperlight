//! The trade tape: recent executed trades from both venues, newest first.
//!
//! Rows are keyed by the venue trade id, so `For` only inserts new rows at the
//! top (with a subtle side-colored flash animation) and existing rows shift
//! down without re-rendering.

use crate::components::*;
use crate::model::fmt_hms;
use crate::ws::Signals;
use leptos::prelude::*;

fn abbrev_notional(v: f64) -> String {
    if v.abs() >= 1_000_000.0 {
        format!("{:.2}M", v / 1_000_000.0)
    } else if v.abs() >= 1_000.0 {
        format!("{:.1}K", v / 1_000.0)
    } else if v.abs() >= 1.0 {
        format!("{v:.0}")
    } else {
        format!("{v:.2}")
    }
}

fn abbrev_size(v: f64) -> String {
    if v.abs() >= 1_000_000.0 {
        format!("{:.2}M", v / 1_000_000.0)
    } else if v.abs() >= 10_000.0 {
        format!("{:.1}K", v / 1_000.0)
    } else {
        format!("{v}")
    }
}

/// Venue short code + text color for the tape's venue chip.
fn venue_chip(v: ob_core::Venue) -> (&'static str, &'static str) {
    use ob_core::Venue;
    match v {
        Venue::Hyperliquid => ("HL", "text-amber-300/90"),
        Venue::Lighter => ("LT", "text-teal-300/90"),
        Venue::Binance => ("BN", "text-yellow-300/90"),
        Venue::Bybit => ("BY", "text-orange-300/90"),
        Venue::Okx => ("OK", "text-sky-300/90"),
        Venue::Kraken => ("KR", "text-violet-300/90"),
        Venue::Coinbase => ("CB", "text-blue-300/90"),
        Venue::Bitstamp => ("BS", "text-emerald-300/90"),
        Venue::Gate => ("GT", "text-rose-300/90"),
    }
}

#[component]
pub fn TradeTape(sig: Signals) -> impl IntoView {
    let market = sig.selected;
    let tape: Memo<Vec<ob_core::Trade>> = Memo::new(move |_| {
        sig.tapes
            .get()
            .get(&market.get())
            .cloned()
            .unwrap_or_default()
    });

    // Buy-pressure over the visible tape.
    let buy_pct = Memo::new(move |_| {
        let t = tape.get();
        if t.is_empty() {
            return 50.0f64;
        }
        let buys = t.iter().filter(|x| x.side == "B").count();
        buys as f64 / t.len() as f64 * 100.0
    });

    view! {
        <Card>
            <CardHeader class="pb-3">
                <div class="flex items-center justify-between gap-2">
                    <div class="flex items-center gap-2">
                        <CardTitle class="text-base">"Trade Tape"</CardTitle>
                        <Badge variant=BadgeVariant::Secondary class="font-mono text-[10px]">
                            {move || market.get().label()}
                        </Badge>
                    </div>
                    <span class="flex items-center gap-1.5 text-[11px] font-mono text-muted-foreground">
                        <PulseDot />
                        "taker flow"
                    </span>
                </div>
                // Buy-pressure meter.
                <div>
                    <div class="flex h-1.5 w-full overflow-hidden rounded-full bg-rose-500/20">
                        <div
                            class="depth-bar h-full bg-emerald-500/70 rounded-l-full"
                            style=move || format!("width: {:.1}%", buy_pct.get())
                        ></div>
                    </div>
                    <div class="mt-1 flex justify-between text-[10px] font-mono">
                        <span class="text-emerald-400/80">{move || format!("{:.0}% buys", buy_pct.get())}</span>
                        <span class="text-rose-400/80">{move || format!("{:.0}% sells", 100.0 - buy_pct.get())}</span>
                    </div>
                </div>
            </CardHeader>
            <CardContent>
                <div class="grid grid-cols-[auto_minmax(0,1fr)_auto_auto] items-center gap-2 px-3 h-6 text-[10px] uppercase tracking-wider text-muted-foreground border-b border-border">
                    <span>"Time"</span>
                    <span class="text-right">"Price"</span>
                    <span class="text-right">"Size"</span>
                    <span class="w-9 text-right">"Venue"</span>
                </div>
                <Show
                    when=move || !tape.get().is_empty()
                    fallback=move || {
                        view! {
                            <div class="flex flex-col gap-1.5 p-3">
                                <Skeleton class="h-4 w-full" />
                                <Skeleton class="h-4 w-10/12" />
                                <Skeleton class="h-4 w-full" />
                                <span class="text-[11px] font-mono text-muted-foreground pt-1">
                                    "waiting for trades\u{2026}"
                                </span>
                            </div>
                        }
                    }
                >
                    <div class="max-h-[300px] xl:max-h-[420px] overflow-y-auto">
                        <For each=move || tape.get() key=|t| t.id.clone() let: t>
                            {let is_buy = t.side == "B";
                            let flash = if is_buy { "tape-flash-buy" } else { "tape-flash-sell" };
                            let (px_cls, side_glyph) = if is_buy {
                                ("text-emerald-300", "+")
                            } else {
                                ("text-rose-300", "\u{2212}")
                            };
                            let usd = t.px.parse::<f64>().unwrap_or(0.0)
                                * t.sz.parse::<f64>().unwrap_or(0.0);
                            let sz_disp = abbrev_size(t.sz.parse::<f64>().unwrap_or(0.0));
                            let (venue_label, venue_cls) = venue_chip(t.venue);
                            let time = fmt_hms(t.t);
                            let usd_disp = abbrev_notional(usd);
                            view! {
                                <div class=format!(
                                    "grid grid-cols-[auto_minmax(0,1fr)_auto_auto] items-center gap-2 px-3 h-[24px] text-[12px] font-mono tabular-nums {flash}",
                                )>
                                    <span class="text-muted-foreground">{time}</span>
                                    <span class=format!("text-right font-medium {px_cls}")>
                                        {move || t.px.clone()}
                                    </span>
                                    <span class="text-right text-zinc-300 flex items-baseline gap-1.5">
                                        {move || sz_disp.clone()}
                                        <span class="text-[10px] text-muted-foreground/70 hidden sm:inline">
                                            {move || format!("${}", usd_disp.clone())}
                                        </span>
                                    </span>
                                    <span class="w-9 flex justify-end">
                                        <span class=format!(
                                            "rounded border px-1 py-px text-[9px] font-sans leading-none {venue_cls}",
                                        )>{venue_label}</span>
                                    </span>
                                    <span class="hidden">{side_glyph}</span>
                                </div>
                            }}
                        </For>
                    </div>
                </Show>
            </CardContent>
        </Card>
    }
}
