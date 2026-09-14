//! shadcn/ui component set, implemented natively in Leptos.
//!
//! shadcn/ui's distribution philosophy is "copy the component source into
//! your project" — this is exactly that, with Tailwind classes mirroring the
//! canonical shadcn/ui (New York, zinc, dark) tokens defined in style.css.

use leptos::prelude::*;

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
// Separator
// ---------------------------------------------------------------------------

#[component]
pub fn Separator(#[prop(optional, default = "")] class: &'static str) -> impl IntoView {
    view! { <div class=format!("h-px w-full bg-border {class}")></div> }
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
