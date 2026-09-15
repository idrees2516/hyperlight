//! Cross-venue arbitrage engine.
//!
//! Design notes (the "most advanced, most efficient" version that stays
//! honest about being a signal + paper-execution engine):
//!
//! 1. **Exact marginal analysis.** Venue books are piecewise-linear, so the
//!    profit-maximizing executable size between a buy venue and a sell venue
//!    is found by walking both books greedily while
//!    `bid * f_sell * (1 - fee_sell) > ask * f_buy * (1 + fee_buy)`
//!    (`ob_core::arb_walk`). Fees are per-venue taker schedules, live
//!    editable. USDT-quoted venues are normalized to USD through the live
//!    Kraken USDT/USD rate, so no phantom edge from quote basis.
//! 2. **Depth-aware sizing.** The reported size/notional/profit are what the
//!    books can actually absorb — walking past the touch automatically
//!    accounts for slippage; the VWAPs are the true executable ones.
//! 3. **Latency modeling.** When an opportunity exceeds the fire threshold,
//!    the paper executor waits `latency_ms`, then re-walks the CURRENT books.
//!    If the edge survived, it fills at the new (worse) prices; if it
//!    vanished, the attempt is recorded as `expired` — realistic adverse
//!    selection instead of naive backtest fantasy.
//! 4. **Dust + cooldown guards.** Opportunities below the dust floor are
//!    never listed; each route has a cooldown and one in-flight fill.
//! 5. **Full telemetry.** Per-route stats, win/loss, fee drag, equity curve.

use crate::state::{now_ms, BcKind, Registry};
use ob_core::{
    arb_walk, ArbConfig, ArbConfigUpdate, ArbFill, ArbOpportunity, ArbPairStat, ArbStats,
    ArbWalk, EquityPt, Market, Venue, WireEvent, ARB_EQUITY_WINDOW, ARB_FILL_LOG,
    ARB_MIN_NOTIONAL, ARB_OP_TTL_MS,
};
use rust_decimal::prelude::{FromPrimitive, ToPrimitive};
use rust_decimal::Decimal;
use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// Identity of one route: (market, buy venue, sell venue).
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct OpKey {
    pub market: Market,
    pub buy: Venue,
    pub sell: Venue,
}

struct TrackedOp {
    op: ArbOpportunity,
    last_seen: u64,
    last_fire: u64,
    in_flight: bool,
}

#[derive(Default)]
struct PairStatInner {
    fills: u64,
    pnl: Decimal,
    best_bps: Decimal,
}

struct Stats {
    ops_tracked: u64,
    fills: u64,
    expired: u64,
    wins: u64,
    losses: u64,
    pnl: Decimal,
    fees: Decimal,
    best_bps: Decimal,
    sum_net_bps: Decimal,
    equity: VecDeque<EquityPt>,
}

impl Default for Stats {
    fn default() -> Self {
        Self {
            ops_tracked: 0,
            fills: 0,
            expired: 0,
            wins: 0,
            losses: 0,
            pnl: Decimal::ZERO,
            fees: Decimal::ZERO,
            best_bps: Decimal::ZERO,
            sum_net_bps: Decimal::ZERO,
            equity: VecDeque::new(),
        }
    }
}

/// The engine state (behind one mutex; always taken AFTER the books lock).
pub struct ArbEngine {
    inner: Mutex<Inner>,
}

struct Inner {
    config: ArbConfig,
    tracked: HashMap<OpKey, TrackedOp>,
    fills: VecDeque<ArbFill>,
    stats: Stats,
    pairs: HashMap<OpKey, PairStatInner>,
    next_fill_id: u64,
}

impl ArbEngine {
    pub fn new() -> Self {
        Self {
            inner: Mutex::new(Inner {
                config: ArbConfig::default(),
                tracked: HashMap::new(),
                fills: VecDeque::new(),
                stats: Stats::default(),
                pairs: HashMap::new(),
                next_fill_id: 1,
            }),
        }
    }

    /// Compact JSON for /api/health.
    pub fn health_json(&self) -> serde_json::Value {
        let i = self.inner.lock().unwrap();
        serde_json::json!({
            "enabled": i.config.enabled,
            "open_opportunities": i.tracked.len(),
            "fills": i.stats.fills,
            "expired": i.stats.expired,
            "pnl_usd": i.stats.pnl.normalize().to_string(),
        })
    }
}

