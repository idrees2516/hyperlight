# HyperLight — Deployment & Operations Guide

HyperLight is a **single self-contained binary + static asset bundle**. Its only
runtime requirement is **outbound WebSocket (wss, port 443) access to nine
crypto exchanges**. No database, no cache, no message broker, no API keys.

```
┌────────────┐     outbound wss × 9     ┌──────────────────────────┐
│ HyperLight │ ───────────────────────► │ Hyperliquid  Lighter     │
│  :3000     │                          │ Binance Bybit OKX Kraken│
│            │                          │ Coinbase Bitstamp Gate  │
└────────────┘                          └──────────────────────────┘
     ▲ ▲
     │ └── /metrics        (Prometheus scrape)
     └──── /ws  /api/*     (browsers, dashboards, bots)
```

## 1. One-command options

### Docker (any host)

```bash
docker build -t hyperlight .
docker run -d --name hyperlight -p 3000:3000 \
  -e HYPAR_MAX_NOTIONAL_USD=5000 \
  hyperlight
```

The [Dockerfile](../Dockerfile) is a multi-stage build (Rust + trunk +
tailwindcss), final image ~120 MB, non-root user, healthcheck-friendly.
Build requirements: 2 GB RAM, ~10 min cold build.

### Render (easiest managed option)

1. Push this repo to GitHub.
2. Dashboard → **New → Blueprint** → pick the repo — [render.yaml](../render.yaml)
   is detected automatically (health-checked on `/api/health`, auto-deploy on push).

### Railway

Connect the repo — [railway.json](../railway.json) selects the Dockerfile path
with `/api/health` as the healthcheck.

### Fly.io

```bash
fly launch --no-deploy     # detects fly.toml
fly deploy
```

[fly.toml](../fly.toml) pins the `sin` region — physically close to
Binance/Bybit/OKX matching engines, which matters for the latency-simulation
realism — and keeps one machine always on (the 9 feeds are stateful long-lived
sockets).

### Vercel (frontend only)

Vercel is serverless and **cannot host** the long-lived WebSocket backend. What
works: host the static `dist/` bundle on Vercel ([vercel.json](../vercel.json)
included) and rewrite `/ws` + `/api` to a Render/Fly/Railway backend, or change
the backend URL in one line (`crates/ui/src/ws.rs`) and rebuild.

## 2. Runtime configuration

| Variable | Default | Effect |
|---|---|---|
| `PORT` | `3000` | HTTP/WS listen port |
| `HYPAR_DISARMED` | `0` | `1`/`true` → boot with the paper executor disarmed (track-only) |
| `HYPAR_MIN_EDGE_BPS` | `3` | listing threshold, bps after fees |
| `HYPAR_FIRE_EDGE_BPS` | `8` | firing threshold (the EV gate applies on top) |
| `HYPAR_MAX_NOTIONAL_USD` | `10000` | capital cap per simulated fill |
| `HYPAR_LATENCY_MS` | `250` | simulated round-trip execution latency |
| `HYPAR_COOLDOWN_MS` | `3000` | per-route cooldown between fires |
| `RUST_LOG` | `info` | `tracing` filter (`debug` for connector chatter) |

The executor boots **armed** by default: it automatically takes every
positive-EV opportunity the engines detect. Everything above is also
live-editable at runtime — via the UI's Engine Configuration form, a WS
`arb_config` frame, or `PUT /api/arb/config` — with no restart.

Example: a conservative watcher deployment:

```bash
HYPAR_DISARMED=1 RUST_LOG=warn ./hyperlight-server
```

An aggressive sim deployment:

```bash
HYPAR_FIRE_EDGE_BPS=4 HYPAR_LATENCY_MS=120 HYPAR_COOLDOWN_MS=1500 \
HYPAR_MAX_NOTIONAL_USD=25000 ./hyperlight-server
```

## 3. Monitoring

### Health probe

