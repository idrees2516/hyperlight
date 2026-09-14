# HyperLight Terminal

A **real-time, cross-venue orderbook terminal** that streams live L2 market data from
**Hyperliquid** and **Lighter** (zkLighter), consolidates them into a single aggregated
book, and renders it in the browser — **100% Rust, end to end**.

- **Backend**: Rust + Axum + tokio + tokio-tungstenite (rustls) + rust_decimal
- **Frontend**: Rust → WebAssembly via Leptos 0.8 (CSR, signals) + Tailwind CSS v4
- **Design system**: shadcn/ui (New York / zinc dark) — components implemented natively
  in Leptos, following shadcn's copy-paste-into-your-repo distribution philosophy
- **Precision**: every price/size is an exact `rust_decimal::Decimal` from feed to pixel —
  no floating point anywhere in the price path

```
crates/
├── core/     # shared, wasm-safe types + aggregation engine (no I/O deps)
├── server/   # Axum backend: venue connectors, registry, coalescing publisher,
│             # trade tape, depth sampler, alert engine
└── ui/       # Leptos CSR frontend compiled to WASM by trunk
```

## Quick start

```bash
# build everything (backend + tailwind + trunk/WASM)
npm run build          # or: bash scripts/build-all.sh

# serve on :3000
npm run dev            # or: bash scripts/dev.sh

# verify
curl localhost:3000/api/health     # feed status for both venues
curl "localhost:3000/api/book?market=doge"
curl "localhost:3000/api/tape?market=sol"
curl "localhost:3000/api/history?market=eth"
curl -X POST localhost:3000/api/alerts \
  -H 'content-type: application/json' \
  -d '{"market":"sol","dir":"above","price":"200"}'
```

Requires: a recent Rust toolchain (rustup), `wasm32-unknown-unknown` target, `trunk`
(`cargo install trunk --locked`), and Node (only for the Tailwind CLI).

## What you see

| Panel | Description |
|---|---|
| **Stats strip** | Cross-venue mid price (with tick direction), best bid/ask across venues with venue attribution, depth imbalance meter over the ±0.5% band |
| **Ladder** | 22 levels/side with cumulative depth bars; asks (rose) on top, bids (emerald) below, spread strip with bps; **CROSSED** badge when the cross-venue book is locked (arbitrage condition) |
| **Venue tabs** | *Consolidated* (merged book, venue-tagged levels: `HL`, `LT`, `HL+LT` for shared price points), *Hyperliquid* (with per-level aggregated order counts), *Lighter* |
| **Market selector** | Dropdown over **12 markets** — ETH, BTC, SOL, DOGE, 1000PEPE, WIF, WLD, XRP, LINK, AVAX, NEAR, DOT — each showing a live mid + spread ticker (2 Hz) |
| **Trade tape** | Streaming taker-side trades from **both venues** (HL `trades` channel + Lighter `trade/{idx}` channel), newest first with side-colored flash animation, USD notional, venue badge and a buy-pressure meter |
| **Price alerts** | Server-side alert engine checked at 10 Hz against the cross-venue mid; above/below thresholds, quick ±1% fills, triggered log with fire time & price, toast notifications. Managed via WS commands or REST (`/api/alerts`) |
| **Depth history** | 60-minute rolling history sampled every 5 s: bid/ask resting notional within 0.1% / 0.5% / 1% / 2% bands (SVG areas + lines), mid-price overlay on the right axis, selectable 5m/15m/30m/60m windows |
| **Venue cards** | Per-venue best bid/ask, size, message rate, full-depth level count |

## Architecture

