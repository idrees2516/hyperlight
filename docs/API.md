# HyperLight — API Reference

Base URL: `http://<host>:3000`. All numeric trading values are **exact decimal
strings** (e.g. `"2690.4283"`, `"1.8786"`) — parse them with a decimal type,
not IEEE floats, if you consume them programmatically.

---

## 1. REST endpoints

### `GET /api/health`

Orchestrator-grade liveness + full state summary.

```jsonc
{
  "uptime_ms": 184000,
  "usdt_usd": "0.9996",
  "arb":    { "enabled": true, "open_opportunities": 5, "fills": 581, "expired": 23, "pnl_usd": "612.44" },
  "cycles": { "open_cycles": 12, "fills": 2371, "expired": 40, "pnl_usd": "988.10",
              "negative_cycles": false, "graph_edges": 78 },
  "sweep":  { "open_plans": 2, "fills": 36, "expired": 0, "pnl_usd": "70.03" },
  "ga":     { "enabled": true, "generation": 4 },
  "venues": [ { "venue": "Binance", "status": "live", "msgs_per_sec": 18.4,
                "total_msgs": 31211, "last_msg_age_ms": 61 }, ... ],
  "markets": [ { "market": "ETH", "tape_trades": 120, "history_samples": 43,
                 "viewers": 1, "venues": [ { "venue": "hl", "levels": 1929 }, ... ] } ]
}
```

### `GET /api/markets`

Static catalogue of the 12 markets with venue-specific identifiers.

### `GET /api/book?market=eth|btc|sol|doge|1000pepe|wif|wld|xrp|link|avax|near|dot`

Current full snapshot: per-venue ladders (26 levels/side), consolidated
USD-normalized book with venue bitmasks, and cross-venue stats (best bid/ask
with venue attribution, mid, spread bps, imbalance, near-touch notionals).

### `GET /api/tape?market=sol` · `GET /api/history?market=sol`

Recent trades (oldest first, deduped) · 60-minute depth samples (5 s cadence).

### `GET /api/alerts` · `POST /api/alerts` · `DELETE /api/alerts/{id}`

Server-side price alerts, checked at 10 Hz against the cross-venue mid.

```bash
curl -X POST localhost:3000/api/alerts -H 'content-type: application/json' \
  -d '{"market":"sol","dir":"above","price":"200"}'
```

### `GET /api/arb`

Full 2-leg engine state: config, stats (+ equity series, per-route pair
table), live opportunities (best net edge first), recent fills/expirations.

### `GET /api/arb/config` · `PUT /api/arb/config`

Live engine configuration (GET) and partial update (PUT — any subset of
fields). **Applies to all engines** (2-leg, sweep, cycles) since they share
one `ArbConfig`.

```bash
curl -X PUT localhost:3000/api/arb/config -H 'content-type: application/json' \
  -d '{"fire_edge_bps":"6","latency_ms":150,"fees_bps":[["binance","10"],["bybit","10"]]}'
```

| Field | Type | Meaning |
|---|---|---|
| `enabled` | bool | arm/disarm the paper executor |
| `min_edge_bps` | decimal string | listing threshold |
| `fire_edge_bps` | decimal string | firing threshold (EV gate applies on top) |
| `max_notional_usd` | decimal string | capital cap per fill |
| `latency_ms` | u64 (≤ 10,000) | simulated round-trip latency |
| `cooldown_ms` | u64 (≤ 600,000) | per-route cooldown |
| `fees_bps` | `[[venue, bps], …]` | per-venue taker fees |

### `GET /api/sweep`

Global sweep optimizer state:

```jsonc
{
  "type": "sweep_snapshot",
  "stats": { "fills": 36, "expired": 0, "pnl_usd": "70.03", "best_net_bps": "3.04",
             "avg_net_bps": "2.36", "avg_venues": "2.7", "open_plans": 2, "equity": [...] },
  "plans": [ {
    "market": "eth",
    "legs": [ { "venue": "bybit", "side": "buy",  "px": "2690.25", "size": "2.23",
                "notional": "6000", "fee_usd": "6" },
              { "venue": "gate",  "side": "buy",  "px": "2690.31", "size": "1.49",
                "notional": "4000", "fee_usd": "8" },
              { "venue": "hyperliquid", "side": "sell", "px": "2690.50", "size": "3.72",
                "notional": "10000", "fee_usd": "45" } ],
    "size": "3.72", "notional": "10000", "proceeds": "10002.31",
    "fees_usd": "59", "profit_usd": "2.31", "net_bps": "2.31", "ev_usd": "1.70",
    "ts": 1727000000000
  } ],
  "fills": [ ... ]
}
```

