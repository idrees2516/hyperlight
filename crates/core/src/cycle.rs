//! Multi-hop swap arbitrage: graph model, negative-cycle detection and exact
//! cycle execution.
//!
//! The whole exchange universe is modeled as a directed graph:
//! - **Nodes** are assets: `Usd`, `Usdt`, and one per tradable market
//!   (`Base(Market)` — ETH, SOL, BTC, ...).
//! - **Edges** are executable swap legs offered by a venue book:
//!   a book for market `M` quoted in `Q` on venue `V` contributes
//!   `Q -> M` (buy, walking the asks) and `M -> Q` (sell, walking the bids).
//!   The Kraken USDT/USD book contributes `Usd <-> Usdt` legs, so USDT-quoted
//!   venues form genuine multi-hop routes through an *explicit* FX conversion
//!   (paying the real spread) instead of a mid-price factor.
//!
//! Every edge's marginal conversion rate includes the venue taker fee:
//! - buy leg:  `g = 1 / (ask * (1 + fee))`  [base per quote]
//! - sell leg: `g = bid * (1 - fee)`        [quote per base]
//!
//! **Detection — Bellman-Ford negative cycles.** In log space a cycle is
//! profitable iff the sum of `-log(g)` around it is negative. A virtual
//! source with zero-weight edges to every node lets one Bellman-Ford pass
//! detect whether *any* negative cycle exists anywhere in the graph. Since
//! deeper book levels are strictly worse than the touch, a negative cycle at
//! touch rates is a *necessary* condition for any profitable executable
//! cycle — so when Bellman-Ford finds none, the enumeration stage is skipped
//! entirely (the common case).
//!
//! **Enumeration — bounded simple-cycle DFS.** Profitable paper trades are
//! rooted at USD (the numeraire), so a bounded DFS from `Usd` enumerates all
//! simple cycles up to `MAX_HOPS` legs (e.g. `Usd -> ETH@Kraken -> Usdt@Binance
//! -> SOL@Bybit -> Usd`: buy ETH with USD, sell ETH into USDT, buy SOL with
//! USDT, sell SOL back to USD — a genuine cross-market multi-hop swap).
//!
//! **Execution — exact marginal cycle walk.** Books are piecewise-linear, so
//! the profit as a function of entry size x, `profit(x)`, is concave
//! piecewise-linear with slope `P(x) - 1` where `P(x)` is the product of the
//! legs' marginal rates at fill level x. The exact optimum is therefore found
//! greedily: advance x while `P(x) > 1`, stepping to the next level of
//! whichever leg binds first — the multi-leg generalization of `arb_walk`.
//! Reported size, per-leg VWAPs, notional, fees and profit are all executable
//! values, not touch-price fantasies.

use rust_decimal::prelude::ToPrimitive;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::{Market, Venue};

// ---------------------------------------------------------------------------
// Graph model
// ---------------------------------------------------------------------------

/// An asset node of the swap graph.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Asset {
    /// The numeraire (USD).
    Usd,
    /// Tether — reached only through the Kraken USDT/USD book.
    Usdt,
    /// The base asset of a market.
    Base(Market),
}

impl Asset {
    pub fn label(&self) -> String {
        match self {
            Asset::Usd => "USD".into(),
            Asset::Usdt => "USDT".into(),
            Asset::Base(m) => m.short().to_string(),
        }
    }

    /// Stable wire slug.
    pub fn slug(&self) -> String {
        match self {
            Asset::Usd => "usd".into(),
            Asset::Usdt => "usdt".into(),
            Asset::Base(m) => format!("m:{}", serde_json::to_value(m).unwrap_or_default()),
        }
    }
}

/// Direction of a swap leg.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LegDir {
    /// Quote -> base (consume the asks).
    Buy,
    /// Base -> quote (consume the bids).
    Sell,
}

impl LegDir {
    pub fn label(&self) -> &'static str {
        match self {
            LegDir::Buy => "buy",
            LegDir::Sell => "sell",
        }
    }
}

/// One swap leg: (venue, traded asset, direction). The quote side is implied
/// by the venue (USD venues quote USD, USDT venues quote USDT; the Kraken
/// USDT/USD book trades USDT itself).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Leg {
    pub venue: Venue,
    pub asset: Asset,
    pub dir: LegDir,
}

