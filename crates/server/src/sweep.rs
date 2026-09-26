//! Global sweep engine: fires the exact optimal multi-venue execution plan
//! (see `ob_core::sweep`) with survival-calibrated EV gating.
//!
//! Cadence: 2 Hz (curve merge + greedy crossing are microseconds on a
//! 9-venue graph; the plans are far smaller than the depth budget).
//!
//! ## Firing policy — expected-value gate
//!
//! An edge that *detects* at X bps only converts to P&L if it survives the
//! latency window. The GA engine measures that survival probability
//! empirically (per edge bucket, Beta-smoothed). The sweep therefore fires
//! iff
//!
//! ```text
//! profit_usd * P(survive | net_bps)  >  churn_floor
//! churn_floor = 0.5 bps * deployed_notional
//! ```
//!
//! in addition to the raw `fire_edge` threshold. This skips negative-EV
//! fires that would otherwise burn cooldown slots and block better routes —
//! the single highest-leverage "profit maximization" rule in the system.
//!
//! Config is shared with the 2-leg engine's `ArbConfig` (single source of
//! truth for fees, thresholds, latency, cooldown, notional).

use crate::state::{now_ms, BcKind, Registry};
use ob_core::{
    sweep_optimize, sweep_plan_of, ArbConfig, EquityPt, Market, SweepFill, SweepPlan, SweepStats,
    SweepWalk, Venue, WireEvent, ARB_EQUITY_WINDOW, ARB_MIN_NOTIONAL,
};
use rust_decimal::prelude::{FromPrimitive, ToPrimitive};
use rust_decimal::Decimal;
use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// Tracked plan TTL (same as the other engines).
const SWEEP_TTL_MS: u64 = 2500;

/// Fill log size for the wire.
const FILL_LOG: usize = 40;

/// Churn floor: 0.5 bps of deployed notional (5e-5), matching the GA's
/// churn penalty so the gate and the optimizer agree on what a wasted fire
/// costs. (`Decimal::new` is not const, hence the fn.)
#[inline]
fn churn_bps() -> Decimal {
    Decimal::new(5, 5)
}

struct TrackedSweep {
    plan: SweepPlan,
    last_seen: u64,
    last_fire: u64,
    in_flight: bool,
}

#[derive(Default)]
struct SStats {
    sweeps_tracked: u64,
    fills: u64,
    expired: u64,
    pnl: Decimal,
    best_bps: Decimal,
    sum_bps: Decimal,
    venues_sum: Decimal, // summed venue counts of filled plans
    equity: VecDeque<EquityPt>,
}

pub struct SweepEngine {
    inner: Mutex<Inner>,
}

struct Inner {
    tracked: HashMap<Market, TrackedSweep>,
    fills: VecDeque<SweepFill>,
    stats: SStats,
    next_fill_id: u64,
}

impl SweepEngine {
    pub fn new() -> Self {
        Self {
            inner: Mutex::new(Inner {
                tracked: HashMap::new(),
                fills: VecDeque::new(),
                stats: SStats::default(),
                next_fill_id: 1,
            }),
        }
    }

    pub fn health_json(&self) -> serde_json::Value {
        let i = self.inner.lock().unwrap();
        serde_json::json!({
            "open_plans": i.tracked.len(),
            "fills": i.stats.fills,
            "expired": i.stats.expired,
            "pnl_usd": i.stats.pnl.normalize().to_string(),
        })
    }
}

/// Parsed config view (same parsing as the 2-leg engine).
fn parse_fees(cfg: &ArbConfig) -> HashMap<Venue, Decimal> {
    let d = |s: &str| s.parse::<Decimal>().unwrap_or(Decimal::ZERO);
    cfg.fees_bps
        .iter()
        .map(|(v, bps)| (*v, d(bps) / Decimal::from(10_000u64)))
        .collect()
}

