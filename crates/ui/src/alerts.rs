//! Price alerts: create form (direction + threshold), active list, triggered
//! log. Alerts live server-side; this panel sends commands over the socket and
//! mirrors the server's `AlertSet` / `AlertFired` events.

use crate::components::*;
use crate::model::fmt_hms;
use crate::ws::Signals;
use leptos::prelude::*;
use ob_core::AlertDir;

#[component]
pub fn AlertsPanel(sig: Signals) -> impl IntoView {
    let market = sig.selected;
    let dir = RwSignal::new(AlertDir::Above);
    let price = RwSignal::new(String::new());

    // Current mid of the selected market (for quick-fill buttons).
    let mid = Memo::new(move |_| {
        sig.tickers
            .get()
            .get(&market.get())
            .map(|t| t.mid.parse::<f64>().ok())
            .flatten()
    });

    let on_focus = {
        let price = price.clone();
        let mid = mid.clone();
        let market = market.clone();
        let sig = sig;
        move || {
            if price.get().is_empty() {
                if let Some(m) = mid.get_untracked() {
                    price.set(format!("{m}"));
                } else if let Some(b) = sig.books.get().get(market.get_untracked()) {
                    price.set(b.stats.mid.clone());
                }
            }
        }
    };

    let create = {
        let sig = sig;
        let market = market.clone();
        let dir = dir.clone();
        let price = price.clone();
        move || {
            let p = price.get().trim().to_string();
            let m = market.get();
            if p.is_empty() {
                sig.push_toast(
                    crate::model::ToastKind::Info,
                    "Enter a price".into(),
                    "Type a threshold price for the alert first.".into(),
                );
                return;
            }
            sig.create_alert(m, dir.get(), p.clone());
            sig.push_toast(
                crate::model::ToastKind::Success,
                "Alert armed".into(),
                format!(
                    "{} {} {}",
                    m.label(),
                    if dir.get() == AlertDir::Above { "crosses above" } else { "crosses below" },
                    p,
                ),
            );
            price.set(String::new());
        }
    };

    let _ = sig.alerts; // read via sig inside AlertList

    // Box the focus handler outside the view (generic `>` breaks RSX parsing).
    let on_focus_prop: Box<dyn Fn() + Send> = Box::new(on_focus.clone());

    view! {
        <Card>
            <CardHeader class="pb-3">
                <div class="flex items-center justify-between gap-2">
                    <CardTitle class="text-base">"Price Alerts"</CardTitle>
                    <Badge variant=BadgeVariant::Secondary class="font-mono text-[10px]">
                        {move || market.get().label()}
                    </Badge>
                </div>
                // Create form: direction toggle + price + Create.
                <div class="flex items-center gap-2 mt-1">
                    <div class="flex items-center rounded-lg border border-border bg-card p-0.5 gap-0.5 shrink-0">
                        <button
                            class=move || format!(
                                "h-7 rounded-md px-2.5 text-[12px] font-mono transition-colors cursor-pointer {}",
                                if dir.get() == AlertDir::Above {
                                    "bg-emerald-950/80 text-emerald-300 border border-emerald-900/70"
                                } else {
                                    "text-muted-foreground hover:text-foreground"
                                },
                            )
                            on:click=move |_| dir.set(AlertDir::Above)
                            title="Alert when mid crosses ABOVE the price"
                        >
                            <span class="text-[10px]">"\u{25B2}"</span>" above"
                        </button>
                        <button
                            class=move || format!(
                                "h-7 rounded-md px-2.5 text-[12px] font-mono transition-colors cursor-pointer {}",
                                if dir.get() == AlertDir::Below {
                                    "bg-rose-950/80 text-rose-300 border border-rose-900/70"
                                } else {
                                    "text-muted-foreground hover:text-foreground"
                                },
                            )
                            on:click=move |_| dir.set(AlertDir::Below)
                            title="Alert when mid crosses BELOW the price"
                        >
                            <span class="text-[10px]">"\u{25BC}"</span>" below"
                        </button>
                    </div>
                    <div class="flex-1 min-w-0">
                        <Input
                            value=price
                            placeholder="price"
                            class="border-zinc-700/80 bg-zinc-900/40"
                            on_focus=on_focus_prop
                        />
                    </div>
                    <Button variant=ButtonVariant::Default class="h-9 px-3 text-[13px] shrink-0" on_click=move || create()>
                        "Set"
                    </Button>
                </div>
                // Quick fills: ±1% of current mid.
                <div class="flex items-center justify-between gap-2">
                    <div class="flex items-center gap-1">
                        <span class="text-[10px] font-mono text-muted-foreground mr-0.5">"quick:"</span>
                        {move || {
                            let price = price.clone();
                            match mid.get() {
                                Some(m) => {
                                    let below = format!("{:.6}", m * 0.99);
                                    let above = format!("{:.6}", m * 1.01);
                                    let (b2, a2) = (below.clone(), above.clone());
                                    view! {
                                        <button
                                            class="rounded border border-border px-1.5 py-0.5 text-[10px] font-mono text-muted-foreground hover:text-rose-300 hover:border-rose-900/60 cursor-pointer"
                                            on:click=move |_| price.set(below.clone())
                                            title=format!("-1% of mid ({})", b2)
                                        >"-1%"</button>
                                        <button
                                            class="rounded border border-border px-1.5 py-0.5 text-[10px] font-mono text-muted-foreground hover:text-emerald-300 hover:border-emerald-900/60 cursor-pointer"
                                            on:click=move |_| price.set(above.clone())
                                            title=format!("+1% of mid ({})", a2)
                                        >"+1%"</button>
                                    }.into_any()
                                }
                                None => view! {
                                    <span class="text-[10px] font-mono text-muted-foreground/60">"\u{2014}"</span>
                                }.into_any(),
                            }
                        }}
                    </div>
                    <span class="text-[10px] font-mono text-muted-foreground/70">
                        "checked at 10 Hz server-side"
                    </span>
                </div>
            </CardHeader>
            <CardContent>
                <AlertList sig />
            </CardContent>
        </Card>
    }
}