impl Leg {
    /// The quote currency this leg's venue prices the traded asset in.
    fn quote_asset(&self) -> Asset {
        match self.asset {
            // the USDT/USD book trades USDT against USD
            Asset::Usdt => Asset::Usd,
            // market books are quoted in the venue's currency
            Asset::Base(_) => match self.venue.quote() {
                crate::Quote::Usd => Asset::Usd,
                crate::Quote::Usdt => Asset::Usdt,
            },
            Asset::Usd => Asset::Usd, // not a real leg; defensive
        }
    }

    /// Input asset of this leg (direction-aware):
    /// buy consumes the quote, sell consumes the traded asset.
    pub fn from_asset(&self) -> Asset {
        match self.dir {
            LegDir::Buy => self.quote_asset(),
            LegDir::Sell => self.asset,
        }
    }

    /// Output asset of this leg (direction-aware):
    /// buy yields the traded asset, sell yields the quote.
    pub fn to_asset(&self) -> Asset {
        match self.dir {
            LegDir::Buy => self.asset,
            LegDir::Sell => self.quote_asset(),
        }
    }
}

/// Maximum legs in an enumerated cycle (incl. the closing edge back to USD).
pub const MAX_HOPS: usize = 5;

/// Levels captured per leg side for exact evaluation (best-first).
pub type LegLevels = Vec<(Decimal, Decimal)>;

/// The whole live swap graph, captured under the books lock.
pub struct SwapGraph {
    /// Legs, grouped by input asset for the DFS.
    pub out_edges: HashMap<Asset, Vec<Leg>>,
    /// Best-first levels for each leg (asks for Buy, bids for Sell).
    pub levels: HashMap<Leg, LegLevels>,
    /// Node count / edge count for telemetry.
    pub nodes: usize,
    pub edges: usize,
}

impl SwapGraph {
    pub fn new() -> Self {
        Self {
            out_edges: HashMap::new(),
            levels: HashMap::new(),
            nodes: 0,
            edges: 0,
        }
    }

    /// Register one venue book for a market (both directions).
    /// `asks`/`bids` are best-first captured levels (top `depth`).
    pub fn add_market_book(
        &mut self,
        venue: Venue,
        market: Market,
        bids: LegLevels,
        asks: LegLevels,
        depth: usize,
    ) {
        let buy = Leg {
            venue,
            asset: Asset::Base(market),
            dir: LegDir::Buy,
        };
        let sell = Leg {
            venue,
            asset: Asset::Base(market),
            dir: LegDir::Sell,
        };
        self.out_edges.entry(buy.from_asset()).or_default().push(buy);
        self.out_edges.entry(sell.from_asset()).or_default().push(sell);
        self.levels.insert(buy, asks.into_iter().take(depth).collect());
        self.levels.insert(sell, bids.into_iter().take(depth).collect());
    }

    /// Register the Kraken USDT/USD FX book (both directions).
    pub fn add_usdt_book(&mut self, bids: LegLevels, asks: LegLevels, depth: usize) {
        let buy = Leg {
            venue: Venue::Kraken,
            asset: Asset::Usdt,
            dir: LegDir::Buy,
        };
        let sell = Leg {
            venue: Venue::Kraken,
            asset: Asset::Usdt,
            dir: LegDir::Sell,
        };
        self.out_edges.entry(Asset::Usd).or_default().push(buy);
        self.out_edges.entry(Asset::Usdt).or_default().push(sell);
        self.levels.insert(buy, asks.into_iter().take(depth).collect());
        self.levels.insert(sell, bids.into_iter().take(depth).collect());
    }

