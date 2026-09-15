# Worklog

---
Task ID: 1
Agent: main (Super Z)
Task: Research Hyperliquid + Lighter APIs and Rust stack for real-time orderbook app

Work Log:
- Tested environment: Rust NOT installed, Node 24 present. Hyperliquid REST/WS reachable.
- Old Lighter endpoints (api.zklighter.elliot.ai) dead; docs.lighter.xyz + apidocs.lighter.xyz alive.
- Live-verified Lighter mainnet WS: wss://mainnet.zklighter.elliot.ai/stream (101 upgrade + connected msg).
- Cloned elliottech/lighter-python SDK; extracted full WS protocol (subscribe/order_book, update semantics, ping/pong).
- Live-tested Lighter WS with Node: market 0=ETH (1192 bids), 1=BTC (1698 bids), 2=SOL; incremental updates confirmed (size 0 = remove).
- Live-tested Hyperliquid WS: wss://api.hyperliquid.xyz/ws; l2Book sub; levels = [bids[],asks[]] arrays of {px,sz,n}; full snapshot per push; ping->pong.
- Installed rustup (Rust 1.98.1), wasm32-unknown-unknown target, trunk 0.21.14 (prebuilt binary).
- Verified crates.io connectivity; latest: leptos 0.8.20, axum 0.8.9, tokio 1.53.1, tokio-tungstenite 0.30, rust_decimal 1.43, tower-http 0.7.1.
- Extracted gateway harness from fullstack tarball: Caddyfile (:81 -> localhost:3000, XTransformPort override), package.json dev script runs on :3000 with tee dev.log.
- Decision: shadcn/ui implemented as owned Leptos components (zinc dark, New York style) per shadcn copy-paste philosophy; guarantees compilability.

Stage Summary:
- Both exchange protocols 100% confirmed with live data. Toolchain ready. Gateway understood (bind 3000).
- Next: scaffold Cargo workspace (crates: core, server, ui).

---
Task ID: 2
Agent: main (Super Z)
Task: Scaffold Cargo workspace + shared core crate

Work Log:
- Created workspace with 3 crates: core (shared wasm-safe types), server (Axum), ui (Leptos).
- ob-core: Market/Venue/FeedStatus enums, Level/VenueBook/BookStats/ConsolidatedBook wire types, VenueState (BTreeMap keyed by 12dp Decimal mantissa ticks), consolidate() cross-venue merge, compute_stats() exact-decimal stats.
- cargo check green for core.
- Key decision: fixed SCALE=12 tick keys — lossless for all real venue data, O(log n) ordering.

Stage Summary:
- Shared data model + aggregation engine complete and compiling.

---
Task ID: 3
Agent: main (Super Z)
Task: Backend crate (Axum, connectors, publisher, routes)

Work Log:
- hyperliquid.rs: connect/subscribe l2Book per coin, full-snapshot replace, ping every 20s, pong responder, backoff reconnect.
- lighter.rs: wait for "connected" then subscribe order_book/{idx} for ETH=0/BTC=1/SOL=2, snapshot replace + incremental apply_side (upsert, size-0 removal), ping->pong, 30s watchdog, backoff reconnect.
- state.rs: Registry with Mutex<HashMap<Market, MarketBook>>, VenueFeed telemetry (EMA msg rate), 10Hz coalescing publish_loop, health_payload, book_json.
- routes.rs: /ws (initial snapshots + broadcast forwarding), /api/health, /api/book?market=, /api/markets, static dist serving with SPA fallback.
- Fixed rustls CryptoProvider panic by adding rustls (ring feature) + install_default() in main.
- Live-verified: HL + LT both live, ~38 msg/s Lighter, full books (ETH 1929 levels, BTC 2789), cross-venue stats correct (incl. crossed markets).

Stage Summary:
- Backend fully working with live production data from both venues.

---
Task ID: 4
Agent: main (Super Z)
Task: Frontend crate (Leptos 0.8 + Tailwind v4 + shadcn-style components)

Work Log:
- model.rs: BookData/Books/TabView/LinkStatus/VenuePair.
- ws.rs: auto-reconnecting browser WS client -> Leptos signals.
- components.rs: shadcn/ui set implemented natively (Card, Badge variants, Button, Separator, Skeleton, PulseDot, StatTile) with zinc dark tokens.
- ladder.rs: build_rows with cumulative depth + pct, keyed For diffing (full-content keys), SpreadStrip with CROSSED badge, skeleton loading state.
- app.rs: Header (market selector, venue badges), StatsStrip (mid+direction, cross-venue best bid/ask with venue attribution, imbalance meter), LadderCard with venue tabs, VenueCards, Footer.
- style.css: Tailwind v4 @theme with shadcn zinc-950 palette, depth-bar transition, thin scrollbars.
- Fixed ~15 compile issues (Memo PartialEq derives on core types, Option<Memo> props, closure children, venue borrow extraction).
- Tailwind v4 CLI via local npm install; trunk release build (NO_COLOR=1 sandbox quirk worked around with NO_COLOR=false; Trunk.toml minimal dist config; CSS copied to dist).

