//! Browser WebSocket client with auto-reconnect.
//!
//! Connects to the Axum backend at `/ws`, parses `WireEvent` JSON frames,
//! routes them into Leptos signals, and sends client commands (market
//! selection, alert create/delete, arbitrage config) over the same socket.

use crate::model::{
    ArbState, Books, BookData, Histories, LinkStatus, Tapes, TickerData, Toast, ToastKind,
    VenueStatuses,
};
use leptos::prelude::*;
use ob_core::{AlertDir, ArbConfigUpdate, Market, Venue, WireEvent, ARB_EQUITY_WINDOW};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use wasm_bindgen::closure::Closure;
use wasm_bindgen::JsCast;
use web_sys::{MessageEvent, WebSocket};

static TOAST_ID: AtomicU64 = AtomicU64::new(1);

/// How many trades the client keeps per market for the tape.
const TAPE_ROWS: usize = 60;

/// How many fills the client renders in the log.
const FILL_ROWS: usize = 40;

/// Commands the browser sends to the backend over `/ws`.
#[derive(serde::Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum ClientCmd {
    Select { market: Market },
    AlertCreate {
        market: Market,
        dir: AlertDir,
        price: String,
    },
    AlertDelete { id: u64 },
    ArbConfig {
        #[serde(flatten)]
        upd: ArbConfigUpdate,
    },
    ArbReset,
}

/// All shared reactive state.
#[derive(Clone, Copy)]
pub struct Signals {
    pub books: RwSignal<Books>,
    pub link: RwSignal<LinkStatus>,
    pub venues: RwSignal<VenueStatuses>,
    /// The market this browser is viewing (drives server-side selection).
    pub selected: RwSignal<Market>,
    /// Compact tickers for every market (market selector).
    pub tickers: RwSignal<HashMap<Market, TickerData>>,
    /// Newest-first trade tapes per market.
    pub tapes: RwSignal<Tapes>,
    /// Depth history per market (oldest-first).
    pub histories: RwSignal<Histories>,
    /// Server-side alerts (mirrors AlertSet).
    pub alerts: RwSignal<Vec<ob_core::Alert>>,
    /// Cross-venue arbitrage panel state.
    pub arb: RwSignal<ArbState>,
    /// Transient notifications.
    pub toasts: RwSignal<Vec<Toast>>,
    /// The live socket (for sending commands).
    pub ws: RwSignal<Option<WebSocket>>,
}

impl Signals {
    pub fn new() -> Self {
        Self {
            books: RwSignal::new(Books::default()),
            link: RwSignal::new(LinkStatus::Connecting),
            venues: RwSignal::new(VenueStatuses::default()),
            selected: RwSignal::new(Market::Eth),
            tickers: RwSignal::new(HashMap::new()),
            tapes: RwSignal::new(HashMap::new()),
            histories: RwSignal::new(HashMap::new()),
            alerts: RwSignal::new(Vec::new()),
            arb: RwSignal::new(ArbState::default()),
            toasts: RwSignal::new(Vec::new()),
            ws: RwSignal::new(None),
        }
    }

    fn send_cmd(&self, cmd: ClientCmd) {
        let Some(ws) = self.ws.get() else { return };
        if let Ok(json) = serde_json::to_string(&cmd) {
            let _ = ws.send_with_str(&json);
        }
    }

    /// Switch the server-side selection for this socket.
    pub fn select_market(&self, market: Market) {
        self.send_cmd(ClientCmd::Select { market });
    }

    pub fn create_alert(&self, market: Market, dir: AlertDir, price: String) {
        self.send_cmd(ClientCmd::AlertCreate {
            market,
            dir,
            price,
        });
    }

    pub fn delete_alert(&self, id: u64) {
        self.send_cmd(ClientCmd::AlertDelete { id });
    }

    /// Push a partial arbitrage engine config update.
    pub fn arb_config(&self, upd: ArbConfigUpdate) {
        self.send_cmd(ClientCmd::ArbConfig { upd });
    }

    /// Reset paper-trading stats.
    pub fn arb_reset(&self) {
        self.send_cmd(ClientCmd::ArbReset);
    }

