//! Multi-hop swap arbitrage engine (Bellman-Ford + DFS enumeration + exact
//! cycle walks + latency-modeled paper execution).
//!
//! Cadence: 2 Hz (graph build + probe + enumeration + exact walks are all
//! cheap on a 14-node / ~80-edge graph with bounded DFS).
//!
//! Config is shared with the 2-leg engine's `ArbConfig` (edges, fees,
//! notional, latency, cooldown) so the UI edits one place.

use crate::state::{now_ms, BcKind, Registry};
use ob_core::{
    enumerate_cycles, probe_negative_cycles, walk_cycle, ArbConfig, Asset, CycleFill, CycleHop,
    CycleOpportunity, CycleStats, CycleWalk, Leg, LegDir, Market, SwapGraph, Venue, WireEvent,
    VenueState, ARB_MIN_NOTIONAL,
};
use rust_decimal::prelude::{FromPrimitive, ToPrimitive};
use rust_decimal::Decimal;
use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// Levels captured per leg side (top 64 is far beyond any optimal walk).
const LEG_DEPTH: usize = 64;

/// DFS node-expansion budget per scan.
const DFS_BUDGET: usize = 20_000;

/// Max cycles evaluated per scan (sorted by touch product first).
const MAX_EVAL: usize = 240;

/// Tracked cycle TTL (same as the 2-leg engine).
const CYCLE_TTL_MS: u64 = 2500;

/// Fill log size for the wire.
const FILL_LOG: usize = 40;

/// Config view for one scan pass.
#[derive(Clone)]
struct ScanCfg {
    enabled: bool,
    min_edge: Decimal,
    fire_edge: Decimal,
    max_notional: Decimal,
    latency_ms: u64,
    cooldown_ms: u64,
    fees: HashMap<Venue, Decimal>,
}

fn parse_cfg(cfg: &ArbConfig) -> ScanCfg {
    let d = |s: &str| s.parse::<Decimal>().unwrap_or(Decimal::ZERO);
    let fees = cfg
        .fees_bps
        .iter()
        .map(|(v, bps)| (*v, d(bps) / Decimal::from(10_000u64)))
        .collect();
    ScanCfg {
        enabled: cfg.enabled,
        min_edge: d(&cfg.min_edge_bps),
        fire_edge: d(&cfg.fire_edge_bps),
        max_notional: d(&cfg.max_notional_usd),
        latency_ms: cfg.latency_ms,
        cooldown_ms: cfg.cooldown_ms,
        fees,
    }
}

pub(crate) struct TrackedCycle {
    opp: CycleOpportunity,
    #[allow(dead_code)]
    legs: Vec<Leg>,
    last_seen: u64,
    last_fire: u64,
    in_flight: bool,
}

#[derive(Default)]
struct CStats {
    cycles_tracked: u64,
    fills: u64,
    expired: u64,
    pnl: Decimal,
    best_bps: Decimal,
}

pub struct CycleEngine {
    inner: Mutex<Inner>,
}

struct Inner {
    cfg: ScanCfg,
    tracked: HashMap<String, TrackedCycle>,
    fills: VecDeque<CycleFill>,
    stats: CStats,
    probe: ob_core::cycle::GraphProbe,
    next_fill_id: u64,
}

impl CycleEngine {
    pub fn new() -> Self {
        Self {
            inner: Mutex::new(Inner {
                cfg: parse_cfg(&ArbConfig::default()),
                tracked: HashMap::new(),
                fills: VecDeque::new(),
                stats: CStats::default(),
                probe: Default::default(),
                next_fill_id: 1,
            }),
        }
    }

    /// Sync engine parameters from the (live-edited) ArbConfig.
    pub fn set_config(&self, cfg: &ArbConfig) {
        self.inner.lock().unwrap().cfg = parse_cfg(cfg);
    }

    pub fn health_json(&self) -> serde_json::Value {
        let i = self.inner.lock().unwrap();
        serde_json::json!({
            "open_cycles": i.tracked.len(),
            "fills": i.stats.fills,
            "expired": i.stats.expired,
            "pnl_usd": i.stats.pnl.normalize().to_string(),
            "negative_cycles": i.probe.negative_cycle,
            "graph_edges": i.probe.edges,
        })
    }
}