Stage Summary:
- WASM bundle built: 1.28MB wasm + 35KB JS + 24KB CSS. VLM review: A+ "production-quality trading terminal UI".

---
Task ID: 5
Agent: main (Super Z)
Task: Build system + deploy harness

Work Log:
- scripts/build-all.sh, build-ui.sh (tailwind + trunk + cp), dev.sh (restart-loop harness).
- package.json: dev/build/check/lint scripts for the platform harness.
- Caddyfile gateway (:81 -> :3000 with XTransformPort) installed; verified proxying works.
- Fixed Lighter x1 order-count noise: core now stores Option<u32> counts (None for Lighter).

Stage Summary:
- One-command builds; server running on :3000 via nohup; gateway verified.

---
Task ID: 6
Agent: main (Super Z)
Task: End-to-end verification

Work Log:
- Node WS client: 204 book events in 8s (25.5/s), correct consolidated books with venue attribution.
- agent-browser: page loads, no console errors, WS open, both venue feeds LIVE.
- Interactions verified: market switch (ETH->BTC updates all panels), venue tabs (Consolidated/Hyperliquid/Lighter — order counts on HL view, venue tags on consolidated).
- Real-time updates confirmed (mid 2523.845 -> 2523.775 over 4s).
- Responsive: no horizontal overflow at 375px; VLM mobile review: usable, structurally sound.
- VLM desktop review: A+ — professional shadcn zinc design, red asks/green bids with depth bars, no glitches.
- README.md with architecture, protocol notes, engineering decisions; dev.log created.

Stage Summary:
- Fully working, browser-verified real-time cross-venue orderbook. Ready for delivery.

---
Task ID: 7
Agent: main (Super Z)
Task: Add 4 features: more markets (12), trade tape, price alerts, historical depth charts

Work Log:
- Live-probed new APIs: Lighter /api/v1/orderBooks (full market catalogue: 217 perps; 0-11 = ETH,BTC,SOL,DOGE,1000PEPE,WIF,WLD,XRP,LINK,AVAX,NEAR,DOT), /api/v1/recentTrades (public), /api/v1/trades (auth-only — avoided); Lighter WS trade/{idx} channel verified live (12 subs on one session, 206 trades/20s); HL universe via /info meta (kPEPE = 1000PEPE) + HL WS trades frames verified (side/px/sz/time ms/tid).
- ob-core: Market enum 12 markets with HL coins + LT indices + slugs; new wire types Trade, DepthSample (0.1/0.5/1/2% bands), Alert/AlertDir, Ticker; WireEvent += Ticker/Trades/History/AlertSet/AlertFired; VenueState::band_notionals single-pass; cross_mid helper; TAPE_BACKLOG/SAMPLE consts.
- ob-server connectors: HL subs l2Book+trades x12, parses trades frames (dedup via tid); LT subs order_book+trade x12, parses update/trade incl. liquidation_trades, taker side from is_maker_ask.
- state.rs: Broadcast envelope (kind, market, json) — serialize once, per-socket routing without parsing; tape state (backlog + dedup set + pending); 5s depth sampler (720x60min ring, exact-decimal bands); alert engine (create/delete/check at 10Hz, fires on cross-venue mid); selection refcounts; publish_loop: 10Hz books (selected only) + trades + alert checks, 2Hz tickers, 0.5Hz heartbeat; history_loop task.
- routes.rs: per-socket protocol — initial burst (tickers, book, tape backlog, history, alerts), select/alert_create/alert_delete client commands, 5s history appends; REST: /api/markets (12), /api/alerts GET/POST, /api/alerts/{id} DELETE, /api/history, /api/tape.
- ob-ui: model.rs (Books map, TickerData, Tapes, Histories, Toasts, fmt_hms via js_sys); ws.rs (all events, client commands, toast system with auto-dismiss); components.rs += Input, MarketSelect (dropdown over live tickers), Toasts; new tape.rs (side-colored flash rows, buy-pressure meter), alerts.rs (above/below toggle, quick ±1% fills, active + triggered lists), chart.rs (pure-SVG depth history: band tabs, window tabs, bid/ask areas+lines, mid overlay right axis, $ + time axes); app.rs grid layout (ladder + right rail, chart full width).
- ~25 compile fixes (leptos RSX quirks: `>` in attr exprs breaks parsing, f64 not Hash in For keys, Send bounds on Box<dyn Fn>, Memo PartialEq, borrow order).
- E2E (node): initial burst, select doge -> book+tape+history, 138 trades events across 10 markets in 22s, history appends every 5s, alert create -> alert_fired (doge below 0.0843 @ 0.0838405), REST lifecycle 201/200, /api/tape 120 trades, /api/markets 12.
- Browser (agent-browser): zero console errors; market switch ETH->DOGE/SOL; alert set via UI; 60 tape rows; 5 chart paths.
- VLM review round 1: B- (dropdown layering, faint area fills) -> fixed (dim backdrop bg-black/50, border-zinc-700 popup, area fills /15 -> /30, stronger alert input border). VLM round 2: A (chart pass, panels pass).
- Note: sandbox kills bg processes between tool sessions; server+tests must run in one bash invocation (scripts/test_browser*.sh do this). Platform dev harness (npm run dev) keeps it alive for preview.

