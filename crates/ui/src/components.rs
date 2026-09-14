//! shadcn/ui component set, implemented natively in Leptos.
//!
//! shadcn/ui's distribution philosophy is "copy the component source into
//! your project" — this is exactly that, with Tailwind classes mirroring the
//! canonical shadcn/ui (New York, zinc, dark) tokens defined in style.css.

use crate::model::TickerData;
use leptos::prelude::*;
use ob_core::Market;
use std::collections::HashMap;

// ---------------------------------------------------------------------------
// Card
// ---------------------------------------------------------------------------

#[component]
pub fn Card(#[prop(optional, default = "")] class: &'static str, children: Children) -> impl IntoView {
    view! {
        <div class=format!(
            "rounded-xl border bg-card text-card-foreground shadow {class}",
        )>{children()}</div>
    }
}

#[component]
pub fn CardHeader(#[prop(optional, default = "")] class: &'static str, children: Children) -> impl IntoView {
    view! { <div class=format!("flex flex-col space-y-1.5 p-5 {class}")>{children()}</div> }
}

#[component]
pub fn CardTitle(#[prop(optional, default = "")] class: &'static str, children: Children) -> impl IntoView {
    view! { <div class=format!("font-semibold leading-none tracking-tight {class}")>{children()}</div> }
}

#[component]
pub fn CardDescription(
    #[prop(optional, default = "")] class: &'static str,
    children: Children,
) -> impl IntoView {
    view! { <div class=format!("text-sm text-muted-foreground {class}")>{children()}</div> }
}

#[component]
pub fn CardContent(#[prop(optional, default = "")] class: &'static str, children: Children) -> impl IntoView {
    view! { <div class=format!("p-5 pt-0 {class}")>{children()}</div> }
}

// ---------------------------------------------------------------------------
// Badge
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)] // full shadcn API surface; variants kept for parity
pub enum BadgeVariant {
    Default,
    Secondary,
    Outline,
    Success,
    Warning,
    Destructive,
}

impl BadgeVariant {
    fn classes(self) -> &'static str {
        match self {
            BadgeVariant::Default => {
                "border-transparent bg-primary text-primary-foreground [a&]:hover:bg-primary/90"
            }
            BadgeVariant::Secondary => {
                "border-transparent bg-secondary text-secondary-foreground [a&]:hover:bg-secondary/90"
            }
            BadgeVariant::Outline => "text-foreground",
            BadgeVariant::Success => {
                "border-emerald-900/70 bg-emerald-950/60 text-emerald-300"
            }
            BadgeVariant::Warning => {
                "border-amber-900/70 bg-amber-950/50 text-amber-300"
            }
            BadgeVariant::Destructive => {
                "border-rose-900/70 bg-rose-950/50 text-rose-300"
            }
        }
    }
}

#[component]
pub fn Badge(
    variant: BadgeVariant,
    #[prop(optional, default = "")] class: &'static str,
    children: Children,
) -> impl IntoView {
    view! {
        <span class=format!(
            "inline-flex items-center gap-1.5 rounded-full border px-2.5 py-0.5 text-xs font-medium font-mono transition-colors focus:outline-none {} {}",
            variant.classes(),
            class,
        )>{children()}</span>
    }
}

/// Pulsing status dot (the classic "live" indicator).
#[component]
pub fn PulseDot(#[prop(optional, default = "bg-emerald-400")] color: &'static str) -> impl IntoView {
    view! {
        <span class="relative flex h-2 w-2">
            <span class=format!(
                "absolute inline-flex h-full w-full animate-ping rounded-full opacity-60 {color}",
            )></span>
            <span class=format!("relative inline-flex h-2 w-2 rounded-full {color}")></span>
        </span>
    }
}

// ---------------------------------------------------------------------------
// Button (used for market selector and depth tabs)
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)] // full shadcn API surface; variants kept for parity
pub enum ButtonVariant {
    Default,
    Secondary,
    Ghost,
    Outline,
}

impl ButtonVariant {
    fn classes(self) -> &'static str {
        match self {
            ButtonVariant::Default => {
                "bg-primary text-primary-foreground shadow hover:bg-primary/90"
            }
            ButtonVariant::Secondary => {
                "bg-secondary text-secondary-foreground hover:bg-secondary/80"
            }
            ButtonVariant::Ghost => "hover:bg-accent hover:text-accent-foreground",
            ButtonVariant::Outline => "border bg-background shadow-sm hover:bg-accent hover:text-accent-foreground",
        }
    }
}