/// Route id of a leg sequence, e.g. "usd>eth@kr>usdt@bn>usd".
fn route_of(legs: &[Leg]) -> String {
    let mut s = String::from("usd");
    for l in legs {
        s.push('>');
        s.push_str(&l.asset.label().to_lowercase());
        s.push('@');
        s.push_str(&l.venue.short().to_lowercase());
    }
    s.push_str(">usd");
    s
}

/// Build the live swap graph under the books lock.
fn build_graph(reg: &Registry) -> SwapGraph {
    let mut g = SwapGraph::new();
    let books = reg.books.lock().unwrap();
    for (m, mb) in books.iter() {
        for (i, slot) in mb.venues.iter().enumerate() {
            let Some(venue) = Venue::from_index(i) else { continue };
            let Some(st) = slot.as_ref() else { continue };
            let (bids, asks) = st.capture(LEG_DEPTH);
            g.add_market_book(venue, *m, bids, asks, LEG_DEPTH);
        }
    }
    drop(books);
    {
        let usdt = reg.usdt_book.lock().unwrap();
        let (bids, asks) = usdt.capture(LEG_DEPTH);
        g.add_usdt_book(bids, asks, LEG_DEPTH);
    }
    g.finalize();
    g
}

/// One scan pass (2 Hz from the publish loop).
pub(crate) fn tick(reg: &Arc<Registry>) {
    // Refresh the config view from the arb engine (single source of truth).
    let cfg = {
        let a = reg.arb.inner_cfg();
        parse_cfg(&a)
    };
    let now = now_ms();
    let dust = Decimal::from_f64(ARB_MIN_NOTIONAL).unwrap_or_else(|| Decimal::from(100u64));

    // Build graph + probe.
    let g = build_graph(reg);
    let probe = probe_negative_cycles(&g, &cfg.fees);

    // Enumerate + evaluate only when a negative cycle could exist.
    let mut observed: Vec<(String, Vec<Leg>, CycleWalk)> = Vec::new();
    if probe.negative_cycle || probe.best_cycle_rate > 1.0 {
        let cycles = enumerate_cycles(&g, DFS_BUDGET);
        for legs in cycles {
            if legs.len() > 5 {
                continue;
            }
            let levels: Vec<&ob_core::cycle::LegLevels> =
                legs.iter().filter_map(|l| g.levels.get(l)).collect();
            if levels.len() != legs.len() {
                continue;
            }
            let Some(w) = walk_cycle(&legs, &levels, &cfg.fees, cfg.max_notional) else {
                continue;
            };
            if w.profit <= Decimal::ZERO || w.entry_usd < dust {
                continue;
            }
            let route = route_of(&legs);
            observed.push((route, legs, w));
            if observed.len() >= MAX_EVAL {
                break;
            }
        }
    }

    // Update tracked state, fire, prune.
    let mut fire: Vec<(String, Vec<Leg>, CycleWalk)> = Vec::new();
    {
        let mut i = reg.cycles.inner.lock().unwrap();
        i.cfg = cfg.clone();
        i.probe = probe;
        for (route, legs, w) in observed {
            if w.net_bps < cfg.min_edge {
                continue;
            }
            let opp = opp_from(&route, &legs, &w, now);
            if !i.tracked.contains_key(&route) {
                i.stats.cycles_tracked += 1;
                i.tracked.insert(
                    route.clone(),
                    TrackedCycle {
                        opp: opp.clone(),
                        legs: legs.clone(),
                        last_seen: now,
                        last_fire: 0,
                        in_flight: false,
                    },
                );
            }
            let entry = i.tracked.get_mut(&route).expect("just inserted");
            entry.opp = opp;
            entry.last_seen = now;

            // EV gate (same policy as the 2-leg and sweep engines):
            // profit × P(survive latency) > 0.5 bps churn floor.
            let surv = crate::ga::survival_rate(&reg.ga, w.net_bps.to_f64().unwrap_or(0.0));
            let ev = w.profit * Decimal::from_f64(surv).unwrap_or(Decimal::ZERO);
            let churn = w.entry_usd * Decimal::new(5, 5); // 0.5 bps
            let can_fire = cfg.enabled
                && cfg.fire_edge > Decimal::ZERO
                && w.net_bps >= cfg.fire_edge
                && ev > churn
                && !entry.in_flight
                && now.saturating_sub(entry.last_fire) >= cfg.cooldown_ms;
            if can_fire {
                entry.in_flight = true;
                entry.last_fire = now;
                fire.push((route, legs, w));
            }
        }
        i.tracked
            .retain(|_, t| now.saturating_sub(t.last_seen) < CYCLE_TTL_MS);
    }

    for (route, legs, w) in fire {
        tracing::info!(
            "[cycles] firing {route}: {} legs, net {} bps, profit {} USD, exec in {}ms",
            legs.len(),
            w.net_bps,
            w.profit,
            cfg.latency_ms
        );
        let reg2 = Arc::clone(reg);
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(cfg.latency_ms.max(1))).await;
            execute(reg2, route, legs);
        });
    }
}