    pub fn finalize(&mut self) {
        // Drop legs with empty level sets (no data); count nodes.
        let empty: Vec<Leg> = self
            .levels
            .iter()
            .filter(|(_, lv)| lv.is_empty())
            .map(|(l, _)| *l)
            .collect();
        for l in empty {
            self.levels.remove(&l);
            if let Some(v) = self.out_edges.get_mut(&l.from_asset()) {
                v.retain(|x| *x != l);
            }
        }
        for v in self.out_edges.values_mut() {
            v.sort_by_key(|l| (l.asset, l.venue.index(), l.dir.label()));
            v.dedup_by(|a, b| {
                a.asset == b.asset && a.venue == b.venue && a.dir == b.dir
            });
        }
        self.out_edges.retain(|_, v| !v.is_empty());
        let mut nodes = std::collections::HashSet::new();
        nodes.insert(Asset::Usd);
        for (a, legs) in &self.out_edges {
            nodes.insert(*a);
            for l in legs {
                nodes.insert(l.to_asset());
            }
        }
        self.nodes = nodes.len();
        self.edges = self.out_edges.values().map(|v| v.len()).sum();
    }
}

// ---------------------------------------------------------------------------
// Bellman-Ford negative-cycle detection (log space)
// ---------------------------------------------------------------------------

/// Result of the detection pass.
#[derive(Debug, Clone, Copy, Default)]
pub struct GraphProbe {
    pub nodes: usize,
    pub edges: usize,
    /// Whether at least one negative (profitable) cycle exists at touch rates.
    pub negative_cycle: bool,
    /// Product of touch marginal rates of the tightest detected cycle.
    pub best_cycle_rate: f64,
}

/// Touch marginal rate (output per input) of one leg, fees included.
pub fn touch_rate(leg: &Leg, levels: &LegLevels, fee: Decimal) -> Option<f64> {
    let (px, _) = levels.first()?;
    let g: Decimal = match leg.dir {
        LegDir::Buy => Decimal::ONE / (px * (Decimal::ONE + fee)),
        LegDir::Sell => px * (Decimal::ONE - fee),
    };
    g.to_f64()
}

/// Bellman-Ford with a virtual source (zero-weight edges to every node).
/// Returns whether any negative cycle exists and, if so, the best rate
/// product among up to `extract` extracted cycles.
///
/// Weights are `-ln(g)`. Relaxation runs |V| times; edges that still relax on
/// the |V|-th pass are on or reachable from a negative cycle, and the cycle
/// is recovered by walking predecessors until a node repeats.
pub fn probe_negative_cycles(g: &SwapGraph, fees: &HashMap<Venue, Decimal>) -> GraphProbe {
    let mut probe = GraphProbe {
        nodes: g.nodes,
        edges: g.edges,
        negative_cycle: false,
        best_cycle_rate: 1.0,
    };
    if g.nodes == 0 {
        return probe;
    }

    // Index the nodes.
    let mut idx: HashMap<Asset, usize> = HashMap::new();
    let mut nodes: Vec<Asset> = Vec::new();
    for (a, legs) in &g.out_edges {
        if !idx.contains_key(a) {
            idx.insert(*a, nodes.len());
            nodes.push(*a);
        }
        for l in legs {
            let to = l.to_asset();
            if !idx.contains_key(&to) {
                idx.insert(to, nodes.len());
                nodes.push(to);
            }
        }
    }
    let n = nodes.len();

    // Edge list in (from_idx, to_idx, weight, leg) form.
    let mut edges: Vec<(usize, usize, f64, Leg)> = Vec::with_capacity(g.edges);
    for (a, legs) in &g.out_edges {
        let from = idx[a];
        for l in legs {
            let Some(lv) = g.levels.get(l) else { continue };
            let Some(rate) = touch_rate(l, lv, *fees.get(&l.venue).unwrap_or(&Decimal::ZERO))
            else {
                continue;
            };
            if rate <= 0.0 || !rate.is_finite() {
                continue;
            }
            let to = idx[&l.to_asset()];
            edges.push((from, to, -rate.ln(), *l));
        }
    }
    if edges.is_empty() {
        return probe;
    }

    // Virtual source: dist = 0 for all nodes (handled by initializing to 0
    // and relaxing every edge |V| times).
    let mut dist = vec![0f64; n];
    let mut pred: Vec<Option<usize>> = vec![None; n]; // predecessor node
    let mut pred_edge: Vec<Option<usize>> = vec![None; n]; // edge id used

    // |V| rounds; if the last round still relaxes, a negative cycle exists.
    let mut relaxed_last = false;
    for round in 0..n {
        let mut relaxed = false;
        for (ei, (from, to, w, _)) in edges.iter().enumerate() {
            if dist[*from] + w < dist[*to] - 1e-15 {
                dist[*to] = dist[*from] + w;
                pred[*to] = Some(*from);
                pred_edge[*to] = Some(ei);
                relaxed = true;
            }
        }
        if round == n - 1 {
            relaxed_last = relaxed;
        } else if !relaxed {
            break;
        }
    }

    if !relaxed_last {
        return probe;
    }
    probe.negative_cycle = true;

    // One more pass recording the nodes that still relax: every such node is
    // reachable from (or on) a negative cycle.
    let mut changed: Vec<usize> = Vec::new();
    for (ei, (from, to, w, _)) in edges.iter().enumerate() {
        if dist[*from] + w < dist[*to] - 1e-15 {
            dist[*to] = dist[*from] + w;
            pred[*to] = Some(*from);
            pred_edge[*to] = Some(ei);
            changed.push(*to);
        }
    }
    if changed.is_empty() {
        return probe;
    }

    // Walk predecessors n steps from a changed node to land inside a cycle.
    let mut cur = changed[0];
    for _ in 0..n {
        match pred[cur] {
            Some(p) => cur = p,
            None => return probe,
        }
    }
    // `cur` is now on a cycle; walk it and sum -ln(g).
    let start = cur;
    let mut log_sum = 0f64;
    let mut steps = 0usize;
    loop {
        match pred_edge[cur] {
            Some(ei) => {
                log_sum += edges[ei].2;
                cur = edges[ei].0;
                steps += 1;
                if cur == start || steps > n {
                    break;
                }
            }
            None => break,
        }
    }
    if steps >= 2 && log_sum < -1e-12 {
        let rate = (-log_sum).exp();
        if rate > probe.best_cycle_rate {
            probe.best_cycle_rate = rate;
        }
    }
    probe
}