/// One scan pass (2 Hz from the publish loop).
pub(crate) fn tick(reg: &Arc<Registry>) {
    let cfg = reg.arb.inner_cfg();
    let min_edge: Decimal = cfg.min_edge_bps.parse().unwrap_or(Decimal::ZERO);
    let fire_edge: Decimal = cfg.fire_edge_bps.parse().unwrap_or(Decimal::ZERO);
    let max_notional: Decimal = cfg.max_notional_usd.parse().unwrap_or(Decimal::ZERO);
    let now = now_ms();
    let dust = Decimal::from_f64(ARB_MIN_NOTIONAL).unwrap_or_else(|| Decimal::from(100u64));
    let (w_eth, w_sol) = reg.ga.weights();

    // 1) Compute the optimal sweep plan per market under the books lock.
    let mut observed: Vec<(Market, SweepWalk)> = Vec::new();
    {
        let fees = parse_fees(&cfg);
        let books = reg.books.lock().unwrap();
        for market in Market::ARB {
            let Some(mb) = books.get(&market) else { continue };
            let sourced = reg.sourced_of(mb);
            if sourced.len() < 2 {
                continue;
            }
            let w = match market {
                Market::Eth => w_eth,
                _ => w_sol,
            };
            let eff_notional =
                max_notional * Decimal::from_f64((w * 2.0).min(1.0)).unwrap_or(Decimal::ONE);
            if let Some(walk) = sweep_optimize(&sourced, &fees, eff_notional) {
                if walk.cost >= dust && walk.profit > Decimal::ZERO {
                    observed.push((market, walk));
                }
            }
        }
    }

    // 2) Track + EV-gated fire.
    let mut fire: Vec<(Market, SweepWalk)> = Vec::new();
    {
        let mut i = reg.sweep.inner.lock().unwrap();
        for (market, w) in observed {
            if w.net_bps < min_edge {
                continue;
            }
            // EV gate: expected profit must clear the churn floor.
            let surv = crate::ga::survival_rate(&reg.ga, w.net_bps.to_f64().unwrap_or(0.0));
            let ev = w.profit * Decimal::from_f64(surv).unwrap_or(Decimal::ZERO);
            let churn = w.cost * churn_bps();
            let plan = sweep_plan_of(market, &w, ev, now);
            if !i.tracked.contains_key(&market) {
                i.stats.sweeps_tracked += 1;
                i.tracked.insert(
                    market,
                    TrackedSweep {
                        plan: plan.clone(),
                        last_seen: now,
                        last_fire: 0,
                        in_flight: false,
                    },
                );
            }
            let entry = i.tracked.get_mut(&market).expect("just inserted");
            entry.plan = plan;
            entry.last_seen = now;

            let can_fire = cfg.enabled
                && fire_edge > Decimal::ZERO
                && w.net_bps >= fire_edge
                && ev > churn
                && !entry.in_flight
                && now.saturating_sub(entry.last_fire) >= cfg.cooldown_ms;
            if can_fire {
                entry.in_flight = true;
                entry.last_fire = now;
                fire.push((market, w));
            }
        }
        i.tracked
            .retain(|_, t| now.saturating_sub(t.last_seen) < SWEEP_TTL_MS);
    }

    // 3) Schedule paper executions with simulated latency.
    for (market, w) in fire {
        tracing::info!(
            "[sweep] firing {} optimal plan: {} legs, {} bps net, {} USD profit, exec in {}ms",
            market.label(),
            w.legs.len(),
            w.net_bps,
            w.profit,
            cfg.latency_ms
        );
        let reg2 = Arc::clone(reg);
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(cfg.latency_ms.max(1))).await;
            execute(reg2, market);
        });
    }
}

