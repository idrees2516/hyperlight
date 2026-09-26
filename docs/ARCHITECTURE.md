# HyperLight — System Architecture

This document describes the internal architecture of HyperLight: a real-time,
nine-venue orderbook terminal and four-engine arbitrage system written entirely in
Rust (native backend + WebAssembly frontend).

> Companion documents: [ALGORITHMS.md](ALGORITHMS.md) for the mathematical
> derivations, [DEPLOYMENT.md](DEPLOYMENT.md) for operations, [API.md](API.md)
> for the REST/WS interface reference.

## 1. Top-level shape

```
┌──────────────────────────────────────────────────────────────────────────┐
│  venue websockets (9)                                                    │
│  Hyperliquid  Lighter   Binance  Bybit   OKX    Kraken   Coinbase        │
│  Bitstamp     Gate                                                     │
└────────────┬─────────────────────────────────────────────────────────────┘
             │  parse → rust_decimal::Decimal → VenueState
             ▼
┌──────────────────────────────────────────────────────────────────────────┐
│  ob-server  (single process, Axum + tokio multi-thread runtime)          │
│                                                                          │
│  Registry (shared state, std::sync::Mutex, lock discipline)              │
│  ├── books: HashMap<Market, MarketBook{ venues: Vec<Option<VenueState>> }>│
│  ├── usdt_book: VenueState        (Kraken USDT/USD, the FX hub)          │
│  ├── feeds: Vec<VenueFeed>        (EMA message-rate telemetry)           │
│  ├── tapes / history / alerts / selections                              │
│  └── engines: arb (2-leg) · sweep (global) · cycles (multi-hop) · ga     │
│                                                                          │
│  publish_loop (the single heartbeat task)                                │
│  ├── 10 Hz  dirty books → Book frames (selected markets)                 │
│  │          trades batches, alert checks, 2-leg arb scan                 │
│  ├── 2 Hz   tickers, arb/sweep/cycle scans + updates,                    │
│  │         (every 4th) Status heartbeat + GA update                      │
│  └── 5 s    depth sampler → 60-minute history ring                       │
│                                                                          │
│  HTTP + WS router                                                         │
│  ├── /ws           per-socket market routing, initial burst              │
│  ├── /api/*        health, markets, book, tape, history, alerts,          │
│  │                arb(+config), sweep, cycles, ga                        │
│  └── /metrics      Prometheus exposition (12 metric families)            │
└────────────┬─────────────────────────────────────────────────────────────┘
             │  WireEvent JSON, pre-serialized ONCE, routed by (kind, market)
             ▼
┌──────────────────────────────────────────────────────────────────────────┐
│  ob-ui  (Leptos 0.8 CSR, compiled to WASM by trunk)                      │
│  auto-reconnecting WS client → signals → keyed For-rows → DOM            │
└──────────────────────────────────────────────────────────────────────────┘
```

## 2. Workspace layout

| Crate | Compiles to | Responsibility |
|---|---|---|
| `ob-core` | native **and** wasm32 | Shared, I/O-free domain types: venues, markets, `VenueState` book container, aggregation, `arb_walk`, the sweep optimizer, the swap-graph cycle engine, all wire types. Only `serde`, `serde_json`, `rust_decimal` dependencies. |
| `ob-server` | native (x86_64/aarch64) | Axum backend: the nine venue connectors, the registry, the coalescing publisher, four arbitrage engines, alert engine, depth sampler, REST/WS/metrics endpoints. |
| `ob-ui` | wasm32-unknown-unknown | Leptos CSR frontend. Tailwind v4 + a hand-implemented shadcn/ui (New York, zinc dark) component set. |

The hard rule that keeps this split clean: **`ob-core` never performs I/O**. It is
compiled into the browser, so it must stay free of tokio, sockets and filesystem
access. Every algorithm that needs to run on both sides (walks, aggregation,
graph math) lives there; everything that touches a socket lives in `ob-server`.

## 3. Data model — exact-decimal bookkeeping

### 3.1 `VenueState`

Each (venue, market) pair owns one `VenueState`:

```rust
pub struct VenueState {
    bids: BTreeMap<i128, (Decimal, Option<u32>)>,  // price-tick → (size, order count)
    asks: BTreeMap<i128, (Decimal, Option<u32>)>,
}
```

- **Price keys are integer ticks.** Every price is rescaled to a fixed 12-decimal
  scale and stored as its `i128` mantissa (`tick_key`). Twelve decimals is
  lossless for every real venue feed (max ~6 dp) and makes cross-venue ordering
  an exact integer comparison inside a `BTreeMap` — O(log n) best bid/ask,
  ordered iteration, and zero floating-point error in the price path.