// ---------------------------------------------------------------------------
// Cycle enumeration (bounded DFS from USD)
// ---------------------------------------------------------------------------

/// A candidate cycle as a leg sequence, closing back to its start.
pub type CycleLegs = Vec<Leg>;

/// Enumerate all simple cycles rooted at USD up to `MAX_HOPS` legs.
/// Returned cycles are canonical (start at USD) and deduped. Bounded work:
/// the DFS aborts after `budget` node expansions.
pub fn enumerate_cycles(g: &SwapGraph, budget: usize) -> Vec<CycleLegs> {
    let mut out: Vec<CycleLegs> = Vec::new();
    let mut seen: std::collections::HashSet<Vec<Leg>> = std::collections::HashSet::new();
    let mut work = 0usize;
    let mut path: Vec<Leg> = Vec::new();
    let mut on_path: std::collections::HashSet<Asset> = std::collections::HashSet::new();
    dfs(g, Asset::Usd, &mut path, &mut on_path, &mut seen, &mut out, &mut work, budget);
    out
}

fn dfs(
    g: &SwapGraph,
    node: Asset,
    path: &mut Vec<Leg>,
    on_path: &mut std::collections::HashSet<Asset>,
    seen: &mut std::collections::HashSet<Vec<Leg>>,
    out: &mut Vec<CycleLegs>,
    work: &mut usize,
    budget: usize,
) {
    if *work > budget || path.len() >= MAX_HOPS {
        return;
    }
    let Some(edges) = g.out_edges.get(&node) else { return };
    for leg in edges {
        *work += 1;
        if *work > budget {
            return;
        }
        let to = leg.to_asset();
        if to == Asset::Usd {
            // Closing edge — record the cycle if non-trivial.
            if path.len() + 1 >= 2 {
                let mut cyc: Vec<Leg> = path.clone();
                cyc.push(*leg);
                // Canonicalize: rotate so the first leg has the smallest key
                // (cycles from DFS always start at USD, so identity is stable).
                if seen.insert(cyc.clone()) {
                    out.push(cyc);
                }
            }
            continue;
        }
        if on_path.contains(&to) {
            continue; // simple cycles only
        }
        on_path.insert(to);
        path.push(*leg);
        dfs(g, to, path, on_path, seen, out, work, budget);
        path.pop();
        on_path.remove(&to);
    }
}

// ---------------------------------------------------------------------------
// Exact cycle execution (marginal greedy walk)
// ---------------------------------------------------------------------------

