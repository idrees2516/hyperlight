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
├── server/   # Axum backend: venue connectors, registry, 10 Hz coalescing publisher
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
curl "localhost:3000/api/book?market=eth"
```

Requires: a recent Rust toolchain (rustup), `wasm32-unknown-unknown` target, `trunk`
(`cargo install trunk --locked`), and Node (only for the Tailwind CLI).

## What you see

| Panel | Description |
|---|---|
| **Stats strip** | Cross-venue mid price (with tick direction), best bid/ask across venues with venue attribution, depth imbalance meter over the ±0.5% band |
| **Ladder** | 22 levels/side with cumulative depth bars; asks (rose) on top, bids (emerald) below, spread strip with bps; **CROSSED** badge when the cross-venue book is locked (arbitrage condition) |
| **Venue tabs** | *Consolidated* (merged book, venue-tagged levels: `HL`, `LT`, `HL+LT` for shared price points), *Hyperliquid* (with per-level aggregated order counts), *Lighter* |
| **Venue cards** | Per-venue best bid/ask, size, message rate, full-depth level count |
| **Header** | Market selector (ETH / BTC / SOL — the markets listed on both venues), live feed badges for both venues and the backend link |

## Architecture

```
Hyperliquid wss            Lighter wss (mainnet.zklighter.elliot.ai/stream)
      │ l2Book snapshots          │ subscribed/order_book snapshot
      │ (20 levels/side,          │ + update/order_book deltas (~50ms batches,
      │  full book each push)     │  full depth 800–1700 levels/side)
      ▼                           ▼
┌─────────────────────────────────────────────────────────────┐
│  ob-server (Axum)                                            │
│  ├── connectors: parse → Decimal → VenueState (BTreeMap      │
│  │   keyed by 12dp integer ticks; Lighter upserts, HL        │
│  │   replaces) with watchdogs + exponential-backoff reconnect│
│  ├── registry: per-market books + feed telemetry (EMA rate)  │
│  ├── publisher: coalesces deltas, caps browser rate at 10 Hz │
│  │   per market, builds stats + consolidated ladder server-side│
│  └── HTTP+WS: /ws (broadcast), /api/health, /api/book, /api/markets │
└─────────────────────────────────────────────────────────────┘
      │ WireEvent JSON (40 levels/side/venue, string decimals)
      ▼
┌─────────────────────────────────────────────────────────────┐
│  ob-ui (Leptos 0.8, compiled to WASM)                        │
│  ├── ws.rs: auto-reconnecting browser WebSocket client        │
│  ├── signals: books per market, tab, link status               │
│  ├── keyed For-rows: only changed levels re-render            │
│  └── components.rs: shadcn/ui set (Card, Badge, Button,        │
│      Separator, Skeleton, StatTile) on Tailwind v4 tokens     │
└─────────────────────────────────────────────────────────────┘
```

## Protocol notes (live-verified)

**Hyperliquid** (`wss://api.hyperliquid.xyz/ws`)
- Subscribe `{"method":"subscribe","subscription":{"type":"l2Book","coin":"ETH"}}`
- Pushes `{"channel":"l2Book","data":{"coin","time","levels":[bids,asks]}}` —
  `levels` is a 2-tuple of arrays; each level is `{"px","sz","n"}` (string decimals)
- Each push is a **full snapshot** (20 levels/side observed on mainnet)
- Heartbeat: client sends `{"method":"ping"}` → `{"channel":"pong"}`

**Lighter** (`wss://mainnet.zklighter.elliot.ai/stream`)
- Server sends `{"type":"connected"}` first; only then subscribe
  `{"type":"subscribe","channel":"order_book/{market_index}"}`
- Mainnet indices (verified): **0 = ETH-USD, 1 = BTC-USD, 2 = SOL-USD**
- `subscribed/order_book` → full snapshot; `update/order_book` → incremental
  deltas with upsert-by-price semantics, `size == 0` removes the level
- Levels are `{"price","size"}` string decimals; **no order count** is provided
- Server sends `{"type":"ping"}` → reply `{"type":"pong"}`

## Engineering decisions

- **Exact decimals, fixed tick scale.** All levels are keyed as `Decimal` mantissas at a
  fixed 12-decimal scale inside `BTreeMap`s — lossless for real venue data, O(log n)
  best-bid/ask, and no float error ever reaches the UI.
- **Snapshot-vs-delta symmetry.** Hyperliquid's snapshot feed and Lighter's delta feed
  both land in the same `VenueState`; the consolidated book, spread/imbalance and depth
  stats are computed once server-side and shipped as display-ready strings.
- **Coalescing publisher.** Lighter batches ~20–60 msgs/s per market; the backend marks
  markets dirty and publishes at most 10 snapshots/s per market, so browser DOM work is
  bounded regardless of venue message rates.
- **Keyed row diffing.** Ladder rows are keyed by full content (price, size, cumulative,
  depth %), so Leptos only patches rows that actually changed and moves the rest —
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

- `crates/core/src/lib.rs` — normalization, `VenueState`, `consolidate`, `compute_stats`
- `crates/server/src/hyperliquid.rs` / `lighter.rs` — venue connectors
- `crates/server/src/state.rs` — registry + 10 Hz coalescing publisher
- `crates/server/src/routes.rs` — `/ws`, `/api/health`, `/api/book`, static serving
- `crates/ui/src/ladder.rs` — ladder + depth bars + spread strip
- `crates/ui/src/components.rs` — shadcn/ui component set for Leptos
- `crates/ui/style.css` — Tailwind v4 theme with shadcn zinc dark tokens