#[component]
pub fn Button(
    variant: ButtonVariant,
    #[prop(optional, default = "")] class: &'static str,
    #[prop(optional, default = "")] button_type: &'static str,
    on_click: impl Fn() + 'static + Clone,
    children: Children,
) -> impl IntoView {
    let on_click = move || on_click();
    view! {
        <button
            type=button_type
            on:click=move |_| on_click()
            class=format!(
                "inline-flex items-center justify-center gap-1.5 whitespace-nowrap rounded-lg text-sm font-medium transition-colors focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring disabled:pointer-events-none disabled:opacity-50 [&_svg]:pointer-events-none [&_svg]:size-4 [&_svg]:shrink-0 cursor-pointer h-9 px-4 {} {}",
                variant.classes(),
                class,
            )
        >
            {children()}
        </button>
    }
}

// ---------------------------------------------------------------------------
// Input
// ---------------------------------------------------------------------------

#[component]
pub fn Input(
    value: RwSignal<String>,
    #[prop(optional, default = "")] placeholder: &'static str,
    #[prop(optional, default = "")] class: &'static str,
    #[prop(optional)] on_focus: Option<Box<dyn Fn() + Send>>,
) -> impl IntoView {
    view! {
        <input
            type="text"
            inputmode="decimal"
            placeholder=placeholder
            prop:value=move || value.get()
            on:input=move |ev| value.set(event_target_value(&ev))
            on:focus=move |_| {
                if let Some(f) = &on_focus { f(); }
            }
            class=format!(
                "flex h-9 w-full rounded-lg border border-input bg-background px-3 py-1 text-sm font-mono tabular-nums shadow-sm transition-colors placeholder:text-muted-foreground/60 focus-visible:outline-none focus-visible:ring-1 focus-visible:ring-ring disabled:cursor-not-allowed disabled:opacity-50 {class}",
            )
        />
    }
}

// ---------------------------------------------------------------------------
// MarketSelect — shadcn-style dropdown over live tickers
// ---------------------------------------------------------------------------

#[component]
pub fn MarketSelect(
    selected: RwSignal<Market>,
    tickers: RwSignal<HashMap<Market, TickerData>>,
) -> impl IntoView {
    let open = RwSignal::new(false);
    let trigger_mid: Memo<String> = Memo::new(move |_| {
        tickers
            .get()
            .get(&selected.get())
            .map(|t| t.mid.clone())
            .unwrap_or_else(|| "\u{2014}".into())
    });

    view! {
        <div class="relative">
            <button
                class="flex items-center gap-2.5 h-9 rounded-lg border border-border bg-card px-3 cursor-pointer transition-colors hover:bg-muted/50"
                on:click=move |_| open.update(|o| *o = !*o)
            >
                <span class="font-mono text-sm font-semibold tracking-tight">
                    {move || selected.get().label()}
                </span>
                <span class="font-mono text-sm tabular-nums text-muted-foreground hidden sm:inline">
                    {move || trigger_mid.get()}
                </span>
                <span class=format!(
                    "text-muted-foreground text-[10px] transition-transform {}",
                    "",
                )>{move || if open.get() { "\u{25B4}" } else { "\u{25BE}" }}</span>
            </button>

            <Show when=move || open.get()>
                // Click-away + dim backdrop so the popover reads as a layer.
                <div class="fixed inset-0 z-40 cursor-default bg-black/50" on:click=move |_| open.set(false)></div>
                <div class="absolute right-0 top-full mt-1.5 z-50 w-[320px] max-h-[430px] overflow-y-auto rounded-xl border border-zinc-700/80 bg-popover text-popover-foreground shadow-2xl shadow-black/50 p-1.5">
                    <div class="px-2.5 py-1.5 text-[10px] uppercase tracking-wider text-muted-foreground flex justify-between">
                        <span>"Market"</span>
                        <span>"mid · spread"</span>
                    </div>
                    <For each=move || Market::ALL.to_vec() key=|m| format!("{m:?}") let: m>
                        {let selected = selected.clone();
                        let tickers = tickers.clone();
                        let open = open.clone();
                        let active = move || selected.get() == m;
                        let mid = move || {
                            tickers.get().get(&m).map(|t| t.mid.clone())
                                .unwrap_or_else(|| "\u{2014}".into())
                        };
                        let mid_class = move || {
                            match tickers.get().get(&m).map(|t| t.dir) {
                                Some(1) => "text-emerald-300",
                                Some(-1) => "text-rose-300",
                                _ => "text-zinc-300",
                            }
                        };
                        let bps = move || {
                            tickers.get().get(&m)
                                .map(|t| format!("{} bps", t.spread_bps))
                                .unwrap_or_default()
                        };
                        view! {
                            <button
                                class=format!(
                                    "group w-full flex items-center justify-between gap-3 px-2.5 py-2 rounded-lg cursor-pointer transition-colors {}",
                                    if active() { "bg-muted" } else { "hover:bg-muted/60" },
                                )
                                on:click=move |_| {
                                    selected.set(m);
                                    open.set(false);
                                }
                            >
                                <span class="flex items-center gap-2 min-w-0">
                                    <span class=format!(
                                        "h-1.5 w-1.5 rounded-full shrink-0 {}",
                                        if active() { "bg-emerald-400" } else { "bg-zinc-700 group-hover:bg-zinc-500" },
                                    )></span>
                                    <span class="font-mono text-[13px] font-medium truncate">{m.label()}</span>
                                    <span class="rounded border border-border px-1 text-[9px] font-mono text-muted-foreground shrink-0">
                                        {format!("#{}", m.lighter_market_index())}
                                    </span>
                                </span>
                                <span class="flex items-baseline gap-2 shrink-0">
                                    <span class=format!("font-mono text-[13px] tabular-nums {}", mid_class())>
                                        {move || mid().clone()}
                                    </span>
                                    <span class="font-mono text-[10px] tabular-nums text-muted-foreground hidden xs:block">
                                        {move || bps().clone()}
                                    </span>
                                </span>
                            </button>
                        }}
                    </For>
                </div>
            </Show>
        </div>
    }
}