```
Hyperliquid wss            Lighter wss (mainnet.zklighter.elliot.ai/stream)
  l2Book + trades            order_book + trade channels (12 markets each)
      │ full snapshots         │ snapshot + ~50ms deltas; trade batches
      ▼                        ▼
┌─────────────────────────────────────────────────────────────┐
│  ob-server (Axum)                                            │
│  ├── connectors: parse → Decimal → VenueState (BTreeMap      │
│  │   keyed by 12dp integer ticks; Lighter upserts, HL       │
│  │   replaces) with watchdogs + exp-backoff reconnects      │
│  ├── registry: books + tape (dedup by venue trade id),      │
│  │   depth history (5 s sampler, 720-sample ring), alerts    │
│  ├── publisher: 10 Hz book snapshots (selected markets),    │
│  │   2 Hz tickers (all markets), 10 Hz trade batches,       │
│  │   10 Hz alert checks, 0.5 Hz heartbeat                     │
│  └── HTTP+WS: per-socket market routing (select), REST APIs │
└─────────────────────────────────────────────────────────────┘
      │ WireEvent JSON (string decimals, pre-serialized once,
      │ routed by (kind, market) without re-parsing)
      ▼
┌─────────────────────────────────────────────────────────────┐
│  ob-ui (Leptos 0.8, compiled to WASM)                        │
│  ├── ws.rs: auto-reconnecting client + select/alert commands │
│  ├── signals: books, tickers, tapes, histories, alerts,      │
│  │   toasts — one reactive graph, no ad-hoc state            │
│  ├── keyed For-rows: only changed levels re-render          │
│  └── components.rs: shadcn/ui set (Card, Badge, Button,      │
│      Input, MarketSelect, Toasts, Separator, Skeleton,       │
│      StatTile, PulseDot) on Tailwind v4 tokens               │
└─────────────────────────────────────────────────────────────┘
```

### Scaling model (why 12 markets stay light)

Every browser socket **selects one market**. The server serializes each event once and
tags it with `(kind, market)`; each socket forwards full-depth `book` frames only for its
selection, plus compact `ticker` frames (2 Hz, every market) and small `trades` batches.
Depth `history` is pushed per-socket for the selected market only (full window on switch,
5 s appends after). So per-client bandwidth stays ~constant as markets grow, and the
server never serializes a full book nobody is looking at.

## Protocol notes (live-verified)

**Hyperliquid** (`wss://api.hyperliquid.xyz/ws`)
- Subscribe `{"method":"subscribe","subscription":{"type":"l2Book","coin":"ETH"}}` and
  `{"method":"subscribe","subscription":{"type":"trades","coin":"ETH"}}`
- Book pushes `{"channel":"l2Book","data":{"coin","time","levels":[bids,asks]}}` —
  `levels` is a 2-tuple of arrays; each level is `{"px","sz","n"}` (string decimals);
  each push is a **full snapshot**
- Trade pushes `{"channel":"trades","data":[{"coin","side":"B|A","px","sz","time","tid"}]}`
  — `side` is the **taker** side, `time` is unix ms, `tid` is a unique id (dedup key)
- Heartbeat: client sends `{"method":"ping"}` → `{"channel":"pong"}`
- 1000PEPE is listed as **`kPEPE`**; perp universe verified via `POST /info {"type":"meta"}`

**Lighter** (`wss://mainnet.zklighter.elliot.ai/stream`)
- Server sends `{"type":"connected"}` first; only then subscribe
  `{"type":"subscribe","channel":"order_book/{idx}"}` and
  `{"type":"subscribe","channel":"trade/{idx}"}`
- Mainnet indices (live-verified via `GET /api/v1/orderBooks`):
  **0 ETH · 1 BTC · 2 SOL · 3 DOGE · 4 1000PEPE · 5 WIF · 6 WLD · 7 XRP · 8 LINK ·
  9 AVAX · 10 NEAR · 11 DOT** (217 active perps total on mainnet)
- `subscribed/order_book` → full snapshot; `update/order_book` → incremental deltas with
  upsert-by-price semantics, `size == 0` removes the level; levels are `{"price","size"}`
  string decimals; **no order count** is provided
