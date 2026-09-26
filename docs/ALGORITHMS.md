# HyperLight — Arbitrage Algorithms

This document derives, in full, the four engines that make up HyperLight's
arbitrage stack and the decision policy that binds them together. All money
math runs on exact decimals (`rust_decimal`); the only f64 in the pipeline is
GA fitness statistics.

Throughout:

- `f_v` — the USD conversion factor of venue `v` (1 for USD venues; the live
  Kraken USDT/USD mid for USDT venues).
- `φ_v` — the taker fee of venue `v` as a decimal (10 bps = 0.001).
- `a` — an ask level (price `p_a`, size `s_a`) on some venue.
- `b` — a bid level (price `p_b`, size `s_b`) on some venue.
- All comparisons are in **USD**, i.e. after multiplying by `f_v`.

---

## 1. The common substrate: executable curves

A venue book is two piecewise-linear curves:

- **Asks** = supply we can buy from, strictly increasing in price.
- **Bids** = demand we can sell into, strictly decreasing in price.

Two "effective" prices put fees and quote-conversion inside a single number:

```text
effective unit cost     c = p_a · f_buy  · (1 + φ_buy)     (USD per unit bought)
effective unit revenue  r = p_b · f_sell · (1 − φ_sell)    (USD per unit sold)
```

A unit bought at effective cost `c` and sold at effective revenue `r` nets
exactly `r − c` dollars **after all fees**. Every engine below is a different
way of exploiting pairs with `r > c`.

---

## 2. Engine 1 — the 2-leg depth walk (`arb_walk`)

**Question answered:** for one (buy venue, sell venue) pair, what is the
profit-maximizing executable size?

Walk the buy venue's asks ascending and the sell venue's bids descending
simultaneously. While

```text
p_bid · f_sell · (1 − φ_sell)  >  p_ask · f_buy · (1 + φ_buy)
```

match `q = min(s_ask_remaining, s_bid_remaining)` units (further capped by the
remaining notional budget `(N − cost)/p_ask`). Accumulate `size`, ex-fee `cost`
and `proceeds`; then

```text
fees   = cost·φ_buy + proceeds·φ_sell
profit = proceeds − cost − fees
netbps = 10000 · profit / cost
```

**Optimality.** Both walks are over monotonically worsening levels, so the
sequence of marginal profits is strictly decreasing; stopping at the first
non-positive marginal is exactly the concave-piecewise-linear optimum. The
reported VWAPs, size, notional and profit are *executable*, not touch-price
fantasies — slippage is included by construction.

**Guards.** A $100 dust floor, per-route cooldown, one in-flight fill per
route, and the EV gate (§5).

## 3. Engine 2 — the global sweep optimizer (flagship)

**Question answered:** across ALL venues at once, what is the set of
simultaneous taker orders that maximizes profit?

The pairwise engine constrains the answer to "all size on one buy venue and
one sell venue." The true optimum may split — buy 60% on Bybit, 40% on Gate,
sell everything on Hyperliquid — because the *second* level of venue A's book
can be worse than the *first* level of venue B's book.

### 3.1 Construction

1. Merge every venue's asks into one list keyed by effective cost `c`
   (ascending); merge every venue's bids into one list keyed by effective
   revenue `r` (descending). Each entry keeps its venue, raw ex-fee USD price
   and size. Depth budget: 200 levels per side per venue — far beyond any
   profitable sweep.
2. Cross the curves greedily: with pointers at the cheapest ask and the
   richest bid, while `r > c`, match `q = min(ask_rem, bid_rem, notional_room)`
   units. Each match buys `q` on the ask's venue and sells `q` on the bid's
   venue.

### 3.2 Why this is exactly optimal

Each matched pair contributes `q·(r − c) > 0` of net profit (fees inside `r`
and `c`). Feasibility of any taker execution plan is: for every prefix of
buy-side size, cumulative buy cost and sell proceeds are supported by the
curves. The greedy:

- always matches the globally cheapest ask with the globally richest bid, and
- never leaves a positive pair unmatched — it only stops when the best
  remaining ask/bid pair has `r ≤ c`, in which case **no** pair of remaining
  levels can be positive (both curves are monotone away from the touch).

Formally this is the classic greedy-crossing optimum for linear supply/demand
curves (equivalently: an optimal solution of the transportation LP with
identical goods, where complementary slackness forces cheapest-source /
richest-sink matching). HyperLight's sweep therefore extracts **every
extractable cent**: no simultaneous taker strategy on the same books can
produce more profit under the same notional cap.

### 3.3 Output

Per-(venue, side) coalesced legs — VWAP, size, notional, fee — forming one
atomic `SweepPlan`:

```text
ETH plan:  buy  BY  $6,000 @ 2690.25   (2 levels)
           buy  GT  $4,000 @ 2690.31
           sell HL  $10,000 @ 2690.50
           net 2.3 bps · profit $2.31 · EV $1.70
```

A venue can only ever appear on one side (its own book cannot be
fee-inclusive crossed with itself), which the coalescer verifies implicitly.

## 4. Engine 3 — multi-hop swap cycles

**Question answered:** do profitable routes exist that traverse *more than two
legs* and *more than one asset* — e.g.
`USD → ETH@Kraken → USDT@Binance → SOL@Bybit → USD`?

### 4.1 The swap graph

- **Nodes**: `USD`, `USDT`, and `Base(M)` for each tradable market.
- **Edges**: every venue book contributes two legs — buy (`quote → base`) and
  sell (`base → quote`). The Kraken USDT/USD book contributes the USD↔USDT FX
  legs, so USDT venues form genuine routes through an *explicit, spread-paying*
  conversion rather than a mid-price approximation.
- Live topology: **14 nodes / ~78 edges**.

Edge weights are log-space marginal rates with fees inside:

```text
buy leg:   g = 1 / (ask · (1 + φ))     [base per quote]
sell leg:  g = bid · (1 − φ)           [quote per base]
```

### 4.2 Detection — Bellman–Ford negative cycles

In log space, a cycle is profitable iff `Σ −log(gᵢ) < 0`. A virtual source
with zero-weight edges to every node lets a single Bellman–Ford relaxation
detect whether *any* negative cycle exists anywhere. Since deeper levels are
strictly worse than the touch, a negative cycle at touch rates is a *necessary*
condition for any profitable executable cycle — when the probe is clean (the
overwhelmingly common case), enumeration is skipped entirely.

### 4.3 Enumeration — bounded simple-cycle DFS

Rooted at USD (the numeraire), a budgeted DFS enumerates every simple cycle up
to `MAX_HOPS = 5` legs, with a 20,000 node-expansion budget per scan and the
top 240 candidates evaluated.

### 4.4 Execution — exact marginal cycle walks

`profit(x)` around a cycle as a function of deployed size `x` is concave
piecewise-linear (a product of linear rate functions in log space). The greedy
advances size while the cycle's marginal rate product `P(x) > 1` — exactly the
multi-leg generalization of `arb_walk` — yielding executable per-leg VWAPs,
entry/exit USD and profit.

## 5. The decision policy — EV gate + survival calibration

Detection is not execution: an edge seen at time `t` must survive the round-trip
latency to pay. HyperLight closes this gap with an **empirically calibrated
expected-value gate**.

### 5.1 Survival measurement

Every fired attempt, in every engine, records its outcome into edge buckets
(0–2, 2–4, 4–8, 8–16, 16+ bps of *detected* net edge):

```text
P(survive | edge bucket) = (survived + 1) / (fired + 2)      (Beta(1,1)-smoothed)
```

Buckets with no data fall back to the global rate; a 0.35 prior applies before
any data exists (most latency-window edges vanish).

### 5.2 The gate

All three execution engines fire only when *all* of these hold:

```text
enabled  ∧  net_bps ≥ fire_edge  ∧  cooldown elapsed  ∧  no in-flight
∧  profit_usd · P(survive | net_bps)  >  0.5 bps × deployed_notional
```