/// Exact evaluation of one cycle.
#[derive(Debug, Clone)]
pub struct CycleWalk {
    /// Entry notional in USD (gross, fees included in the conversion).
    pub entry_usd: Decimal,
    /// Exit USD received (net of all fees).
    pub exit_usd: Decimal,
    pub profit: Decimal,
    pub fees: Decimal,
    pub net_bps: Decimal,
    /// Per-leg executed VWAP (in the leg's own quote units) and size.
    pub legs: Vec<(Decimal, Decimal)>,
}

/// Per-leg walker state for the exact cycle walk.
struct W {
    px: Decimal,
    rem: Decimal,
    idx: usize,
    filled: Decimal,   // cumulative base units executed on this leg
    quote_ex: Decimal, // cumulative quote (ex-fee) on this leg
    fee_rate: Decimal,
    dir: LegDir,
}

/// Marginal conversion rate (output per input) of a leg at its current level,
/// fee included: Buy `1/(px*(1+fee))`, Sell `px*(1-fee)`.
fn g_of(w: &W) -> Decimal {
    match w.dir {
        LegDir::Buy => Decimal::ONE / (w.px * (Decimal::ONE + w.fee_rate)),
        LegDir::Sell => w.px * (Decimal::ONE - w.fee_rate),
    }
}

/// Walk a cycle of legs against captured levels and find the exact
/// profit-maximizing entry size (bounded by `max_notional`).
///
/// Marginal rates per leg (fees included):
/// - Buy:  `g = 1 / (ask * (1 + fee))`  [base per quote]
/// - Sell: `g = bid * (1 - fee)`        [quote per base]
///
/// The cycle product `P = prod(g)` is the marginal USD-per-USD rate at the
/// current fill state. `profit(x)` is concave piecewise-linear, so advancing
/// `x` while `P > 1` is exactly optimal. Each step advances to the next level
/// of whichever leg binds first.
pub fn walk_cycle(
    legs: &[Leg],
    levels: &[&LegLevels],
    fees: &HashMap<Venue, Decimal>,
    max_notional: Decimal,
) -> Option<CycleWalk> {
    let k = legs.len();
    if k < 2 || legs.len() != levels.len() {
        return None;
    }

    // Per-leg walker state (module-level struct `W`, marginal fn `g_of`).
    let mut w: Vec<W> = Vec::with_capacity(k);
    for (i, leg) in legs.iter().enumerate() {
        let lv = levels.get(i)?;
        let (px, sz) = lv.first().copied()?;
        w.push(W {
            px,
            rem: sz,
            idx: 0,
            filled: Decimal::ZERO,
            quote_ex: Decimal::ZERO,
            fee_rate: *fees.get(&leg.venue).unwrap_or(&Decimal::ZERO),
            dir: leg.dir,
        });
    }

    // Cumulative products G[i] = g_0 * ... * g_i (USD -> output of leg i).
    let mut cum: Vec<Decimal> = Vec::with_capacity(k);
    {
        let mut acc = Decimal::ONE;
        for leg_w in &w {
            acc = acc * g_of(leg_w);
            cum.push(acc);
        }
    }

    let mut entry = Decimal::ZERO;
    let mut exit = Decimal::ZERO;

    // Advance while the marginal cycle rate > 1.
    loop {
        // Recompute cumulative products at current levels.
        let mut acc = Decimal::ONE;
        for (i, lw) in w.iter().enumerate() {
            acc = acc * g_of(lw);
            cum[i] = acc;
        }
        let total = cum[k - 1];
        if total <= Decimal::ONE {
            break;
        }

        // Max dx (entry USD) before some leg exhausts its current level or
        // the notional budget runs out.
        let budget = if max_notional > Decimal::ZERO {
            (max_notional - entry).max(Decimal::ZERO)
        } else {
            Decimal::MAX
        };
        if budget.is_zero() {
            break;
        }
        let mut dx = budget;
        for i in 0..k {
            // input flow to leg i per unit of entry: 1 / G_{i-1}
            let into_leg = if i == 0 {
                Decimal::ONE
            } else {
                Decimal::ONE / cum[i - 1]
            };
            // Buy legs constrain on output (base): dx * G_i <= rem
            // Sell legs constrain on input (base): dx / G_{i-1} <= rem
            let cap = match w[i].dir {
                LegDir::Buy => w[i].rem * (Decimal::ONE / cum[i]),
                LegDir::Sell => w[i].rem * into_leg,
            };
            if cap < dx {
                dx = cap;
            }
        }
        if dx <= Decimal::ZERO {
            // A leg has an empty current level: try to advance it.
            let mut advanced = false;
            for i in 0..k {
                if w[i].rem <= Decimal::ZERO {
                    let lv = levels.get(i)?;
                    w[i].idx += 1;
                    match lv.get(w[i].idx) {
                        Some((px, sz)) => {
                            w[i].px = *px;
                            w[i].rem = *sz;
                            advanced = true;
                        }
                        None => return finish(entry, exit, legs, &w),
                    }
                }
            }
            if !advanced {
                return finish(entry, exit, legs, &w);
            }
            continue;
        }

        // Execute dx through every leg.
        // Input flow to leg i per entry unit is G_{i-1} = cum[i-1] (the
        // cumulative marginal product converts USD into leg i's input asset).
        // Output flow = input * g_i.
        for i in 0..k {
            let in_per_entry = if i == 0 {
                Decimal::ONE
            } else {
                cum[i - 1]
            };
            let flow_in = dx * in_per_entry; // units: input asset of leg i
            let flow_out = flow_in * g_of(&w[i]); // units: output asset of leg i
            match w[i].dir {
                LegDir::Buy => {
                    // input = quote, output = base. Level size is in base.
                    w[i].filled += flow_out;
                    w[i].quote_ex += flow_in;
                    w[i].rem -= flow_out;
                }
                LegDir::Sell => {
                    // input = base, output = quote. Level size is in base.
                    w[i].filled += flow_in;
                    w[i].quote_ex += flow_out;
                    w[i].rem -= flow_in;
                }
            }
        }
        entry += dx;
        exit += dx * total; // exact within this step (marginals constant)

        // Advance exhausted levels.
        for i in 0..k {
            while w[i].rem <= Decimal::ZERO {
                let lv = levels.get(i)?;
                w[i].idx += 1;
                match lv.get(w[i].idx) {
                    Some((px, sz)) => {
                        w[i].px = *px;
                        w[i].rem = *sz;
                    }
                    None => return finish(entry, exit, legs, &w),
                }
            }
        }
    }

    finish(entry, exit, legs, &w)
}