- `update/trade` → `{"channel":"trade:{idx}","trades":[Trade],"liquidation_trades":[Trade]}`;
  `Trade = {trade_id, price, size, is_maker_ask, timestamp(ms), ...}`. Taker side:
  `is_maker_ask == true` → the taker **bought** (`"B"`)
- Server sends `{"type":"ping"}` → reply `{"type":"pong"}`
- REST market catalogue: `GET /api/v1/orderBooks`; public trade history:
  `GET /api/v1/recentTrades?market_id={idx}&limit={1..100}` (the sibling
  `/api/v1/trades` endpoint requires account auth)

### Browser ↔ backend socket protocol

Server → client (`WireEvent`): `book` (selected market), `ticker` (all, 2 Hz),
`trades` (batched), `history` (full window on select + 5 s appends), `alert_set`,
`alert_fired`, `status` (heartbeat).

Client → server:
- `{"type":"select","market":"doge"}` — switch this socket's market
- `{"type":"alert_create","market":"sol","dir":"above","price":"123.4"}`
- `{"type":"alert_delete","id":7}`

## Engineering decisions

- **Exact decimals, fixed tick scale.** All levels are keyed as `Decimal` mantissas at a
  fixed 12-decimal scale inside `BTreeMap`s — lossless for real venue data, O(log n)
  best-bid/ask, and no float error ever reaches the UI. (Depth-history samples use f64
  — they feed a chart, not the trading ladder.)
- **Snapshot-vs-delta symmetry.** Hyperliquid's snapshot feed and Lighter's delta feed
  both land in the same `VenueState`; the consolidated book, spread/imbalance and depth
  stats are computed once server-side and shipped as display-ready strings.
- **Coalescing publisher + per-socket routing.** Lighter batches ~20–60 msgs/s per
  market across 12 markets; the backend marks markets dirty and publishes at most 10
  snapshots/s for markets someone is actually viewing, 2 Hz tickers for everything else —
  so browser DOM and network work is bounded regardless of venue message rates.
- **Trade dedup.** Both venues key trades by a venue-unique id (HL `tid`, Lighter
  `trade_id`) in a bounded set, so reconnects and re-subscribes never duplicate tape rows.
- **Keyed row diffing.** Ladder and tape rows are keyed by stable ids (venue trade id for
  the tape, full row content for the ladder), so Leptos only patches what changed —
  smooth 10 Hz updates without full re-renders.
- **Resilience.** Exponential-backoff reconnects, a 30s Lighter watchdog (CloudFront idle
  drops), lazy book creation for out-of-order snapshots, and per-venue health surfaced
  in the UI.
- **Why hand-rolled shadcn for Leptos?** shadcn/ui distributes components as source you
  own, not a runtime library. The canonical components are implemented natively in
  Leptos with the exact shadcn Tailwind tokens (`--color-background`, `border`, `muted`,
  `ring`, zinc-950 base), which guarantees compilation and pixel-level fidelity to the
  shadcn New York dark aesthetic.

## Files of interest

- `crates/core/src/lib.rs` — normalization, `VenueState`, `consolidate`, `compute_stats`,
  wire types (`Trade`, `DepthSample`, `Alert`, `Ticker`)
- `crates/server/src/hyperliquid.rs` / `lighter.rs` — venue connectors (books + trades)
- `crates/server/src/state.rs` — registry, publisher, depth sampler, alert engine
- `crates/server/src/routes.rs` — `/ws` socket protocol, `/api/*` REST, static serving
- `crates/ui/src/ladder.rs` — ladder + depth bars + spread strip
- `crates/ui/src/tape.rs` — trade tape panel
- `crates/ui/src/alerts.rs` — price alerts panel
- `crates/ui/src/chart.rs` — SVG depth-history chart
- `crates/ui/src/components.rs` — shadcn/ui component set for Leptos
- `crates/ui/style.css` — Tailwind v4 theme with shadcn zinc dark tokens