/// Parsed, ready-to-use view of the config for one scan pass.
struct ScanCfg {
    enabled: bool,
    min_edge: Decimal,
    fire_edge: Decimal,
    max_notional: Decimal,
    latency_ms: u64,
    cooldown_ms: u64,
    fees: HashMap<Venue, Decimal>, // as decimals (0.001 = 10 bps)
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

/// One scan pass (called at 10 Hz from the publish loop).
pub(crate) async fn tick(reg: &Arc<Registry>) {
    let (cfg, now) = {
        let i = reg.arb.inner.lock().unwrap();
        (parse_cfg(&i.config), now_ms())
    };
    let dust = Decimal::from_f64(ARB_MIN_NOTIONAL).unwrap_or_else(|| Decimal::from(100u64));

    // Collect opportunities under the books lock (walks are microseconds).
    let mut observed: Vec<(OpKey, ArbWalk)> = Vec::new();
    {
        let books = reg.books.lock().unwrap();
        for market in Market::ARB {
            let Some(mb) = books.get(&market) else { continue };
            let sourced = reg.sourced_of(mb);
            if sourced.len() < 2 {
                continue;
            }
            for b in &sourced {
                for s in &sourced {
                    if b.venue == s.venue {
                        continue;
                    }
                    let Some(f_buy) = cfg.fees.get(&b.venue).copied() else { continue };
                    let Some(f_sell) = cfg.fees.get(&s.venue).copied() else { continue };
                    let Some(w) = arb_walk(
                        b.state,
                        s.state,
                        b.factor,
                        s.factor,
                        f_buy,
                        f_sell,
                        cfg.max_notional,
                    ) else {
                        continue;
                    };
                    if w.cost < dust {
                        continue;
                    }
                    let key = OpKey { market, buy: b.venue, sell: s.venue };
                    observed.push((key, w));
                }
            }
        }
    }

    // Update tracked state, fire, prune.
    let mut fire: Vec<(OpKey, ArbWalk)> = Vec::new();
    {
        let mut i = reg.arb.inner.lock().unwrap();
        for (key, w) in observed {
            if w.net_bps < cfg.min_edge || w.profit <= Decimal::ZERO {
                continue;
            }
            if !i.tracked.contains_key(&key) {
                i.stats.ops_tracked += 1;
                i.tracked.insert(
                    key,
                    TrackedOp {
                        op: op_from(key, &w, now),
                        last_seen: now,
                        last_fire: 0,
                        in_flight: false,
                    },
                );
            }
            let entry = i.tracked.get_mut(&key).expect("just inserted");
            entry.op = op_from(key, &w, now);
            entry.last_seen = now;

            let can_fire = cfg.enabled
                && cfg.fire_edge > Decimal::ZERO
                && w.net_bps >= cfg.fire_edge
                && !entry.in_flight
                && now.saturating_sub(entry.last_fire) >= cfg.cooldown_ms;
            if can_fire {
                entry.in_flight = true;
                entry.last_fire = now;
                fire.push((key, w));
            }
        }
        // Prune stale routes.
        i.tracked
            .retain(|_, t| now.saturating_sub(t.last_seen) < ARB_OP_TTL_MS);
    }

    // Schedule paper executions with simulated latency.
    for (key, w) in fire {
        let latency = cfg.latency_ms;
        tracing::info!(
            "[arb] firing {} buy {} @ {} -> sell {} @ {} (net {} bps, {} USD), exec in {latency}ms",
            key.market.label(),
            key.buy.label(),
            w.buy_vwap,
            key.sell.label(),
            w.sell_vwap,
            w.net_bps,
            w.profit
        );
        let reg2 = Arc::clone(reg);
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(latency.max(1))).await;
            execute(reg2, key);
        });
    }
}

fn op_from(key: OpKey, w: &ArbWalk, now: u64) -> ArbOpportunity {
    ArbOpportunity {
        market: key.market,
        buy_venue: key.buy,
        sell_venue: key.sell,
        buy_px: w.buy_vwap.normalize().to_string(),
        sell_px: w.sell_vwap.normalize().to_string(),
        buy_touch: w.buy_touch.normalize().to_string(),
        sell_touch: w.sell_touch.normalize().to_string(),
        size: w.size.normalize().to_string(),
        notional: w.cost.normalize().to_string(),
        gross_bps: w.gross_bps.normalize().to_string(),
        fee_bps: w.fee_bps.normalize().to_string(),
        net_bps: w.net_bps.normalize().to_string(),
        profit_usd: w.profit.normalize().to_string(),
        ts: now,
    }
}