- Prices are stored **as the venue quotes them** (USDT venues keep native USDT
  ticks). Conversion to USD happens only at aggregation/algorithm boundaries via
  a per-venue `factor`.
- Two ingestion methods cover every venue protocol in existence:
  - `replace(bids, asks)` — full snapshot (Hyperliquid, Binance, OKX, Bitstamp,
    Gate, Coinbase);
  - `apply_side(side, deltas)` — upsert-by-price, size 0 removes (Lighter,
    Bybit, Kraken, Coinbase touch updates).

### 3.2 USD normalization

Four venues quote ETH/SOL in **USDT**; five in **USD**. HyperLight converts
through the **live Kraken USDT/USD book mid** (updated on every Kraken book
message, and used as the `factor` for USDT venues). This is deliberately the
*executable* market rate rather than a fixed constant: ignoring quote basis
injects a phantom 2–3 bps "edge" into every USDT route, which the engines would
then happily trade.

### 3.3 Consolidated book & stats

`consolidate(&[Sourced])` merges any number of venue books into one
USD-normalized ladder keyed on 4-decimal ticks (shared levels merge; each level
carries a `u8` venue bitmask so the UI can show venue chips).
`compute_stats` produces best bid/ask with venue attribution, mid, spread bps,
near-touch notionals (±0.5% band) and imbalance — all in exact decimals, all
computed server-side so the browser only renders strings.

## 4. Concurrency model

HyperLight uses **one tokio runtime, one registry, plain mutexes, and a strict
lock discipline** — no async locks, no lock-free tricks, no actor framework.

Lock ordering (violating this is the only way to deadlock the server):

```
books/usdt_book  →  engine inner (arb / sweep / cycles / ga)  →  broadcast
```

- Connectors take `books` briefly to apply one message and mark the market dirty.
- Engine scans take `books`, compute opportunit­ies into stack vectors, release,
  then take their own `inner` to track/fire.
- Paper executors re-take `books` after the simulated latency to re-walk.

The **publish loop is the heartbeat**: a single `tokio::select!` over two
interval ticks (100 ms / 500 ms) that drives book coalescing, trade batching,
alert checks and every engine scan. Engine cadences therefore never drift and
never stampede — the 10 Hz tick cannot overlap itself.

Backpressure & fan-out use one `tokio::sync::broadcast` channel (capacity 4096).
Each event is serialized **once** into a `Broadcast { kind, market, json }`
envelope; every socket routes on `(kind, market)` without re-parsing. A lagged
client simply drops frames (logged) and continues — the next snapshot is
authoritative, so the UI self-heals.

## 5. The four arbitrage engines

All four share one configuration (`ArbConfig`), one latency-survival
calibration (measured by the GA), and one EV gate. Full math in
[ALGORITHMS.md](ALGORITHMS.md).

| Engine | Cadence | Search space | Result |
|---|---|---|---|
| **2-leg** (`arb.rs`) | 10 Hz | every (buy venue, sell venue) pair × ETH/SOL — 72 routes with 9 venues | best executable size for one pair; per-route stats |
| **Global sweep** (`sweep.rs`) | 2 Hz | ALL venues merged into two executable curves | the provably optimal simultaneous multi-venue order plan |
| **Multi-hop cycles** (`cycles.rs`) | 2 Hz | 14-node / ~78-edge swap graph, simple cycles ≤ 5 legs | profitable A→B→C→…→A routes incl. USDT-hub conversions |
| **Genetic optimizer** (`ga.rs`) | 30 s / generation | 7-gene parameter space | evolving engine parameters, hot-applied when they beat live |

Paper execution is uniform across engines: **fire → wait `latency_ms` → re-walk
the live books → fill at the new prices or expire**. This converts the honest
adverse-selection question ("would the edge still exist after my round trip?")
into a measurable outcome — and every outcome feeds the GA's survival
calibration, which feeds the EV gate, which decides the next fire. The system
literally learns its own latency.

## 6. The wire protocol

One JSON frame type (`WireEvent`, serde tag = `"type"`) flows server→browser:

| Frame | Cadence | Payload |
|---|---|---|
| `book` | ≤ 10 Hz, selected market only | per-venue ladders + consolidated book + stats |
| `ticker` | 2 Hz, every market | mid / spread bps / imbalance strings |
| `trades` | batched 10 Hz | new taker trades (deduped by venue trade id) |
| `history` | on select + 5 s appends | 60-min depth-band samples |
| `arb_snapshot` / `arb_update` / `arb_fill_event` | on connect / 2 Hz / event | 2-leg engine state |
| `sweep_snapshot` / `sweep_update` / `sweep_fill_event` | on connect / 2 Hz / event | global sweep plans + fills |
| `cycle_snapshot` / `cycle_update` / `cycle_fill_event` | on connect / 2 Hz / event | multi-hop routes + fills |
| `ga_update` | 0.5 Hz | GA generation, genomes, survival calibration |
| `alert_set` / `alert_fired` | event | server-side alerts |
| `status` | 0.5 Hz | per-venue feed status + USDT/USD |

Client→server commands: `select`, `alert_create`, `alert_delete`, `arb_config`
(partial update, applies to all engines), `arb_reset`, `ga_toggle`, `ga_apply`,
`ga_reset`. See [API.md](API.md) for exact shapes.

### 6.1 Scaling model

Each socket **selects one market**. Full-depth `book` frames flow only to
sockets watching that market; everyone gets compact `ticker`s. Per-client
bandwidth therefore stays roughly constant as the market universe grows, and
the server never serializes a book nobody is viewing.

## 7. Connector engineering

One file per venue (`crates/server/src/{venue}.rs`), all sharing one
native-tls WebSocket connector (`wsio.rs`):

- **Why native-tls?** Bitstamp and Gate sit behind AWS Network Load Balancers
  that reset rustls ClientHellos mid-handshake. OpenSSL handshakes complete
  reliably, so every connector uses one shared `native-tls` connector.
- **Reconnect discipline**: exponential backoff that halves after a
  clean/established session ends (fixes sticky "connecting" states), a 30 s
  watchdog for CloudFront-idle disconnects (Lighter), and Bybit sequence-gap
  detection that forces a resubscribe.
- **Heartbeats**: each venue's idiom (HL JSON ping, Lighter ping→pong, OKX raw
  text `ping`, Gate `spot.ping`, exchange-side pings answered with pongs).
- **Normalization**: every message is parsed to `Decimal` at the connector
  boundary — no string arithmetic anywhere downstream.

## 8. Frontend architecture

- **Leptos 0.8 CSR** with fine-grained signals. The WS client routes each frame
  into typed signals (`books`, `tickers`, `tapes`, `arb`, `sweep`, `cycles`,
  `ga`, `alerts`, `toasts`); the view graph re-renders only what a frame
  actually touched.
- **Keyed rows**: ladder and tape rows use stable keys (venue trade id; full
  row content) so 10 Hz updates patch single rows instead of re-rendering lists.
- **shadcn/ui, owned**: the canonical component set is re-implemented natively
  (`components.rs`) with the exact shadcn Tailwind tokens — matching shadcn's
  copy-paste-into-your-repo philosophy while guaranteeing the WASM bundle
  compiles with zero JS framework dependencies.
- **Engine-fill toasts are throttled** (one per 3 s) so high-frequency
  paper-fill streams never cover the UI in notifications.

## 9. Production posture

- **Observability**: `/api/health` (orchestrator-grade JSON), `/metrics`
  (Prometheus: venue up + msg rates, USDT/USD, per-engine fills/expired/P&L,
  GA generation, uptime), `tracing` structured logs (`RUST_LOG`).
- **Lifecycle**: SIGTERM *and* SIGINT handled; WebSockets drain before exit.
- **Configuration**: `HYPAR_*` environment variables tune the engine at boot
  (see [DEPLOYMENT.md](DEPLOYMENT.md)); everything is also live-editable via
  WS/REST without restarts.
- **Resource shape**: ~80 MB RSS, ~10–40 msg/s per venue feed, one process,
  one port, zero external dependencies beyond outbound WSS to the exchanges.

## 10. Design decisions worth knowing

1. **Paper execution, honestly modeled.** Placing real orders needs API keys,
   nonce signing per venue, and inventory/transfer management — out of scope
   for a market-data terminal. But every number HyperLight shows is computed
   exactly as a real executor would compute it, including fees, depth, USDT
   conversion, and post-latency re-walks.
2. **Exact decimals end-to-end.** f64 appears only in chart samples and GA
   fitness (statistics, not trading state).
3. **One heartbeat task** (the publish loop) drives all engine cadences —
   deterministic, debuggable timing.
4. **The GA is an online system, not a backtest**: it replays a rolling window
   of *live-recorded* opportunities and hot-applies winning parameters to the
   running engines.
5. **The EV gate makes detection and execution agree**: fires happen only when
   `profit × P(survive) > churn floor`, with `P(survive)` measured from the
   engine's own outcomes.