`ev_usd` is the survival-weighted expected value — what the plan is *worth*
after the empirically measured probability that its edge survives the latency
window.

### `GET /api/cycles`

Multi-hop cycle engine state: graph telemetry (nodes/edges/negative-cycle
probe), live cycle opportunities as hop chains, fills/expirations.

### `GET /api/ga` · `PUT /api/ga`

Genetic optimizer state (generation, population, fitness history, best genome,
survival calibration per edge bucket, observation count). PUT body:
`{"enabled":true,"apply":true,"reset":false}` (all optional).

### `GET /metrics`

Prometheus text exposition — see [DEPLOYMENT.md §3](DEPLOYMENT.md).

---

## 2. WebSocket protocol (`/ws`)

One JSON frame per message, both directions.

### 2.1 Server → client (`WireEvent`, tagged by `"type"`)

| Frame | Cadence | Notes |
|---|---|---|
| `book` | ≤ 10 Hz | only for the socket's selected market |
| `ticker` | 2 Hz | every market — mid/spread/imbalance |
| `trades` | batched | new taker trades |
| `history` | on select + every 5 s | depth samples |
| `arb_snapshot` | on connect / config change / reset | full 2-leg state |
| `arb_update` | 2 Hz | live opportunities + scalar stats + equity point |
| `arb_fill_event` | event | paper fill or latency expiry |
| `sweep_snapshot` / `sweep_update` | on connect / 2 Hz | optimal multi-venue plans |
| `sweep_fill_event` | event | sweep fill or expiry |
| `cycle_snapshot` / `cycle_update` / `cycle_fill_event` | on connect / 2 Hz / event | multi-hop routes |
| `ga_update` | 0.5 Hz | optimizer state |
| `alert_set` / `alert_fired` | event | alert lifecycle |
| `status` | 0.5 Hz | per-venue feed status + USDT/USD |

On connect, a socket receives an initial burst: tickers for all 12 markets,
the full book + tape + history for its market (default ETH), alert set, and
snapshots of all four engines.

### 2.2 Client → server commands

```jsonc
{"type":"select","market":"doge"}                      // switch this socket's market
{"type":"alert_create","market":"sol","dir":"above","price":"200"}
{"type":"alert_delete","id":7}
{"type":"arb_config","fire_edge_bps":"6","latency_ms":150,
 "fees_bps":[["binance","10"],["bybit","10"]]}          // partial update, all engines
{"type":"arb_reset"}                                   // reset paper stats (all engines)
{"type":"ga_toggle","enabled":false}
{"type":"ga_apply"}                                    // apply best genome now
{"type":"ga_reset"}
```

### 2.3 Minimal client example

```js
const ws = new WebSocket("ws://localhost:3000/ws");
ws.onmessage = (m) => {
  const ev = JSON.parse(m.data);
  if (ev.type === "sweep_update") {
    for (const p of ev.plans) {
      console.log(`${p.market}: ${p.net_bps} bps, $${p.profit_usd} profit, ` +
        `EV $${p.ev_usd}, legs: ${p.legs.map(l => `${l.side} ${l.venue}`).join(" + ")}`);
    }
  }
};
ws.onopen = () => ws.send(JSON.stringify({ type: "select", market: "sol" }));
```

## 3. Rate / size limits

- Book frames: 26 levels per side per venue, coalesced to at most 10 Hz per
  market, and only for markets with active viewers.
- Engine updates: 2 Hz. Opportunities: top 14 (2-leg) / top 4 plans (sweep) /
  top 12 (cycles). Fill logs: last 40–60 entries.
- There is no auth: the API is read-only market data + paper-trading state.
  Bind to a private interface if you expose it beyond localhost.