/// Execute a scheduled sweep fill against the CURRENT books (after the
/// simulated latency). Expired if the plan no longer clears.
fn execute(reg: Arc<Registry>, market: Market) {
    let cfg = reg.arb.inner_cfg();
    let max_notional: Decimal = cfg.max_notional_usd.parse().unwrap_or(Decimal::ZERO);
    let (fill_id, now) = {
        let mut i = reg.sweep.inner.lock().unwrap();
        let id = i.next_fill_id;
        i.next_fill_id += 1;
        (id, now_ms())
    };

    // Re-walk the live books.
    let mut walked: Option<SweepWalk> = None;
    {
        let fees = parse_fees(&cfg);
        let books = reg.books.lock().unwrap();
        if let Some(mb) = books.get(&market) {
            let sourced = reg.sourced_of(mb);
            if let Some(w) = sweep_optimize(&sourced, &fees, max_notional) {
                if w.profit > Decimal::ZERO {
                    walked = Some(w);
                }
            }
        }
    }

    let ev: SweepFill;
    {
        let mut i = reg.sweep.inner.lock().unwrap();
        if let Some(t) = i.tracked.get_mut(&market) {
            t.in_flight = false;
        }
        let detected = i
            .tracked
            .get(&market)
            .map(|t| t.plan.net_bps.clone())
            .unwrap_or_default();
        let detected_bps: f64 = detected.parse().unwrap_or(0.0);
        match walked {
            Some(w) => {
                let plan = sweep_plan_of(market, &w, Decimal::ZERO, now);
                ev = SweepFill {
                    id: fill_id,
                    market,
                    status: "filled".into(),
                    legs: plan.legs,
                    notional: plan.notional,
                    profit_usd: plan.profit_usd,
                    net_bps: plan.net_bps,
                    detected_net_bps: detected,
                    ts: now,
                };
                i.stats.fills += 1;
                i.stats.pnl += w.profit;
                i.stats.sum_bps += w.net_bps;
                i.stats.venues_sum += Decimal::from(w.legs.len());
                if w.net_bps > i.stats.best_bps {
                    i.stats.best_bps = w.net_bps;
                }
                let eq = i.stats.pnl.to_f64().unwrap_or(0.0);
                i.stats.equity.push_back(EquityPt { t: now, eq });
                crate::ga::record_outcome(&reg.ga, detected_bps, true);
            }
            None => {
                ev = SweepFill {
                    id: fill_id,
                    market,
                    status: "expired".into(),
                    legs: Vec::new(),
                    notional: "0".into(),
                    profit_usd: "0".into(),
                    net_bps: "0".into(),
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
        while i.stats.equity.len() > ARB_EQUITY_WINDOW {
            i.stats.equity.pop_front();
        }
    }
    if ev.status == "filled" {
        tracing::info!(
            "[sweep] fill #{} {}: pnl {} USD ({} bps net, {} venues)",
            ev.id,
            ev.market.label(),
            ev.profit_usd,
            ev.net_bps,
            ev.legs.len()
        );
    }
    reg.broadcast(BcKind::SweepFill, ev.market, &WireEvent::SweepFillEvent { fill: ev });
}

fn stats_lite(i: &Inner) -> SweepStats {
    let avg = if i.stats.fills > 0 {
        i.stats.sum_bps / Decimal::from(i.stats.fills)
    } else {
        Decimal::ZERO
    };
    let avg_v = if i.stats.fills > 0 {
        i.stats.venues_sum / Decimal::from(i.stats.fills)
    } else {
        Decimal::ZERO
    };
    SweepStats {
        sweeps_tracked: i.stats.sweeps_tracked,
        fills: i.stats.fills,
        expired: i.stats.expired,
        pnl_usd: i.stats.pnl.round_dp(2).normalize().to_string(),
        best_net_bps: i.stats.best_bps.round_dp(2).normalize().to_string(),
        avg_net_bps: avg.round_dp(2).normalize().to_string(),
        avg_venues: avg_v.round_dp(1).normalize().to_string(),
        eq_now: i.stats.pnl.to_f64().unwrap_or(0.0),
        open_plans: i.tracked.len(),
        equity: Vec::new(),
    }
}

/// Live plans, best net edge first (top 4 — one per market, deduped).
fn live_plans(i: &Inner) -> Vec<SweepPlan> {
    let mut plans: Vec<SweepPlan> = i.tracked.values().map(|t| t.plan.clone()).collect();
    plans.sort_by(|a, b| {
        b.net_bps
            .parse::<Decimal>()
            .unwrap_or(Decimal::ZERO)
            .cmp(&a.net_bps.parse::<Decimal>().unwrap_or(Decimal::ZERO))
    });
    plans.truncate(4);
    plans
}

/// 2 Hz live update broadcast.
pub(crate) fn broadcast_update(reg: &Registry) {
    let (stats, plans) = {
        let mut i = reg.sweep.inner.lock().unwrap();
        // Heartbeat equity point so the curve advances on flat P&L.
        let eq = i.stats.pnl.to_f64().unwrap_or(0.0);
        i.stats.equity.push_back(EquityPt { t: now_ms(), eq });
        while i.stats.equity.len() > ARB_EQUITY_WINDOW {
            i.stats.equity.pop_front();
        }
        (stats_lite(&i), live_plans(&i))
    };
    reg.broadcast(
        BcKind::SweepUpdate,
        Market::Eth,
        &WireEvent::SweepUpdate { stats, plans },
    );
}

/// Full snapshot for new sockets.
pub fn snapshot(reg: &Registry) -> WireEvent {
    let i = reg.sweep.inner.lock().unwrap();
    let mut stats = stats_lite(&i);
    stats.equity = i.stats.equity.iter().copied().collect();
    WireEvent::SweepSnapshot {
        stats,
        plans: live_plans(&i),
        fills: i.fills.iter().cloned().collect(),
    }
}

/// Reset paper-trading stats.
pub fn reset(reg: &Registry) {
    let mut i = reg.sweep.inner.lock().unwrap();
    i.stats = SStats::default();
    i.fills.clear();
    i.tracked.clear();
    drop(i);
    reg.broadcast(BcKind::SweepSnapshot, Market::Eth, &snapshot(reg));
}
