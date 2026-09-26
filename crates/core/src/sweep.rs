//! Global sweep optimizer: the exact profit-maximizing taker execution plan
//! across ALL venues simultaneously.
//!
//! ## Why a sweep dominates pairwise walking
//!
//! The 2-leg engine walks one buy venue against one sell venue. But the true
//! optimum may *split*: buy 40% of the size on Binance, 60% on Bybit, and
//! sell across Kraken + Coinbase. Because every venue book is a
//! piecewise-linear supply (asks) or demand (bids) curve, the optimal taker
//! execution is the classic greedy crossing of the two merged curves:
//!
//! 1. Merge every venue's asks into one curve keyed by **effective unit
//!    cost** `c = px * factor * (1 + fee)` (USD, fee included), ascending.
//! 2. Merge every venue's bids into one curve keyed by **effective unit
//!    revenue** `r = px * factor * (1 - fee)` (USD, fee included),
//!    descending.
//! 3. Repeatedly match `min(ask_depth, bid_depth)` units between the
//!    cheapest ask and the richest bid while `r > c`.
//!
//! Every matched pair contributes exactly `q * (r - c)` of *net* profit
//! (fees already included in `r` and `c`), so the matching condition itself
//! is the profitability condition, and the greedy is provably optimal for
//! taker-only access on piecewise-linear books: it exhausts **every**
//! positive marginal pair — no extractable value is left on the table.
//!
//! The notional budget caps deployed cash ex-fees; the marginal match is
//! truncated so total cost never exceeds the cap.
//!
//! Output is a `SweepPlan`: per-(venue, side) coalesced legs with exact
//! decimal VWAPs, sizes, notionals and fees — an atomic multi-venue order
//! plan ready for simultaneous submission.

use crate::{Sourced, Venue};
use rust_decimal::Decimal;
use std::collections::HashMap;

/// Venue levels scanned per side for the merged curves (far beyond any
/// profitable sweep; sweeps typically consume < 10 levels).
const SWEEP_DEPTH: usize = 200;

/// Coalesced per-(venue, side) execution slice.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct SweepLeg {
    pub venue: Venue,
    /// `"buy"` (lift the asks) or `"sell"` (hit the bids).
    pub side: String,
    /// Executable VWAP, USD-normalized, ex-fee.
    pub px: String,
    /// Base size for this leg.
    pub size: String,
    /// USD notional (ex-fee) for this leg.
    pub notional: String,
    /// Taker fee on this leg, USD.
    pub fee_usd: String,
}

/// A complete optimal execution plan for one market (exact decimal strings
/// on the wire).
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct SweepPlan {
    pub market: crate::Market,
    /// Buy legs first, then sell legs.
    pub legs: Vec<SweepLeg>,
    /// Total executable base size.
    pub size: String,
    /// Total deployed cash (buy side, ex-fee), USD.
    pub notional: String,
    /// Total proceeds (sell side, ex-fee), USD.
    pub proceeds: String,
    /// Combined taker fees, USD.
    pub fees_usd: String,
    /// Net profit, USD (proceeds - notional - fees).
    pub profit_usd: String,
    /// Gross edge before fees, bps of deployed notional.
    pub gross_bps: String,
    /// Fee drag, bps.
    pub fee_bps: String,
    /// Net edge after fees, bps.
    pub net_bps: String,
    /// Survival-weighted expected value, USD (see the EV gate).
    pub ev_usd: String,
    pub ts: u64,
}

/// A paper sweep fill (or latency expiry of the whole plan).
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct SweepFill {
    pub id: u64,
    pub market: crate::Market,
    /// `"filled"` or `"expired"`.
    pub status: String,
    pub legs: Vec<SweepLeg>,
    pub notional: String,
    pub profit_usd: String,
    pub net_bps: String,
    /// Net edge in bps at detection time.
    pub detected_net_bps: String,
    pub ts: u64,
}