/// Finalize a completed walk into a `CycleWalk` (None if nothing filled).
fn finish(entry: Decimal, exit: Decimal, legs: &[Leg], w: &[W]) -> Option<CycleWalk> {
    if entry <= Decimal::ZERO {
        return None;
    }
    let profit = exit - entry;
    // Fees: buy legs pay fee on quote spent; sell legs pay fee on quote received.
    let mut fees = Decimal::ZERO;
    let mut leg_vwaps = Vec::with_capacity(legs.len());
    for lw in w {
        fees += lw.fee_rate * lw.quote_ex;
        let vwap = if lw.filled.is_zero() {
            Decimal::ZERO
        } else {
            (lw.quote_ex / lw.filled).round_dp(6)
        };
        leg_vwaps.push((vwap, lw.filled.round_dp(8)));
    }
    let net_bps = if entry.is_zero() {
        Decimal::ZERO
    } else {
        profit / entry * Decimal::from(10_000u64)
    };
    Some(CycleWalk {
        entry_usd: entry.round_dp(2),
        exit_usd: exit.round_dp(2),
        profit: profit.round_dp(4),
        fees: fees.round_dp(4),
        net_bps: net_bps.round_dp(2),
        legs: leg_vwaps,
    })
}

// ---------------------------------------------------------------------------
// Wire types
// ---------------------------------------------------------------------------

/// One rendered hop of a cycle opportunity.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CycleHop {
    pub venue: Venue,
    /// Asset traded on this leg ("ETH", "USDT", ...).
    pub asset: String,
    /// "buy" | "sell".
    pub dir: String,
    /// Executed VWAP in the leg's own quote units.
    pub px: String,
    /// Executed size in base units.
    pub sz: String,
}