The churn floor (0.5 bps of notional) prices a wasted fire: it burns a
cooldown slot that a better route could have used. The same constant is the
GA's churn penalty, so the optimizer and the executors agree on what a fire
costs — the system converges on parameters that maximize *realized*, not
detected, profit.

### 5.3 Why this matters more than any single algorithm

The gate is the compound-interest rule of the whole stack: firing a 10 bps
edge with a 20% survival rate is *worse* than firing a 6 bps edge with a 90%
survival rate, and no amount of smarter path-finding fixes firing at the wrong
times. The gate turns each engine's raw detections into positive-expectancy
decisions using the system's own measured microstructure.

## 6. Engine 4 — the genetic optimizer

**Question answered:** which parameters maximize realized P&L on the *actual*
venue microstructure this deployment faces?

### 6.1 Genome (7 bounded real genes)

| # | Gene | Range |
|---|---|---|
| 1 | `fire_edge_bps` | 2 – 30 |
| 2 | `min_edge_bps` | 0 – fire_edge |
| 3 | `latency_ms` | 50 – 2000 |
| 4 | `cooldown_ms` | 250 – 30,000 |
| 5 | `max_notional` | 500 – 50,000 USD |
| 6 | `eth_w` | 0.05 – 1 |
| 7 | `sol_w` | 0.05 – 1 |

Genes 6–7 are Kelly-flavoured capital weights: the per-market notional cap is
`max_notional · min(2·w_mkt, 1)`, evolved online.

### 6.2 Fitness — replay simulation on live data

The recorder keeps a rolling window (20,000 observations, ~1 Hz per route) of
every opportunity the 2-leg engine sees. `fitness(genome)` replays the window
in time order:

```text
for each observation with net_bps ≥ fire_edge and cooldown elapsed:
    cap     = max_notional · min(2 · w_market, 1)
    scale   = min(cap / notional, 1)
    pnl    += profit · scale · P(survive | net_bps) · latency_factor − 0.00005 · cap
```

where `latency_factor = exp(−max(latency − 250, 0)/1500)` clamped to
[0.2, 1] — longer simulated latencies discount survival exponentially. This is
a *simulator of the engines themselves*, run against real recorded edges with
the real measured survival curve.

### 6.3 Evolution

Steady-state GA, population 48, one generation per 30 s:

- **Selection**: tournament, k = 3.
- **Crossover**: BLX-α (α = 0.5) — uniform sampling in the α-extended
  interval between parent genes, keeping gene correlation structure.
- **Mutation**: per-gene Gaussian, σ adaptive (0.10 normally; 0.25 after 8
  stagnant generations), per-gene hit probability 0.25.
- **Elitism**: top-2 carried unchanged.
- **Diversity**: random immigrants (8% while stagnant), jittered around the
  best genome.
- **RNG**: an embedded PCG32 (Box–Muller for normals) — deterministic,
  dependency-free, reproducible.

### 6.4 Hot-apply

Every 5 generations, if the best genome's fitness beats the *current live
parameters'* fitness on the same observation window, it is applied to the
running engines (2-leg, sweep and cycles all share `ArbConfig`). Manual
Apply/Pause/Reset are exposed in the UI and via `PUT /api/ga`.

## 7. Honest accounting: what "paper P&L" means here

All fills are simulated, but with a discipline most paper traders skip:

1. **Fees** — per-venue taker schedules, live-editable, always inside the math.
2. **Depth** — every size comes from a marginal walk of the real book; slippage
  is included by construction.
3. **Quote basis** — USDT venues converted at the live executable USDT/USD rate.
4. **Latency** — the executor sleeps `latency_ms`, then re-walks the *current*
  books; if the edge vanished the attempt is logged `expired` and counts as a
  loss in the survival calibration. No "I got the price I saw" fantasy.
5. **Cooldowns and in-flight constraints** — exactly as a real single-position
  executor would face them.

Turning this into live execution requires API keys, per-venue nonce signing,
inventory/rebalance logic and withdrawal limits — a different project — but the
*decision layer* (what to fire, when, at what size) is already computed exactly
as a production executor would compute it.