Stage Summary:
- 12 live markets, cross-venue trade tape, server-side price alerts with toasts, 60-min depth history chart. Full cargo check green (native + wasm), release build green, E2E + browser + VLM A-grade verified.

---
Task ID: 8
Agent: main (Super Z)
Task: Add all ETH/SOL CLOBs (9 venues total) + advanced cross-venue arbitrage engine

Work Log:
- Live-probed 7 candidate CLOB WS APIs (35s each): Binance combined depth20+trades, Bybit v5 orderbook.50+publicTrade, OKX books5+trades, Kraken v2 book(100)+trade (ETH/SOL/USDT-USD), Coinbase ticker+matches (level2 now auth-gated -> touch-level book), Bitstamp order_book+live_trades, Gate spot.order_book(20,100ms)+spot.trades. All message shapes captured.
- Sandbox reset wiped Rust toolchain + trunk: reinstalled rustup stable 1.98.1 + wasm32 target + trunk 0.21.14 prebuilt.
- TLS issue: Bitstamp + Gate (AWS NLB) reset rustls ClientHellos (openssl handshake fine, Node fine). Fixed by switching ALL connectors to a shared native-tls connector helper (crates/server/src/wsio.rs) + workspace feature change rustls->native-tls.
- ob-core: Venue enum 9 venues (index/short/bit/quote/default_taker_fee_bps/supports), VENUES const; books generalized to Vec<Option<VenueState>> per market with Sourced<'a> USD-normalized views; consolidate() venue-bitmask levels; compute_stats + venue attribution (bb_v/ba_v); arb_walk() exact marginal depth-walking (pay ask*(1+fee) < recv bid*(1-fee), capped by max_notional); arb wire types (ArbConfig/ArbOpportunity/ArbFill/ArbStats/ArbPairStat/EquityPt/ArbConfigUpdate) + consts.
- ob-server: state.rs N-venue refactor (feeds vec, venue_factor via live USDT/USD from Kraken USDT/USD book mid, set_usdt_rate marks USDT books dirty); 7 new connectors (binance/bybit/okx/kraken/coinbase/bitstamp/gate.rs); hyperliquid/lighter moved to replace_venue/apply_venue_side; arb.rs engine: 10Hz scan over ETH/SOL x venue-pairs, dust guard $100, tracking w/ 2.5s TTL, fire threshold + cooldown + one in-flight per route, latency-modeled paper execution (re-walks CURRENT books after latency_ms; expired if edge vanished), full stats/equity/pair aggregation, live config via WS + REST.
- routes.rs: ArbSnapshot in initial burst; client cmds arb_config/arb_reset; REST /api/arb + /api/arb/config GET/PUT.
- ob-ui: model.rs BookData.venues map + ArbState + fmt_num; ws.rs arb events + commands; ladder.rs venue-mask chips; app.rs 9 venue badges + usdt badge + venue tabs + venue card grid; new arb.rs panel (6 stat tiles, opportunities table w/ venue chips, SVG equity curve, execution log w/ filled/expired, config form w/ 9 fee inputs, arm/pause/reset); tape.rs venue chips + trimmed numbers.
- 3 connector data bugs found+fixed via live verification: OKX arg instId rename; OKX books5 levels are 4-element arrays (Vec<Vec<String>> arity-tolerant parse); Gate book result uses bids/asks keys (b/a aliases).
- Connector backoff recovery: halve on established-connection end (fixes sticky "connecting").
- E2E (scripts/test_e2e_arb.mjs): PASS venues=9 (all live books), 465 paper fills in zero-fee sim mode, 17 latency-expirations after realistic-fee restore, USDT/USD live 0.9996, REST arb endpoints 200.
- Browser (scripts/test_browser_arb.sh): zero console errors, 15 live badges, 60 tape rows, SVG paths render.
- VLM round 1: B+ (BID SZ clipping, OKX connecting, Gate empty, raw decimals, tape row clip, waiting... text). All fixed (fmt_num normalization, OKX/Gate connector fixes, tape height multiple, em-dash). VLM round 2: A-/A-/A-/A, defects a-f all verified FIXED.

Stage Summary:
- 9-venue live terminal (Hyperliquid, Lighter, Binance, Bybit, OKX, Kraken, Coinbase, Bitstamp, Gate) + fee-aware, depth-walking, latency-modeled cross-venue arbitrage engine with paper execution, live config and equity curve. cargo check green (native+wasm), release builds green, E2E + browser + VLM A-grade verified.