/// A live multi-hop swap opportunity.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CycleOpportunity {
    /// Canonical route id, e.g. "usd>eth@kr>usdt@bn>usd".
    pub route: String,
    pub hops: Vec<CycleHop>,
    pub entry_usd: String,
    pub exit_usd: String,
    pub profit_usd: String,
    pub net_bps: String,
    pub ts: u64,
}

/// A paper cycle fill (or latency expiry).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CycleFill {
    pub id: u64,
    pub route: String,
    pub hops: Vec<CycleHop>,
    pub entry_usd: String,
    pub profit_usd: String,
    pub net_bps: String,
    pub status: String,
    pub detected_net_bps: String,
    pub ts: u64,
}

/// Aggregate cycle engine stats.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct CycleStats {
    pub cycles_tracked: u64,
    pub open_cycles: usize,
    pub fills: u64,
    pub expired: u64,
    pub pnl_usd: String,
    pub best_net_bps: String,
    /// Graph telemetry from the last probe.
    pub graph_nodes: usize,
    pub graph_edges: usize,
    /// Touch product of the tightest negative cycle (1.0 = none).
    pub graph_best_rate: String,
    /// Negative cycles detected by Bellman-Ford this pass.
    pub negative_cycles: bool,
}

/// One genome the GA evolves.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GaGenome {
    pub fire_edge_bps: String,
    pub min_edge_bps: String,
    pub latency_ms: u64,
    pub cooldown_ms: u64,
    pub max_notional_usd: String,
    /// Capital weight for ETH routes [0, 1].
    pub eth_w: String,
    /// Capital weight for SOL routes [0, 1].
    pub sol_w: String,
}

impl Default for GaGenome {
    fn default() -> Self {
        Self {
            fire_edge_bps: "8".into(),
            min_edge_bps: "3".into(),
            latency_ms: 250,
            cooldown_ms: 3000,
            max_notional_usd: "10000".into(),
            eth_w: "0.5".into(),
            sol_w: "0.5".into(),
        }
    }
}

/// Live GA state for the wire.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct GaState {
    pub enabled: bool,
    pub generation: u64,
    pub population: usize,
    /// Fitness of the best genome (simulated net P&L over the recording window).
    pub best_fitness: String,
    /// Fitness of the engine's CURRENT parameters on the same window.
    pub current_fitness: String,
    pub best: GaGenome,
    /// Generation whose best genome was last hot-applied.
    pub applied_gen: u64,
    /// Fitness history (generation, best fitness).
    pub history: Vec<(u64, f64)>,
    /// Empirical latency-survival calibration: (edge bucket bps, survival rate).
    pub survival: Vec<(String, String)>,
    /// Observations recorded in the replay window.
    pub observations: usize,
}

impl Default for SwapGraph {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(x: i64) -> Decimal {
        Decimal::from(x)
    }

    #[test]
    fn two_leg_cycle_buys_the_crossed_depth() {
        // USD -> ETH (buy 10 @ 3000 on Kraken) -> USD (sell 5 @ 3001 on HL).
        let legs = vec![
            Leg { venue: Venue::Kraken, asset: Asset::Base(Market::Eth), dir: LegDir::Buy },
            Leg { venue: Venue::Hyperliquid, asset: Asset::Base(Market::Eth), dir: LegDir::Sell },
        ];
        let asks: LegLevels = vec![(d(3000), d(10))];
        let bids: LegLevels = vec![(d(3001), d(5))];
        let levels: Vec<&LegLevels> = vec![&asks, &bids];
        let fees: HashMap<Venue, Decimal> = HashMap::new();
        let w = walk_cycle(&legs, &levels, &fees, d(1_000_000)).unwrap();
        // Optimal size = 5 ETH (limited by the sell side): entry 15000, exit 15005.
        assert_eq!(w.entry_usd, d(15000));
        assert_eq!(w.exit_usd, d(15005));
        assert_eq!(w.profit, d(5));
    }