/// Execute a scheduled paper fill against the CURRENT books (after the
/// simulated latency). If the edge vanished, record an expiry.
fn execute(reg: Arc<Registry>, key: OpKey) {
    let (cfg, fill_id, now) = {
        let mut i = reg.arb.inner.lock().unwrap();
        let id = i.next_fill_id;
        i.next_fill_id += 1;
        (parse_cfg(&i.config), id, now_ms())
    };

    // Re-walk the live books.
    let mut walked: Option<ArbWalk> = None;
    {
        let books = reg.books.lock().unwrap();
        if let Some(mb) = books.get(&key.market) {
            let buy_state = mb.venues[key.buy.index()].as_ref();
            let sell_state = mb.venues[key.sell.index()].as_ref();
            if let (Some(bs), Some(ss)) = (buy_state, sell_state) {
                let f_buy = reg.venue_factor(key.buy);
                let f_sell = reg.venue_factor(key.sell);
                let fee_buy = cfg.fees.get(&key.buy).copied().unwrap_or(Decimal::ZERO);
                let fee_sell = cfg.fees.get(&key.sell).copied().unwrap_or(Decimal::ZERO);
                if let Some(w) = arb_walk(bs, ss, f_buy, f_sell, fee_buy, fee_sell, cfg.max_notional) {
                    if w.profit > Decimal::ZERO {
                        walked = Some(w);
                    }
                }
            }
        }
    }

    let ev: Option<ArbFill>;
    {
        let mut i = reg.arb.inner.lock().unwrap();
        if let Some(t) = i.tracked.get_mut(&key) {
            t.in_flight = false;
        }
        match &walked {
            Some(w) => {
                let fill = ArbFill {
                    id: fill_id,
                    market: key.market,
                    buy_venue: key.buy,
                    sell_venue: key.sell,
                    buy_px: w.buy_vwap.normalize().to_string(),
                    sell_px: w.sell_vwap.normalize().to_string(),
                    size: w.size.normalize().to_string(),
                    notional: w.cost.normalize().to_string(),
                    fees_usd: w.fees.normalize().to_string(),
                    pnl_usd: w.profit.normalize().to_string(),
                    net_bps: w.net_bps.normalize().to_string(),
                    status: "filled".into(),
                    detected_net_bps: i
                        .tracked
                        .get(&key)
                        .map(|t| t.op.net_bps.clone())
                        .unwrap_or_else(|| w.net_bps.normalize().to_string()),
                    ts: now,
                };
                i.stats.fills += 1;
                i.stats.pnl += w.profit;
                i.stats.fees += w.fees;
                i.stats.sum_net_bps += w.net_bps;
                if w.profit > Decimal::ZERO {
                    i.stats.wins += 1;
                } else {
                    i.stats.losses += 1;
                }
                if w.net_bps > i.stats.best_bps {
                    i.stats.best_bps = w.net_bps;
                }
                let p = i.pairs.entry(key).or_default();
                p.fills += 1;
                p.pnl += w.profit;
                if w.net_bps > p.best_bps {
                    p.best_bps = w.net_bps;
                }
                i.fills.push_back(fill.clone());
                while i.fills.len() > ARB_FILL_LOG {
                    i.fills.pop_front();
                }
                let eq = i.stats.pnl.to_f64().unwrap_or(0.0);
                i.stats.equity.push_back(EquityPt { t: now, eq });
                ev = Some(fill);
            }
            None => {
                i.stats.expired += 1;
                let fill = ArbFill {
                    id: fill_id,
                    market: key.market,
                    buy_venue: key.buy,
                    sell_venue: key.sell,
                    buy_px: "0".into(),
                    sell_px: "0".into(),
                    size: "0".into(),
                    notional: "0".into(),
                    fees_usd: "0".into(),
                    pnl_usd: "0".into(),
                    net_bps: "0".into(),
                    status: "expired".into(),
                    detected_net_bps: i
                        .tracked
                        .get(&key)
                        .map(|t| t.op.net_bps.clone())
                        .unwrap_or_default(),
                    ts: now,
                };
                i.fills.push_back(fill.clone());
                while i.fills.len() > ARB_FILL_LOG {
                    i.fills.pop_front();
                }
                ev = Some(fill);
            }
        }
        while i.stats.equity.len() > ARB_EQUITY_WINDOW {
            i.stats.equity.pop_front();
        }
    }
    if let Some(fill) = ev {
        if fill.status == "filled" {
            tracing::info!(
                "[arb] fill #{} {} buy {} @ {} -> sell {} @ {}: pnl {} USD ({} bps net)",
                fill.id,
                fill.market.label(),
                fill.buy_venue.label(),
                fill.buy_px,
                fill.sell_venue.label(),
                fill.sell_px,
                fill.pnl_usd,
                fill.net_bps
            );
        }
        reg.broadcast(BcKind::ArbFill, fill.market, &WireEvent::ArbFillEvent { fill });
    }
}

/// Serialize the current scalar stats (no series / pair table).
fn stats_lite(i: &Inner) -> ArbStats {
    let avg = if i.stats.fills > 0 {
        i.stats.sum_net_bps / Decimal::from(i.stats.fills)
    } else {
        Decimal::ZERO
    };
    ArbStats {
        ops_tracked: i.stats.ops_tracked,
        fills: i.stats.fills,
        expired: i.stats.expired,
        wins: i.stats.wins,
        losses: i.stats.losses,
        pnl_usd: i.stats.pnl.normalize().to_string(),
        fees_usd: i.stats.fees.normalize().to_string(),
        best_net_bps: i.stats.best_bps.normalize().to_string(),
        avg_net_bps: avg.round_dp(2).normalize().to_string(),
        eq_now: i.stats.pnl.to_f64().unwrap_or(0.0),
        open_ops: i.tracked.len(),
        equity: Vec::new(),
        pairs: Vec::new(),
    }
}