fn opp_from(route: &str, legs: &[Leg], w: &CycleWalk, now: u64) -> CycleOpportunity {
    let hops = legs
        .iter()
        .zip(w.legs.iter())
        .map(|(l, (vwap, sz))| CycleHop {
            venue: l.venue,
            asset: l.asset.label(),
            dir: l.dir.label().to_string(),
            px: vwap.normalize().to_string(),
            sz: sz.normalize().to_string(),
        })
        .collect();
    CycleOpportunity {
        route: route.to_string(),
        hops,
        entry_usd: w.entry_usd.normalize().to_string(),
        exit_usd: w.exit_usd.normalize().to_string(),
        profit_usd: w.profit.normalize().to_string(),
        net_bps: w.net_bps.normalize().to_string(),
        ts: now,
    }
}

/// Execute a scheduled cycle fill against the CURRENT books (after the
/// simulated latency). Expired if the edge vanished.
fn execute(reg: Arc<Registry>, route: String, legs: Vec<Leg>) {
    let cfg = {
        let a = reg.arb.inner_cfg();
        parse_cfg(&a)
    };
    let (fill_id, now) = {
        let mut i = reg.cycles.inner.lock().unwrap();
        let id = i.next_fill_id;
        i.next_fill_id += 1;
        (id, now_ms())
    };

    // Re-walk the current books along the same route.
    let mut walked: Option<CycleWalk> = None;
    {
        let g = build_graph(&reg);
        let levels: Vec<&ob_core::cycle::LegLevels> =
            legs.iter().filter_map(|l| g.levels.get(l)).collect();
        if levels.len() == legs.len() {
            if let Some(w) = walk_cycle(&legs, &levels, &cfg.fees, cfg.max_notional) {
                if w.profit > Decimal::ZERO {
                    walked = Some(w);
                }
            }
        }
    }

    let ev: CycleFill;
    {
        let mut i = reg.cycles.inner.lock().unwrap();
        if let Some(t) = i.tracked.get_mut(&route) {
            t.in_flight = false;
        }
        let detected = i
            .tracked
            .get(&route)
            .map(|t| t.opp.net_bps.clone())
            .unwrap_or_default();
        let detected_bps: f64 = detected.parse().unwrap_or(0.0);
        match walked {
            Some(w) => {
                let opp = opp_from(&route, &legs, &w, now);
                ev = CycleFill {
                    id: fill_id,
                    route: route.clone(),
                    hops: opp.hops,
                    entry_usd: opp.entry_usd,
                    profit_usd: opp.profit_usd,
                    net_bps: opp.net_bps,
                    status: "filled".into(),
                    detected_net_bps: detected,
                    ts: now,
                };
                i.stats.fills += 1;
                i.stats.pnl += w.profit;
                if w.net_bps > i.stats.best_bps {
                    i.stats.best_bps = w.net_bps;
                }
                // Calibrate the GA's latency-survival model with the outcome.
                crate::ga::record_outcome(&reg.ga, detected_bps, true);
            }
            None => {
                ev = CycleFill {
                    id: fill_id,
                    route: route.clone(),
                    hops: Vec::new(),
                    entry_usd: "0".into(),
                    profit_usd: "0".into(),
                    net_bps: "0".into(),
                    status: "expired".into(),
                    detected_net_bps: detected,
                    ts: now,
                };
                i.stats.expired += 1;
                crate::ga::record_outcome(&reg.ga, detected_bps, false);
            }
        }
        i.fills.push_back(ev.clone());
        while i.fills.len() > FILL_LOG {
            i.fills.pop_front();
        }
    }
    if ev.status == "filled" {
        tracing::info!(
            "[cycles] fill #{} {route}: pnl {} USD ({} bps net)",
            ev.id,
            ev.profit_usd,
            ev.net_bps
        );
    }
    reg.broadcast(BcKind::CycleFill, Market::Eth, &WireEvent::CycleFillEvent { fill: ev });
}