    #[test]
    fn fees_kill_a_thin_edge() {
        let legs = vec![
            Leg { venue: Venue::Kraken, asset: Asset::Base(Market::Eth), dir: LegDir::Buy },
            Leg { venue: Venue::Hyperliquid, asset: Asset::Base(Market::Eth), dir: LegDir::Sell },
        ];
        let asks: LegLevels = vec![(d(3000), d(10))];
        let bids: LegLevels = vec![(d(3001), d(5))];
        let levels: Vec<&LegLevels> = vec![&asks, &bids];
        let mut fees: HashMap<Venue, Decimal> = HashMap::new();
        fees.insert(Venue::Kraken, Decimal::new(26, 4)); // 26 bps
        fees.insert(Venue::Hyperliquid, Decimal::new(45, 4)); // 45 bps
        let w = walk_cycle(&legs, &levels, &fees, d(1_000_000));
        assert!(w.is_none(), "fees should kill the 3.3bps edge");
    }

    #[test]
    fn three_hop_through_usdt() {
        // USD -> USDT (buy @0.999) -> ETH (buy @3000 USDT) -> USD (sell @3005).
        // Rate product at touch: (1/0.999) * (1/3000) * 3005 = 3005/(0.999*3000) = 1.00200...
        let legs = vec![
            Leg { venue: Venue::Kraken, asset: Asset::Usdt, dir: LegDir::Buy },
            Leg { venue: Venue::Binance, asset: Asset::Base(Market::Eth), dir: LegDir::Buy },
            Leg { venue: Venue::Hyperliquid, asset: Asset::Base(Market::Eth), dir: LegDir::Sell },
        ];
        let fx_asks: LegLevels = vec![(Decimal::new(999, 3), d(100_000))];
        let eth_asks: LegLevels = vec![(d(3000), d(10))];
        let eth_bids: LegLevels = vec![(d(3005), d(10))];
        let levels: Vec<&LegLevels> = vec![&fx_asks, &eth_asks, &eth_bids];
        let fees: HashMap<Venue, Decimal> = HashMap::new();
        let w = walk_cycle(&legs, &levels, &fees, d(1_000_000)).unwrap();
        assert!(w.profit > Decimal::ZERO, "profit {w:?}");
        // Full depth: 10 ETH; entry = 10 * 0.999 * 3000 = 29970 USD; exit = 10 * 3005 = 30050.
        assert_eq!(w.entry_usd, d(29970));
        assert_eq!(w.exit_usd, d(30050));
        assert_eq!(w.profit, d(80));
    }

    #[test]
    fn enumeration_finds_usd_rooted_cycles() {
        let mut g = SwapGraph::new();
        g.add_usdt_book(
            vec![(Decimal::new(999, 3), d(100_000))],
            vec![(Decimal::new(999, 3), d(100_000))],
            8,
        );
        g.add_market_book(
            Venue::Binance,
            Market::Eth,
            vec![(d(3001), d(10))],
            vec![(d(3000), d(10))],
            8,
        );
        g.add_market_book(
            Venue::Hyperliquid,
            Market::Eth,
            vec![(d(3002), d(10))],
            vec![(d(3003), d(10))],
            8,
        );
        g.finalize();
        let cycles = enumerate_cycles(&g, 10_000);
        assert!(!cycles.is_empty(), "no cycles enumerated");
        // Every cycle must start at a USD-out edge and close at USD.
        for c in &cycles {
            assert_eq!(c[0].from_asset(), Asset::Usd);
            assert_eq!(c.last().unwrap().to_asset(), Asset::Usd);
        }
        // The 2-edge USD->ETH@BN->USD@HL cycle must be among them.
        assert!(cycles.iter().any(|c| c.len() == 2));
    }

    #[test]
    fn bellman_ford_flags_negative_cycle() {
        let mut g = SwapGraph::new();
        g.add_market_book(
            Venue::Kraken,
            Market::Eth,
            vec![(d(3001), d(10))],
            vec![(d(3000), d(10))],
            8,
        );
        g.add_market_book(
            Venue::Hyperliquid,
            Market::Eth,
            vec![(d(3002), d(10))],
            vec![(d(3003), d(10))],
            8,
        );
        g.finalize();
        let fees: HashMap<Venue, Decimal> = HashMap::new();
        let probe = probe_negative_cycles(&g, &fees);
        assert!(probe.negative_cycle);
        assert!(probe.best_cycle_rate > 1.0, "rate {}", probe.best_cycle_rate);
    }
}