#[component]
fn AlertList(sig: Signals) -> impl IntoView {
    let active: Memo<Vec<ob_core::Alert>> = Memo::new(move |_| {
        sig.alerts
            .get()
            .into_iter()
            .filter(|a| a.triggered_ms.is_none())
            .collect()
    });
    let fired: Memo<Vec<ob_core::Alert>> = Memo::new(move |_| {
        let mut v: Vec<ob_core::Alert> = sig
            .alerts
            .get()
            .into_iter()
            .filter(|a| a.triggered_ms.is_some())
            .collect();
        v.sort_by_key(|a| std::cmp::Reverse(a.triggered_ms));
        v
    });

    view! {
        <Show
            when=move || !active.get().is_empty() || !fired.get().is_empty()
            fallback=move || view! {
                <div class="py-3 text-center text-[12px] font-mono text-muted-foreground/70">
                    "no alerts yet — arm one above"
                </div>
            }
        >
            <div class="flex flex-col gap-1">
                <For each=move || active.get() key=|a| a.id let: a>
                    {view! { <AlertRow sig alert=a /> }}
                </For>
            </div>
            <Show when=move || !fired.get().is_empty()>
                <div class="mt-2 pt-2 border-t border-border">
                    <span class="text-[10px] uppercase tracking-wider text-muted-foreground">
                        "triggered"
                    </span>
                    <div class="mt-1 flex flex-col gap-1 opacity-75">
                        <For each=move || fired.get() key=|a| a.id let: a>
                            {view! { <AlertRow sig alert=a /> }}
                        </For>
                    </div>
                </div>
            </Show>
        </Show>
    }
}

#[component]
fn AlertRow(sig: Signals, alert: ob_core::Alert) -> impl IntoView {
    let is_above = alert.dir == AlertDir::Above;
    let fired = alert.triggered_ms.is_some();
    let (arrow, arrow_cls) = if is_above {
        ("\u{25B2}", "text-emerald-300")
    } else {
        ("\u{25BC}", "text-rose-300")
    };
    let price = alert.price.clone();
    let market_label = alert.market.label().to_string();
    let alert_id = alert.id;
    let del = {
        let sig = sig;
        move |id: u64| sig.delete_alert(id)
    };
    // Trigger line / armed hint, precomputed so the view stays Copy.
    let (tail_text, tail_cls) = if fired {
        (
            format!(
                "{} @ {}",
                fmt_hms(alert.triggered_ms.unwrap_or(0)),
                alert.triggered_px.clone().unwrap_or_else(|| "?".into()),
            ),
            "text-amber-300/90",
        )
    } else {
        ("armed".to_string(), "text-muted-foreground/60 uppercase tracking-wider")
    };
    view! {
        <div class="group flex items-center gap-2 rounded-lg border border-border/70 bg-background/40 px-2.5 h-9 text-[12px] font-mono tabular-nums">
            <span class=format!("w-3 text-center shrink-0 {}", arrow_cls)>{arrow}</span>
            <span class="font-medium">{price}</span>
            <span class="text-muted-foreground text-[11px]">{market_label}</span>
            <span class=format!("ml-auto text-[10px] pr-1 {}", tail_cls)>{tail_text}</span>
            <button
                class="ml-1 text-muted-foreground/50 hover:text-rose-300 cursor-pointer text-[11px] px-1 shrink-0"
                title="Delete alert"
                on:click=move |_| del(alert_id)
            >
                "\u{2715}"
            </button>
        </div>
    }
}
