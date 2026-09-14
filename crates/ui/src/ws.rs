//! Browser WebSocket client with auto-reconnect.
//!
//! Connects to the Axum backend at `/ws`, parses `WireEvent` JSON frames and
//! routes them into Leptos signals.

use crate::model::{Books, BookData, LinkStatus, VenuePair};
use leptos::prelude::*;
use ob_core::WireEvent;
use std::sync::Arc;
use wasm_bindgen::closure::Closure;
use wasm_bindgen::JsCast;
use web_sys::{MessageEvent, WebSocket};

/// All shared reactive state.
#[derive(Clone, Copy)]
pub struct Signals {
    pub books: RwSignal<Books>,
    pub link: RwSignal<LinkStatus>,
    pub venues: RwSignal<VenuePair>,
}

impl Signals {
    pub fn new() -> Self {
        Self {
            books: RwSignal::new(Books::default()),
            link: RwSignal::new(LinkStatus::Connecting),
            venues: RwSignal::new(VenuePair::default()),
        }
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
            schedule_reconnect(sig);
        });
        ws.set_onclose(Some(on_close.as_ref().unchecked_ref()));
        on_close.forget();
    }
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

fn apply_event(sig: Signals, ev: WireEvent) {
    match ev {
        WireEvent::Book {
            market,
            hyperliquid,
            lighter,
            consolidated,
            stats,
            ts,
        } => {
            let data = Arc::new(BookData {
                market,
                hyperliquid,
                lighter,
                consolidated,
                stats,
                ts,
            });
            sig.books.update(|b| {
                match market {
                    ob_core::Market::Eth => b.eth = Some(data),
                    ob_core::Market::Btc => b.btc = Some(data),
                    ob_core::Market::Sol => b.sol = Some(data),
                };
            });
        }
        WireEvent::Status {
            hyperliquid,
            lighter,
            ..
        } => {
            sig.venues.update(|v| {
                v.hyperliquid = Some(hyperliquid);
                v.lighter = Some(lighter);
            });
        }
    }
}