```bash
curl -s localhost:3000/api/health | jq '{
  uptime_ms, usdt_usd,
  arb, cycles, sweep, ga,
  venues: [.venues[] | {venue, status, msgs_per_sec}]
}'
```

Every venue reports `status` (connecting/live/reconnecting/error), an EMA
message rate, total messages and last-message age. Engine sections report
open opportunities, fills, expirations and paper P&L.

### Prometheus

`GET /metrics` exposes (text format 0.0.4):

| Family | Type | Labels |
|---|---|---|
| `hyperlight_uptime_seconds` | gauge | — |
| `hyperlight_venue_up` | gauge | `venue` |
| `hyperlight_venue_msg_per_sec` | gauge | `venue` |
| `hyperlight_usdt_usd` | gauge | — |
| `hyperlight_arb_fills_total` / `hyperlight_arb_expired_total` | counter | — |
| `hyperlight_arb_pnl_usd` | gauge | — |
| `hyperlight_cycles_fills_total` / `hyperlight_cycles_pnl_usd` | counter/gauge | — |
| `hyperlight_sweep_fills_total` / `hyperlight_sweep_pnl_usd` | counter/gauge | — |
| `hyperlight_ga_generation` | counter | — |

Suggested scrape interval: 15 s. Useful alerts:

```yaml
- alert: HyperLightVenueDown
  expr: min(hyperlight_venue_up) < 1 for 2m
- alert: HyperLightFeedStalled
  expr: min(hyperlight_venue_msg_per_sec) < 0.5 for 5m
- alert: HyperLightDown
  expr: up{job="hyperlight"} < 1 for 1m
```

### Logs

Structured `tracing` output on stdout. Notable lines:

```
INFO [sweep] firing ETH optimal plan: 3 legs, 2.31 bps net, 2.31 USD profit, exec in 250ms
INFO [sweep] fill #36 ETH: pnl 2.31 USD (2.31 bps net, 3 venues)
INFO [arb] fill #581 ETH-USD buy Lighter @ 2690.25 -> sell Bybit @ 2690.43: pnl 1.34 USD
INFO [cycles] firing usd>sol@bs>sol@by>eth@ok>eth@kr>usd: 4 legs, net 1.33 bps
INFO [ga] applied generation genome: fire 6.2 bps, latency 180 ms, cooldown 2.1 s
```

## 4. Lifecycle

- **SIGTERM and SIGINT** are both handled: in-flight WebSocket writes drain,
  then the process exits 0. Orchestrator rolling deploys are safe.
- Feeds reconnect internally with exponential backoff — a transient network
  blip does not restart the process.
- State (books, engine stats) is in-memory by design: a restart is a cold
  start with a fresh ~10 s convergence window.

## 5. Resource sizing

| Metric | Value |
|---|---|
| RSS | ~80 MB |
| CPU | 1 core is plenty (2 Hz scans are µs-scale; the 10 Hz book publisher dominates) |
| Network in | ~10–40 msg/s per venue feed (small JSON frames) |
| Network out | per-browser, bounded by the coalescing publisher |
| Disk | none (logs to stdout only) |

A shared 512 MB / 0.5 CPU container runs comfortably.

## 6. Production checklist

- [ ] Container host with **outbound wss** allowed (some serverless platforms
      block long-lived sockets — that kills all 9 feeds).
- [ ] `/api/health` wired to the platform healthcheck.
- [ ] `/metrics` scraped by Prometheus (or equivalent).
- [ ] Health alerts for venue-down and feed-stalled (above).
- [ ] `HYPAR_*` tuned per deployment; fees verified against your actual
      venue fee tiers (they are live-editable — no rebuild).
- [ ] Region chosen close to the venues that matter most to you (`sin` and
      `hkg` minimize latency to Binance/Bybit/OKX/Gate).
- [ ] If you plan to act on the engines' signals: remember fills are **paper**
      — see [ALGORITHMS.md §7](ALGORITHMS.md) for exactly what is and is not
      simulated.
