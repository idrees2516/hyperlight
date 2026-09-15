//! HyperLight — WASM entry point.

mod alerts;
mod app;
mod arb;
mod chart;
mod components;
mod ladder;
mod model;
mod tape;
mod ws;

fn main() {
    console_error_panic_hook::set_once();
    leptos::mount::mount_to_body(app::App);
}

/// Forward leptos panic messages to the browser console for easier debugging.
mod console_error_panic_hook {
    use wasm_bindgen::prelude::*;

    #[wasm_bindgen]
    extern "C" {
        #[wasm_bindgen(js_namespace = console)]
        pub fn error(msg: String);
    }

    pub fn set_once() {
        std::panic::set_hook(Box::new(|info| {
            error(info.to_string());
        }));
    }
}