/// Aggregate sweep-engine stats. `equity` is only populated in full
/// snapshots; updates carry `eq_now` for the live curve.
#[derive(Debug, Clone, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct SweepStats {
    pub sweeps_tracked: u64,
    pub fills: u64,
    pub expired: u64,
    pub pnl_usd: String,
    pub best_net_bps: String,
    pub avg_net_bps: String,
    /// Average number of venues engaged per filled plan.
    pub avg_venues: String,
    /// Live cumulative P&L (curve feed).
    pub eq_now: f64,
    pub open_plans: usize,
    pub equity: Vec<crate::EquityPt>,
}

/// Internal exact-decimal result of the optimizer.
#[derive(Debug, Clone)]
pub struct SweepWalk {
    pub size: Decimal,
    /// Deployed cash, ex-fee, USD.
    pub cost: Decimal,
    /// Proceeds, ex-fee, USD.
    pub proceeds: Decimal,
    pub fees: Decimal,
    pub profit: Decimal,
    pub gross_bps: Decimal,
    pub fee_bps: Decimal,
    pub net_bps: Decimal,
    /// (venue, is_buy, size, raw_usd_notional_ex_fee, fee_usd), buys first.
    pub legs: Vec<(Venue, bool, Decimal, Decimal, Decimal)>,
}

/// One level of a merged executable curve, still venue-attributed.
#[derive(Clone)]
struct CurveLevel {
    /// Effective unit cost (asks) or revenue (bids), fee included, USD.
    eff: Decimal,
    /// Raw ex-fee USD price (px * factor) — cash actually moved.
    raw: Decimal,
    size: Decimal,
    venue: Venue,
}