    pub fn push_toast(&self, kind: ToastKind, title: String, body: String) {
        let id = TOAST_ID.fetch_add(1, Ordering::Relaxed);
        self.toasts.update(|t| {
            t.push(Toast {
                id,
                kind,
                title,
                body,
            });
            while t.len() > 4 {
                t.remove(0);
            }
        });
        let sig = *self;
        let cb = Closure::<dyn FnMut()>::new(move || {
            sig.toasts.update(|t| t.retain(|x| x.id != id));
        });
        if let Some(win) = web_sys::window() {
            let _ = win.set_timeout_with_callback_and_timeout_and_arguments_0(
                cb.as_ref().unchecked_ref(),
                8000,
            );
        }
        cb.forget();
    }
}

/// Start (or restart) the backend connection. Reconnects forever.
pub fn connect(sig: Signals) {
    let (url, ws) = match ws_url().and_then(|u| WebSocket::new(&u).ok().map(|w| (u, w))) {
        Some(x) => x,
        None => {
            schedule_reconnect(sig);
            return;
        }
    };
    leptos::logging::log!("connecting to {url}");

    {
        let sig = sig;
        let on_open = Closure::<dyn FnMut(_)>::new(move |_e: web_sys::Event| {
            sig.link.set(LinkStatus::Open);
        });
        ws.set_onopen(Some(on_open.as_ref().unchecked_ref()));
        on_open.forget();
    }

    {
        let sig = sig;
        let on_msg = Closure::<dyn FnMut(MessageEvent)>::new(move |e: MessageEvent| {
            let Some(text) = e.data().as_string() else { return };
            match serde_json::from_str::<WireEvent>(&text) {
                Ok(ev) => apply_event(sig, ev),
                Err(err) => leptos::logging::warn!("unparsable event: {err}"),
            }
        });
        ws.set_onmessage(Some(on_msg.as_ref().unchecked_ref()));
        on_msg.forget();
    }

    {
        let sig = sig;
        let on_err = Closure::<dyn FnMut(_)>::new(move |_e: web_sys::ErrorEvent| {
            sig.link.set(LinkStatus::Closed);
        });
        ws.set_onerror(Some(on_err.as_ref().unchecked_ref()));
        on_err.forget();
    }

    {
        let sig = sig;
        let on_close = Closure::<dyn FnMut(_)>::new(move |_e: web_sys::CloseEvent| {
            sig.link.set(LinkStatus::Closed);
            sig.ws.set(None);
            schedule_reconnect(sig);
        });
        ws.set_onclose(Some(on_close.as_ref().unchecked_ref()));
        on_close.forget();
    }

    sig.ws.set(Some(ws));
}

fn ws_url() -> Option<String> {
    let win = web_sys::window()?;
    let loc = win.location();
    let proto = loc.protocol().ok()?;
    let host = loc.host().ok()?;
    let scheme = if proto == "https:" { "wss" } else { "ws" };
    Some(format!("{scheme}://{host}/ws"))
}

fn schedule_reconnect(sig: Signals) {
    let retry = Closure::<dyn FnMut()>::new(move || connect(sig));
    if let Some(win) = web_sys::window() {
        let _ = win.set_timeout_with_callback_and_timeout_and_arguments_0(
            retry.as_ref().unchecked_ref(),
            1200,
        );
    }
    retry.forget();
}

fn now_ms() -> u64 {
    js_sys::Date::now().max(0.0) as u64
}