// ---------------------------------------------------------------------------
// Separator
// ---------------------------------------------------------------------------

#[component]
pub fn Separator(#[prop(optional, default = "")] class: &'static str) -> impl IntoView {
    let _ = class; // full shadcn API surface
    view! { <div class="h-px w-full bg-border"></div> }
}

// ---------------------------------------------------------------------------
// Skeleton — loading placeholder
// ---------------------------------------------------------------------------

#[component]
pub fn Skeleton(#[prop(optional, default = "h-5 w-full")] class: &'static str) -> impl IntoView {
    view! { <div class=format!("animate-pulse rounded-md bg-muted {class}")></div> }
}

// ---------------------------------------------------------------------------
// Stat — label + value tile used across the top strip
// ---------------------------------------------------------------------------

#[component]
pub fn StatTile(
    label: &'static str,
    value: Memo<String>,
    #[prop(optional)] value_class: Option<Memo<String>>,
    #[prop(optional)] sub: Option<Memo<String>>,
    children: Children,
) -> impl IntoView {
    view! {
        <Card class="p-4 gap-0 flex flex-col justify-between rounded-xl">
            <div class="flex items-center justify-between">
                <span class="text-xs font-medium uppercase tracking-wider text-muted-foreground">{label}</span>
            </div>
            <div class=move || {
                format!(
                    "mt-1.5 font-mono tabular-nums text-lg font-semibold leading-none truncate {}",
                    value_class.as_ref().map(|m| m.get()).unwrap_or_default(),
                )
            }>
                {move || value.get()}
            </div>
            <div class="mt-1.5 text-xs text-muted-foreground font-mono flex items-center gap-2 h-4">
                {move || sub.as_ref().map(|m| m.get()).unwrap_or_default()}
                {children()}
            </div>
        </Card>
    }
}

// ---------------------------------------------------------------------------
// Toasts — transient notifications, bottom-right stack
// ---------------------------------------------------------------------------

#[component]
pub fn Toasts(toasts: RwSignal<Vec<crate::model::Toast>>) -> impl IntoView {
    view! {
        <div class="fixed bottom-4 right-4 z-[70] flex flex-col gap-2 w-[min(92vw,360px)]">
            <For
                each=move || toasts.get()
                key=|t| t.id
                let: t
            >
                {let toasts = toasts.clone();
                let (border, icon, icon_color) = match t.kind {
                    crate::model::ToastKind::Info => (
                        "border-border",
                        "i",
                        "text-zinc-300",
                    ),
                    crate::model::ToastKind::Success => (
                        "border-emerald-900/70",
                        "\u{2713}",
                        "text-emerald-300",
                    ),
                    crate::model::ToastKind::Warning => (
                        "border-amber-900/70",
                        "!",
                        "text-amber-300",
                    ),
                };
                let title = t.title.clone();
                let body = t.body.clone();
                view! {
                    <div class=format!(
                        "toast-in flex items-start gap-3 rounded-xl border {border} bg-popover/95 backdrop-blur px-4 py-3 shadow-2xl",
                    )>
                        <span class=format!(
                            "flex h-5 w-5 shrink-0 items-center justify-center rounded-full border border-border font-mono text-[11px] font-bold {icon_color}",
                        )>{icon}</span>
                        <div class="min-w-0 flex-1">
                            <div class="text-[13px] font-semibold leading-tight">{title.clone()}</div>
                            <div class="mt-0.5 font-mono text-xs text-muted-foreground leading-snug break-words">{body.clone()}</div>
                        </div>
                        <button
                            class="shrink-0 text-muted-foreground hover:text-foreground cursor-pointer text-xs -mr-1 -mt-0.5 px-1"
                            on:click=move |_| {
                                let id = t.id;
                                toasts.update(|list| list.retain(|x| x.id != id));
                            }
                        >
                            "\u{2715}"
                        </button>
                    </div>
                }}
            </For>
        </div>
    }
}