/// 2 Hz live update broadcast.
pub(crate) fn broadcast_update(reg: &Registry) {
    let (stats, opps) = {
        let i = reg.cycles.inner.lock().unwrap();
        (stats_of(&i), live_ops(&i))
    };
    reg.broadcast(
        BcKind::CycleUpdate,
        Market::Eth,
        &WireEvent::CycleUpdate {
            stats,
            opportunities: opps,
        },
    );
}

/// Full snapshot for new sockets.
pub fn snapshot(reg: &Registry) -> WireEvent {
    let i = reg.cycles.inner.lock().unwrap();
    WireEvent::CycleSnapshot {
        stats: stats_of(&i),
        opportunities: live_ops(&i),
        fills: i.fills.iter().cloned().collect(),
    }
}

fn stats_of(i: &Inner) -> CycleStats {
    CycleStats {
        cycles_tracked: i.stats.cycles_tracked,
        open_cycles: i.tracked.len(),
        fills: i.stats.fills,
        expired: i.stats.expired,
        pnl_usd: i.stats.pnl.normalize().to_string(),
        best_net_bps: i.stats.best_bps.normalize().to_string(),
        graph_nodes: i.probe.nodes,
        graph_edges: i.probe.edges,
        graph_best_rate: format!("{:.6}", i.probe.best_cycle_rate),
        negative_cycles: i.probe.negative_cycle,
    }
}

/// Live opportunities, best net edge first (top 12).
fn live_ops(i: &Inner) -> Vec<CycleOpportunity> {
    let mut ops: Vec<CycleOpportunity> = i.tracked.values().map(|t| t.opp.clone()).collect();
    ops.sort_by(|a, b| {
        b.net_bps
            .parse::<Decimal>()
            .unwrap_or(Decimal::ZERO)
            .cmp(&a.net_bps.parse::<Decimal>().unwrap_or(Decimal::ZERO))
    });
    ops.truncate(12);
    ops
}

/// Reset paper-trading stats.
pub fn reset(reg: &Registry) {
    let mut i = reg.cycles.inner.lock().unwrap();
    i.stats = CStats::default();
    i.fills.clear();
    i.tracked.clear();
    drop(i);
    reg.broadcast(BcKind::CycleSnapshot, Market::Eth, &snapshot(reg));
}

/// Hook used by `arb::set_config` to keep parameters in sync.
pub fn sync_config(reg: &Registry, cfg: &ArbConfig) {
    reg.cycles.set_config(cfg);
}

/// Vendored read of the tracked-cycle count (for tests).
#[allow(dead_code)]
pub(crate) fn open_count(reg: &Registry) -> usize {
    reg.cycles.inner.lock().unwrap().tracked.len()
}

/// Mark: `LegDir` is re-exported for downstream modules.
#[allow(dead_code)]
fn _legdir_marker(d: LegDir) -> &'static str {
    d.label()
}

/// Mark: `VenueState` import used by `build_graph` via `capture`.
#[allow(dead_code)]
fn _vs_marker(_: &VenueState) {}

/// Mark: `Asset` used in route construction.
#[allow(dead_code)]
fn _asset_marker(a: Asset) -> String {
    a.label()
}