/// Compute the exact optimal taker sweep across all sourced venues.
///
/// `fees` maps venue -> taker fee as a decimal (0.001 = 10 bps). Returns
/// `None` when no fee-inclusive positive marginal pair exists.
pub fn sweep_optimize(
    sourced: &[Sourced<'_>],
    fees: &HashMap<Venue, Decimal>,
    max_notional: Decimal,
) -> Option<SweepWalk> {
    // 1) Build the merged executable curves.
    let mut asks: Vec<CurveLevel> = Vec::with_capacity(sourced.len() * 32);
    let mut bids: Vec<CurveLevel> = Vec::with_capacity(sourced.len() * 32);
    for s in sourced {
        let fee = fees.get(&s.venue).copied()?;
        let one = Decimal::ONE;
        // Asks: liquidity we BUY. Effective cost = px * f * (1 + fee).
        for (px, sz) in s.state.iter_asks().take(SWEEP_DEPTH) {
            let raw = px * s.factor;
            asks.push(CurveLevel {
                eff: raw * (one + fee),
                raw,
                size: sz,
                venue: s.venue,
            });
        }
        // Bids: liquidity we SELL into. Effective revenue = px * f * (1 - fee).
        for (px, sz) in s.state.iter_bids().take(SWEEP_DEPTH) {
            let raw = px * s.factor;
            bids.push(CurveLevel {
                eff: raw * (one - fee),
                raw,
                size: sz,
                venue: s.venue,
            });
        }
    }
    asks.sort_by(|a, b| a.eff.cmp(&b.eff)); // cheapest first
    bids.sort_by(|a, b| b.eff.cmp(&a.eff)); // richest first

    // 2) Greedy crossing with a two-pointer walk.
    let mut size = Decimal::ZERO;
    let mut cost = Decimal::ZERO; // ex-fee cash out
    let mut proceeds = Decimal::ZERO; // ex-fee cash in
    let mut fees_paid = Decimal::ZERO;
    // (venue, is_buy) -> (size_sum, raw_sum, fee_sum)
    let mut leg_map: HashMap<(Venue, bool), (Decimal, Decimal, Decimal)> = HashMap::new();

    let mut ai = 0usize;
    let mut bi = 0usize;
    let mut ask_rem = asks.first()?.size;
    let mut bid_rem = bids.first()?.size;

    loop {
        let ask = &asks[ai];
        let bid = &bids[bi];
        // Profitability including fees on both sides.
        if bid.eff <= ask.eff {
            break;
        }
        let mut q = ask_rem.min(bid_rem);
        // Cap by the remaining notional budget (cash ex-fee).
        if max_notional > Decimal::ZERO {
            let room = (max_notional - cost) / ask.raw;
            if room <= Decimal::ZERO {
                break;
            }
            if q > room {
                q = room;
            }
        }
        if q <= Decimal::ZERO {
            break;
        }

        let ask_fee = ask.raw * fee_of(fees, ask.venue) * q;
        let bid_fee = bid.raw * fee_of(fees, bid.venue) * q;
        size += q;
        cost += q * ask.raw;
        proceeds += q * bid.raw;
        fees_paid += ask_fee + bid_fee;

        let ae = leg_map.entry((ask.venue, true)).or_default();
        ae.0 += q;
        ae.1 += q * ask.raw;
        ae.2 += ask_fee;
        let be = leg_map.entry((bid.venue, false)).or_default();
        be.0 += q;
        be.1 += q * bid.raw;
        be.2 += bid_fee;

        ask_rem -= q;
        bid_rem -= q;
        if ask_rem <= Decimal::ZERO {
            ai += 1;
            if ai >= asks.len() {
                break;
            }
            ask_rem = asks[ai].size;
        }
        if bid_rem <= Decimal::ZERO {
            bi += 1;
            if bi >= bids.len() {
                break;
            }
            bid_rem = bids[bi].size;
        }
    }

    if size <= Decimal::ZERO {
        return None;
    }
    let profit = proceeds - cost - fees_paid;
    if profit <= Decimal::ZERO {
        return None;
    }
    let gross = proceeds - cost;
    let tenk = Decimal::from(10_000u64);
    let (gross_bps, fee_bps, net_bps) = if cost.is_zero() {
        (Decimal::ZERO, Decimal::ZERO, Decimal::ZERO)
    } else {
        (gross / cost * tenk, fees_paid / cost * tenk, profit / cost * tenk)
    };

    // 3) Coalesce legs: buys first (cheapest first), then sells.
    let mut legs: Vec<(Venue, bool, Decimal, Decimal, Decimal)> = Vec::new();
    for is_buy in [true, false] {
        let mut side_legs: Vec<(Venue, bool, Decimal, Decimal, Decimal)> = leg_map
            .iter()
            .filter(|((_, b), _)| *b == is_buy)
            .map(|((v, _), (sz, raw, fee))| (*v, is_buy, *sz, *raw, *fee))
            .collect();
        side_legs.sort_by(|a, b| b.3.cmp(&a.3).then(a.0.index().cmp(&b.0.index())));
        legs.extend(side_legs);
    }

    Some(SweepWalk {
        size,
        cost,
        proceeds,
        fees: fees_paid,
        profit,
        gross_bps,
        fee_bps,
        net_bps,
        legs,
    })
}

fn fee_of(fees: &HashMap<Venue, Decimal>, v: Venue) -> Decimal {
    fees.get(&v).copied().unwrap_or(Decimal::ZERO)
}

/// Render a `SweepWalk` as a wire `SweepPlan` (rounded for display).
pub fn sweep_plan_of(
    market: crate::Market,
    w: &SweepWalk,
    ev_usd: Decimal,
    now: u64,
) -> SweepPlan {
    let legs = w
        .legs
        .iter()
        .map(|(v, is_buy, sz, raw, fee)| SweepLeg {
            venue: *v,
            side: if *is_buy { "buy".into() } else { "sell".into() },
            px: (raw / sz).round_dp(4).normalize().to_string(),
            size: sz.normalize().to_string(),
            notional: raw.round_dp(2).normalize().to_string(),
            fee_usd: fee.round_dp(4).normalize().to_string(),
        })
        .collect();
    SweepPlan {
        market,
        legs,
        size: w.size.normalize().to_string(),
        notional: w.cost.round_dp(2).normalize().to_string(),
        proceeds: w.proceeds.round_dp(2).normalize().to_string(),
        fees_usd: w.fees.round_dp(4).normalize().to_string(),
        profit_usd: w.profit.round_dp(4).normalize().to_string(),
        gross_bps: w.gross_bps.round_dp(2).normalize().to_string(),
        fee_bps: w.fee_bps.round_dp(2).normalize().to_string(),
        net_bps: w.net_bps.round_dp(2).normalize().to_string(),
        ev_usd: ev_usd.round_dp(4).normalize().to_string(),
        ts: now,
    }
}