fn apply_event(sig: Signals, ev: WireEvent) {
    match ev {
        WireEvent::Book {
            market,
            venues,
            consolidated,
            stats,
            ts,
        } => {
            let map = venues
                .into_iter()
                .map(|v| (v.venue, v.book))
                .collect::<HashMap<Venue, _>>();
            let data = Arc::new(BookData {
                market,
                venues: map,
                consolidated,
                stats,
                ts,
            });
            sig.books.update(|b| {
                b.0.insert(market, data);
            });
        }
        WireEvent::Ticker {
            market,
            mid,
            spread_bps,
            imbalance,
            ts,
        } => {
            sig.tickers.update(|t| {
                let dir = t.get(&market).map(|p| {
                    let prev: f64 = p.mid.parse().unwrap_or(0.0);
                    let now: f64 = mid.parse().unwrap_or(0.0);
                    if prev != 0.0 && now != prev {
                        if now > prev {
                            1
                        } else {
                            -1
                        }
                    } else {
                        0
                    }
                });
                t.insert(
                    market,
                    TickerData {
                        mid,
                        spread_bps,
                        imbalance,
                        dir: dir.unwrap_or(0),
                        ts,
                    },
                );
            });
        }
        WireEvent::Trades { market, trades } => {
            if trades.is_empty() {
                return;
            }
            sig.tapes.update(|tapes| {
                let tape = tapes.entry(market).or_default();
                // Prepend newest-first.
                for t in trades.into_iter().rev() {
                    tape.insert(0, t);
                }
                tape.truncate(TAPE_ROWS);
            });
        }
        WireEvent::History { market, samples } => {
            if samples.is_empty() {
                return;
            }
            sig.histories.update(|h| {
                let v = h.entry(market).or_default();
                let append = v.last().map(|l| samples[0].t > l.t).unwrap_or(true);
                if append {
                    for s in samples {
                        if v.last().map(|l| l.t < s.t).unwrap_or(true) {
                            v.push(s);
                        }
                    }
                    if v.len() > ob_core::SAMPLE_WINDOW {
                        let excess = v.len() - ob_core::SAMPLE_WINDOW;
                        v.drain(0..excess);
                    }
                } else {
                    // Full-window refresh (connect / market switch).
                    *v = samples;
                }
            });
        }
        WireEvent::AlertSet { alerts } => {
            sig.alerts.set(alerts);
        }
        WireEvent::AlertFired { alert } => {
            let title = "Alert triggered".to_string();
            let body = format!(
                "{} crossed {} {} — mid {}",
                alert.market.label(),
                alert.dir.label(),
                alert.price,
                alert.triggered_px.as_deref().unwrap_or("?"),
            );
            sig.alerts.update(|list| {
                if let Some(a) = list.iter_mut().find(|a| a.id == alert.id) {
                    *a = alert.clone();
                } else {
                    list.push(alert.clone());
                }
            });
            sig.push_toast(ToastKind::Warning, title, body);
        }
        WireEvent::ArbSnapshot {
            config,
            stats,
            opportunities,
            fills,
        } => {
            sig.arb.update(|a| {
                a.config = config;
                a.stats = stats.clone();
                a.opportunities = opportunities;
                a.fills = fills;
                a.equity = stats.equity;
            });
        }
        WireEvent::ArbUpdate {
            opportunities,
            stats,
        } => {
            sig.arb.update(|a| {
                a.opportunities = opportunities;
                a.stats = stats.clone();
                if let Some(pt) = stats.equity.into_iter().last() {
                    if a.equity.last().map(|l| l.t < pt.t).unwrap_or(true) {
                        a.equity.push(pt);
                        if a.equity.len() > ARB_EQUITY_WINDOW {
                            let excess = a.equity.len() - ARB_EQUITY_WINDOW;
                            a.equity.drain(0..excess);
                        }
                    }
                }
            });
        }
        WireEvent::ArbFillEvent { fill } => {
            let (title, kind) = if fill.status == "filled" {
                (
                    format!("Arb fill #{} — {} net bps", fill.id, fill.net_bps),
                    ToastKind::Success,
                )
            } else {
                (
                    format!("Arb expired #{} — edge vanished in latency window", fill.id),
                    ToastKind::Info,
                )
            };
            let body = format!(
                "{}: buy {} @ {} → sell {} @ {} · {} {} · P&L ${}",
                fill.market.short(),
                fill.buy_venue.short(),
                fill.buy_px,
                fill.sell_venue.short(),
                fill.sell_px,
                fill.size,
                fill.market.short(),
                fill.pnl_usd,
            );
            sig.arb.update(|a| {
                a.fills.push(fill.clone());
                if a.fills.len() > FILL_ROWS {
                    let excess = a.fills.len() - FILL_ROWS;
                    a.fills.drain(0..excess);
                }
            });
            sig.push_toast(kind, title, body);
        }
        WireEvent::Status { venues, usdt, .. } => {
            sig.venues.update(|v| {
                v.map = venues.into_iter().collect();
                v.usdt = usdt;
            });
        }
    }
}

/// Client clock (used for the equity curve fallback stamp).
#[allow(dead_code)]
fn client_now() -> u64 {
    now_ms()
}