/// Live opportunities, best net edge first (top 14).
fn live_ops(i: &Inner) -> Vec<ArbOpportunity> {
    let mut ops: Vec<ArbOpportunity> = i.tracked.values().map(|t| t.op.clone()).collect();
    ops.sort_by(|a, b| {
        b.net_bps
            .parse::<Decimal>()
            .unwrap_or(Decimal::ZERO)
            .cmp(&a.net_bps.parse::<Decimal>().unwrap_or(Decimal::ZERO))
    });
    ops.truncate(14);
    ops
}

/// 2 Hz live update broadcast (called from the publish loop).
pub(crate) fn broadcast_update(reg: &Registry) {
    let (opps, stats, now) = {
        let mut i = reg.arb.inner.lock().unwrap();
        // Heartbeat equity point so the curve advances on flat P&L.
        let eq = i.stats.pnl.to_f64().unwrap_or(0.0);
        i.stats.equity.push_back(EquityPt { t: now_ms(), eq });
        while i.stats.equity.len() > ARB_EQUITY_WINDOW {
            i.stats.equity.pop_front();
        }
        (live_ops(&i), stats_lite(&i), now_ms())
    };
    let mut stats = stats;
    stats.equity = vec![EquityPt { t: now, eq: stats.eq_now }];
    reg.broadcast(
        BcKind::ArbUpdate,
        Market::Eth,
        &WireEvent::ArbUpdate {
            opportunities: opps,
            stats,
        },
    );
}

/// Full snapshot for new sockets and config changes.
pub fn snapshot(reg: &Registry) -> WireEvent {
    let i = reg.arb.inner.lock().unwrap();
    let mut stats = stats_lite(&i);
    stats.equity = i.stats.equity.iter().copied().collect();
    stats.pairs = i
        .pairs
        .iter()
        .map(|(k, p)| ArbPairStat {
            market: k.market,
            buy: k.buy,
            sell: k.sell,
            fills: p.fills,
            pnl: p.pnl.normalize().to_string(),
            best_bps: p.best_bps.normalize().to_string(),
        })
        .collect();
    WireEvent::ArbSnapshot {
        config: i.config.clone(),
        stats,
        opportunities: live_ops(&i),
        fills: i.fills.iter().cloned().collect(),
    }
}

/// Apply a partial config update and rebroadcast a full snapshot.
pub fn set_config(reg: &Registry, upd: ArbConfigUpdate) -> Result<(), String> {
    let dec = |s: &str| s.parse::<Decimal>().map_err(|_| format!("bad decimal {s:?}"));
    {
        let mut i = reg.arb.inner.lock().unwrap();
        if let Some(e) = upd.enabled {
            i.config.enabled = e;
        }
        if let Some(v) = upd.min_edge_bps.as_deref() {
            dec(v)?;
            i.config.min_edge_bps = v.to_string();
        }
        if let Some(v) = upd.fire_edge_bps.as_deref() {
            dec(v)?;
            i.config.fire_edge_bps = v.to_string();
        }
        if let Some(v) = upd.max_notional_usd.as_deref() {
            let d = dec(v)?;
            if d <= Decimal::ZERO {
                return Err("max_notional must be positive".into());
            }
            i.config.max_notional_usd = v.to_string();
        }
        if let Some(v) = upd.latency_ms {
            i.config.latency_ms = v.min(10_000);
        }
        if let Some(v) = upd.cooldown_ms {
            i.config.cooldown_ms = v.min(600_000);
        }
        if let Some(fees) = upd.fees_bps {
            for (v, bps) in fees {
                dec(&bps)?;
                if let Some(e) = i.config.fees_bps.iter_mut().find(|(vv, _)| *vv == v) {
                    e.1 = bps;
                }
            }
        }
    }
    reg.broadcast(BcKind::ArbSnapshot, Market::Eth, &snapshot(reg));
    Ok(())
}

/// Reset paper-trading stats (config kept).
pub fn reset(reg: &Registry) {
    let mut i = reg.arb.inner.lock().unwrap();
    i.stats = Stats::default();
    i.fills.clear();
    i.pairs.clear();
    i.tracked.clear();
    drop(i);
    reg.broadcast(BcKind::ArbSnapshot, Market::Eth, &snapshot(reg));
}

/// Notional dust helper for scheduled fills.
#[allow(dead_code)]
fn _unused() {
    let _ = ARB_MIN_NOTIONAL;
}
