//! Genetic algorithm that evolves the arbitrage engine's parameters online
//! to maximize simulated paper P&L.
//!
//! ## What it optimizes
//!
//! Genome (7 real genes, bounded):
//! 1. `fire_edge_bps` — minimum net edge to execute (bps)
//! 2. `min_edge_bps`  — listing threshold (bps)
//! 3. `latency_ms`    — simulated execution latency (models adverse selection)
//! 4. `cooldown_ms`   — per-route cooldown between fires
//! 5. `max_notional`  — capital per fill (USD)
//! 6. `eth_w` / 7. `sol_w` — capital weights per market (Kelly-flavoured)
//!
//! ## Fitness — replay simulation against the live stream
//!
//! The recorder keeps a rolling window of every opportunity the 2-leg engine
//! observes (~1 Hz per route). Fitness(genome) replays that window in time
//! order: a recorded opportunity fires iff its edge >= fire_edge and the
//! route cooldown elapsed; the expected profit is the recorded executable
//! profit, scaled by the notional cap and market weight, times an
//! **empirically calibrated latency-survival rate** — measured from the
//! engine's own fill/expired outcomes bucketed by detected edge (real
//! adverse-selection statistics, not a made-up curve). Longer simulated
//! latencies discount survival exponentially. A small churn penalty
//! (fee-drag equivalent) discourages over-firing.
//!
//! ## Evolution
//!
//! Steady-state GA: population 48, tournament selection (k=3), BLX-α
//! crossover (α=0.5), per-gene Gaussian mutation with adaptive sigma,
//! elitism (2), random immigrants on stagnation (8 generations). One
//! generation every 30 s; the best genome hot-applies to the live engine
//! every 5 generations when it beats the current parameters' fitness.

use crate::state::{now_ms, BcKind, Registry};
use ob_core::{ArbConfig, GaGenome, GaState, Market, Venue, WireEvent};
use rust_decimal::Decimal;
use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// Population size.
const POP: usize = 48;

/// Recording window (observations; ~1 Hz per active route).
const OBS_WINDOW: usize = 20_000;

/// Fitness history cap (generations).
const HIST_CAP: usize = 240;

/// Edge buckets for survival calibration, bps lower bounds.
const BUCKETS: [f64; 5] = [0.0, 2.0, 4.0, 8.0, 16.0];

/// Generations between hot-applies.
const APPLY_EVERY: u64 = 5;

// ---------------------------------------------------------------------------
// Genome
// ---------------------------------------------------------------------------

/// Bounded gene vector <-> GaGenome.
#[derive(Clone)]
pub(crate) struct Genes {
    fire_edge: f64, // 2..30 bps
    min_edge: f64,  // 0..fire bps
    latency: f64,   // 50..2000 ms
    cooldown: f64,  // 250..30000 ms
    notional: f64,  // 500..50000 USD
    eth_w: f64,     // 0.05..1
    sol_w: f64,     // 0.05..1
}

const BOUNDS: [(f64, f64); 7] = [
    (2.0, 30.0),
    (0.0, 28.0),
    (50.0, 2000.0),
    (250.0, 30_000.0),
    (500.0, 50_000.0),
    (0.05, 1.0),
    (0.05, 1.0),
];

impl Genes {
    fn to_vec(&self) -> Vec<f64> {
        vec![
            self.fire_edge,
            self.min_edge,
            self.latency,
            self.cooldown,
            self.notional,
            self.eth_w,
            self.sol_w,
        ]
    }

    fn from_vec(v: &[f64]) -> Self {
        Self {
            fire_edge: v[0],
            min_edge: v[1],
            latency: v[2],
            cooldown: v[3],
            notional: v[4],
            eth_w: v[5],
            sol_w: v[6],
        }
    }

    fn clamp(&mut self) {
        let mut g = self.to_vec();
        for (i, (lo, hi)) in BOUNDS.iter().enumerate() {
            g[i] = g[i].clamp(*lo, *hi);
        }
        // min_edge must stay below fire_edge.
        g[1] = g[1].min(g[0] - 0.5);
        *self = Self::from_vec(&g);
    }

    fn genome(&self) -> GaGenome {
        GaGenome {
            fire_edge_bps: format!("{:.2}", self.fire_edge),
            min_edge_bps: format!("{:.2}", self.min_edge),
            latency_ms: self.latency.round() as u64,
            cooldown_ms: self.cooldown.round() as u64,
            max_notional_usd: format!("{:.0}", self.notional),
            eth_w: format!("{:.3}", self.eth_w),
            sol_w: format!("{:.3}", self.sol_w),
        }
    }

    fn from_genome(g: &GaGenome) -> Self {
        let p = |s: &str| s.parse::<f64>().unwrap_or(0.0);
        let mut s = Self {
            fire_edge: p(&g.fire_edge_bps),
            min_edge: p(&g.min_edge_bps),
            latency: g.latency_ms as f64,
            cooldown: g.cooldown_ms as f64,
            notional: p(&g.max_notional_usd),
            eth_w: p(&g.eth_w),
            sol_w: p(&g.sol_w),
        };
        s.clamp();
        s
    }

    /// Current engine parameters as a genome (fitness baseline).
    fn from_config(cfg: &ArbConfig) -> Self {
        Self::from_genome(&GaGenome {
            fire_edge_bps: cfg.fire_edge_bps.clone(),
            min_edge_bps: cfg.min_edge_bps.clone(),
            latency_ms: cfg.latency_ms,
            cooldown_ms: cfg.cooldown_ms,
            max_notional_usd: cfg.max_notional_usd.clone(),
            eth_w: "0.5".into(),
            sol_w: "0.5".into(),
        })
    }

    /// Jitter around a base genome (population seeding).
    fn jitter(base: &Self, rng: &mut Pcg, amount: f64) -> Self {
        let mut g = base.to_vec();
        for (i, (lo, hi)) in BOUNDS.iter().enumerate() {
            let span = hi - lo;
            g[i] = (g[i] + rng.next_normal() * span * amount).clamp(*lo, *hi);
        }
        let mut out = Self::from_vec(&g);
        out.clamp();
        out
    }
}

// ---------------------------------------------------------------------------
// Tiny deterministic RNG (PCG32) — no external deps, reproducible runs.
// ---------------------------------------------------------------------------

pub struct Pcg {
    state: u64,
    inc: u64,
}

impl Pcg {
    pub fn new(seed: u64) -> Self {
        Self {
            state: seed.wrapping_add(0x853c_49e6_748f_ea9d),
            inc: seed.wrapping_mul(0xda3e_39cb_94b9_5bdb) | 1,
        }
    }

    fn next_u32(&mut self) -> u32 {
        let old = self.state;
        self.state = old
            .wrapping_mul(6364136223846793005)
            .wrapping_add(self.inc);
        let xorshifted = (((old >> 18) ^ old) >> 27) as u32;
        let rot = (old >> 59) as u32;
        xorshifted.rotate_right(rot)
    }

    fn next_f64(&mut self) -> f64 {
        self.next_u32() as f64 / u32::MAX as f64
    }

    fn next_normal(&mut self) -> f64 {
        // Box-Muller.
        let u1 = self.next_f64().max(1e-12);
        let u2 = self.next_f64();
        (-2.0 * u1.ln()).sqrt() * (2.0 * std::f64::consts::PI * u2).cos()
    }
}

// ---------------------------------------------------------------------------
// Recorder + survival calibration
// ---------------------------------------------------------------------------

/// One recorded opportunity observation (2-leg engine).
#[derive(Clone, Copy)]
struct Obs {
    t: u64,
    route: u16, // dense route id
    market: u8, // 0 = ETH, 1 = SOL
    net_bps: f64,
    notional: f64,
    profit: f64,
}

struct SurvBucket {
    fired: u64,
    survived: u64,
}

pub struct GaEngine {
    inner: Mutex<Inner>,
}

struct Inner {
    enabled: bool,
    generation: u64,
    pop: Vec<Genes>,
    fitness: Vec<f64>,
    best_idx: usize,
    best_fit: f64,
    best: Genes,
    current_fit: f64,
    applied_gen: u64,
    history: VecDeque<(u64, f64)>,
    obs: VecDeque<Obs>,
    routes: HashMap<(u8, u8, u8), u16>, // (market, buy, sell) -> dense id
    route_names: Vec<String>,
    survival: Vec<SurvBucket>,
    stagnation: u32,
    rng: Pcg,
}

impl GaEngine {
    pub fn new() -> Self {
        let mut rng = Pcg::new(0x9E37_79B9_7F4A_7C15u64 ^ now_ms());
        let base = Genes::from_config(&ArbConfig::default());
        let mut pop = vec![base.clone()];
        for _ in 1..POP {
            pop.push(Genes::jitter(&base, &mut rng, 0.25));
        }
        let fitness = vec![f64::NEG_INFINITY; POP];
        Self {
            inner: Mutex::new(Inner {
                enabled: true,
                generation: 0,
                pop,
                fitness,
                best_idx: 0,
                best_fit: f64::NEG_INFINITY,
                best: base,
                current_fit: f64::NEG_INFINITY,
                applied_gen: 0,
                history: VecDeque::new(),
                obs: VecDeque::new(),
                routes: HashMap::new(),
                route_names: Vec::new(),
                survival: BUCKETS.iter().map(|_| SurvBucket { fired: 0, survived: 0 }).collect(),
                stagnation: 0,
                rng,
            }),
        }
    }

    pub fn set_enabled(&self, on: bool) {
        self.inner.lock().unwrap().enabled = on;
    }

    pub fn is_enabled(&self) -> bool {
        self.inner.lock().unwrap().enabled
    }

    /// Current generation counter (health/metrics).
    pub fn generation(&self) -> u64 {
        self.inner.lock().unwrap().generation
    }

    /// Current best market weights (used by the engines for notional sizing).
    pub fn weights(&self) -> (f64, f64) {
        let i = self.inner.lock().unwrap();
        (i.best.eth_w, i.best.sol_w)
    }
}

/// Record a 2-leg opportunity observation (called ~1 Hz per route).
pub(crate) fn record_obs(
    ga: &GaEngine,
    t: u64,
    market: Market,
    buy: Venue,
    sell: Venue,
    net_bps: Decimal,
    notional: Decimal,
    profit: Decimal,
) {
    let mut i = ga.inner.lock().unwrap();
    let m = match market {
        Market::Eth => 0u8,
        _ => 1u8,
    };
    let key = (m, buy.index() as u8, sell.index() as u8);
    let route = match i.routes.get(&key) {
        Some(r) => *r,
        None => {
            let r = i.route_names.len() as u16;
            i.route_names.push(format!("{}>{}>{}", market.short(), buy.short(), sell.short()));
            i.routes.insert(key, r);
            r
        }
    };
    i.obs.push_back(Obs {
        t,
        route,
        market: m,
        net_bps: net_bps.to_f64().unwrap_or(0.0),
        notional: notional.to_f64().unwrap_or(0.0),
        profit: profit.to_f64().unwrap_or(0.0),
    });
    while i.obs.len() > OBS_WINDOW {
        i.obs.pop_front();
    }
}

/// Empirical survival probability of a detected edge (0..1): the fraction
/// of fired attempts whose edge survived the simulated latency window,
/// Beta-smoothed per edge bucket. Used by all three execution engines as
/// the EV gate (`profit x survival > churn floor`).
pub fn survival_rate(ga: &GaEngine, bps: f64) -> f64 {
    let i = ga.inner.lock().unwrap();
    survival_rate_inner(&i, bps)
}

/// Record a fired attempt's outcome for survival calibration.
pub(crate) fn record_outcome(ga: &GaEngine, detected_bps: f64, survived: bool) {
    let mut i = ga.inner.lock().unwrap();
    let b = bucket_of(detected_bps);
    i.survival[b].fired += 1;
    if survived {
        i.survival[b].survived += 1;
    }
}

fn bucket_of(bps: f64) -> usize {
    let mut b = 0;
    for (i, lo) in BUCKETS.iter().enumerate() {
        if bps >= *lo {
            b = i;
        }
    }
    b
}

/// Survival rate for an edge, blended toward 0.5 with a weak Beta prior
/// (2 pseudo-observations) so early statistics don't swing wildly.
fn survival_rate_inner(i: &Inner, bps: f64) -> f64 {
    let b = bucket_of(bps);
    let s = &i.survival[b];
    if s.fired == 0 {
        // No data for this bucket: interpolate from global stats.
        let fired: u64 = i.survival.iter().map(|x| x.fired).sum();
        let lived: u64 = i.survival.iter().map(|x| x.survived).sum();
        if fired == 0 {
            return 0.35; // prior: most latency-window edges vanish
        }
        return (lived as f64) / (fired as f64);
    }
    (s.survived as f64 + 1.0) / (s.fired as f64 + 2.0)
}

// ---------------------------------------------------------------------------
// Fitness: replay simulation
// ---------------------------------------------------------------------------

fn fitness_of(i: &Inner, g: &Genes) -> f64 {
    let mut last_fire: HashMap<u16, u64> = HashMap::new();
    let mut pnl = 0.0f64;
    let mut _fires = 0u32;
    for o in &i.obs {
        if o.net_bps < g.fire_edge {
            continue;
        }
        if let Some(&lf) = last_fire.get(&o.route) {
            if o.t.saturating_sub(lf) < g.cooldown as u64 {
                continue;
            }
        }
        last_fire.insert(o.route, o.t);
        _fires += 1;
        // Notional cap + market weight.
        let w = if o.market == 0 { g.eth_w } else { g.sol_w };
        let cap = g.notional * (w * 2.0).min(1.0);
        let scale = if o.notional > 0.0 {
            (cap / o.notional).min(1.0)
        } else {
            0.0
        };
        // Latency discount relative to the engine's calibration point.
        let surv = survival_rate_inner(i, o.net_bps);
        let latency_factor = (-(g.latency - 250.0).max(0.0) / 1500.0).exp().clamp(0.2, 1.0);
        // Churn penalty: 0.5 bps of deployed notional per fire.
        let churn = 0.00005 * cap;
        pnl += o.profit * scale * surv * latency_factor - churn;
    }
    pnl
}

// ---------------------------------------------------------------------------
// Evolution
// ---------------------------------------------------------------------------

fn tournament(i: &Inner, rng: &mut Pcg, k: usize) -> usize {
    let mut best = (rng.next_f64() * POP as f64) as usize % POP;
    for _ in 1..k {
        let c = (rng.next_f64() * POP as f64) as usize % POP;
        if i.fitness[c] > i.fitness[best] {
            best = c;
        }
    }
    best
}

fn blx(a: &[f64], b: &[f64], rng: &mut Pcg, alpha: f64) -> Vec<f64> {
    (0..a.len())
        .map(|j| {
            let lo = a[j].min(b[j]) - alpha * (a[j] - b[j]).abs();
            let hi = a[j].max(b[j]) + alpha * (a[j] - b[j]).abs();
            lo + rng.next_f64() * (hi - lo)
        })
        .collect()
}

/// Run one generation. Called every 30 s from the GA loop task.
pub(crate) fn evolve(reg: &Arc<Registry>) {
    let mut i = reg.ga.inner.lock().unwrap();

    // Snapshot observations for fitness (cheap: Arc-free copy of refs is not
    // possible under the lock, so evaluate in place).
    let cur_cfg = reg.arb.inner_cfg();
    let current = Genes::from_config(&cur_cfg);
    i.current_fit = fitness_of(&i, &current);

    // Initial evaluation.
    if i.generation == 0 {
        for gi in 0..i.pop.len() {
            let g = i.pop[gi].clone();
            let f = fitness_of(&i, &g);
            i.fitness[gi] = f;
        }
    }

    // Track the previous best for stagnation detection.
    let prev_best = i.best_fit;

    // Steady-state replacement: generate POP-2 children (keep 2 elites).
    let mut children: Vec<Genes> = Vec::with_capacity(POP);
    // Elitism.
    let mut order: Vec<usize> = (0..POP).collect();
    order.sort_by(|&a, &b| i.fitness[b].partial_cmp(&i.fitness[a]).unwrap_or(std::cmp::Ordering::Equal));
    children.push(i.pop[order[0]].clone());
    children.push(i.pop[order[1 % POP]].clone());

    let mut rng = std::mem::replace(&mut i.rng, Pcg::new(1));
    let sigma = if i.stagnation > 8 { 0.25 } else { 0.10 };
    while children.len() < POP {
        // Random immigrants on stagnation.
        if i.stagnation > 8 && rng.next_f64() < 0.08 {
            children.push(Genes::jitter(&i.best, &mut rng, 0.35));
            continue;
        }
        let pa = tournament(&i, &mut rng, 3);
        let pb = tournament(&i, &mut rng, 3);
        let (va, vb) = (i.pop[pa].to_vec(), i.pop[pb].to_vec());
        let mut child = blx(&va, &vb, &mut rng, 0.5);
        // Gaussian mutation, per gene, adaptive sigma.
        for (gi, (lo, hi)) in BOUNDS.iter().enumerate() {
            if rng.next_f64() < 0.25 {
                child[gi] += rng.next_normal() * (hi - lo) * sigma;
            }
        }
        let mut g = Genes::from_vec(&child);
        g.clamp();
        children.push(g);
    }
    i.rng = rng;

    // Evaluate the new population.
    for gi in 0..children.len() {
        let g = children[gi].clone();
        let f = fitness_of(&i, &g);
        i.fitness[gi] = f;
    }
    i.pop = children;

    // Best tracking.
    let (bi, bf) = i
        .fitness
        .iter()
        .enumerate()
        .max_by(|a, b| a.1.partial_cmp(b.1).unwrap_or(std::cmp::Ordering::Equal))
        .map(|(a, b)| (a, *b))
        .unwrap_or((0, f64::NEG_INFINITY));
    i.best_idx = bi;
    if bf > i.best_fit || i.best_fit.is_infinite() {
        i.best_fit = bf;
        i.best = i.pop[bi].clone();
    }
    if i.best_fit <= prev_best {
        i.stagnation += 1;
    } else {
        i.stagnation = 0;
    }

    i.generation += 1;
    let gen = i.generation;
    let best_fit = i.best_fit;
    i.history.push_back((gen, best_fit));
    while i.history.len() > HIST_CAP {
        i.history.pop_front();
    }

    let gen = i.generation;
    let enabled = i.enabled;
    let best = i.best.clone();
    let best_fit = i.best_fit;
    let cur_fit = i.current_fit;
    drop(i);

    // Hot-apply the best genome to the live engine periodically.
    if enabled && gen % APPLY_EVERY == 0 && best_fit > cur_fit {
        apply_genome(reg, &best);
        let mut i = reg.ga.inner.lock().unwrap();
        i.applied_gen = gen;
    }
}

/// Apply a genome to the live engine configuration (fires + listing edges +
/// latency + cooldown + notional; market weights feed the engines' sizing).
pub fn apply_genome(reg: &Arc<Registry>, g: &Genes) {
    let upd = ob_core::ArbConfigUpdate {
        enabled: None,
        min_edge_bps: Some(g.genome().min_edge_bps),
        fire_edge_bps: Some(g.genome().fire_edge_bps),
        max_notional_usd: Some(g.genome().max_notional_usd),
        latency_ms: Some(g.genome().latency_ms),
        cooldown_ms: Some(g.genome().cooldown_ms),
        fees_bps: None,
    };
    if let Err(e) = crate::arb::set_config(reg, upd) {
        tracing::debug!("[ga] apply rejected: {e}");
    } else {
        tracing::info!(
            "[ga] applied generation genome: fire {:.1} bps, latency {} ms, cooldown {} ms, notional {:.0} USD",
            g.fire_edge,
            g.latency,
            g.cooldown,
            g.notional
        );
    }
}

/// Background GA loop (one generation every 30 s).
pub async fn run(reg: Arc<Registry>) {
    let mut tick = tokio::time::interval(Duration::from_secs(30));
    tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    tick.tick().await; // skip the immediate first tick
    loop {
        tick.tick().await;
        {
            let i = reg.ga.inner.lock().unwrap();
            if !i.enabled || i.obs.len() < 24 {
                continue;
            }
        }
        evolve(&reg);
        broadcast_update(&reg);
    }
}

/// 0.5 Hz wire update.
pub(crate) fn broadcast_update(reg: &Registry) {
    let state = state_of(reg);
    reg.broadcast(BcKind::GaUpdate, Market::Eth, &WireEvent::GaUpdate { state });
}

/// Force an immediate apply of the current best genome (UI button).
pub fn apply_now(reg: &Arc<Registry>) {
    let best = {
        let i = reg.ga.inner.lock().unwrap();
        i.best.clone()
    };
    apply_genome(reg, &best);
    let mut i = reg.ga.inner.lock().unwrap();
    i.applied_gen = i.generation;
}

/// Reset the evolution (fresh population around the current config).
pub fn reset(reg: &Arc<Registry>) {
    let cur = Genes::from_config(&reg.arb.inner_cfg());
    let mut i = reg.ga.inner.lock().unwrap();
    let mut rng = std::mem::replace(&mut i.rng, Pcg::new(now_ms()));
    i.pop = vec![cur.clone()];
    for _ in 1..POP {
        i.pop.push(Genes::jitter(&cur, &mut rng, 0.25));
    }
    i.fitness = vec![f64::NEG_INFINITY; POP];
    i.best = cur;
    i.best_fit = f64::NEG_INFINITY;
    i.generation = 0;
    i.applied_gen = 0;
    i.stagnation = 0;
    i.history.clear();
    i.rng = rng;
    drop(i);
    broadcast_update(reg);
}

fn state_of(reg: &Registry) -> GaState {
    let i = reg.ga.inner.lock().unwrap();
    GaState {
        enabled: i.enabled,
        generation: i.generation,
        population: i.pop.len(),
        best_fitness: fmt_fit(i.best_fit),
        current_fitness: fmt_fit(i.current_fit),
        best: i.best.genome(),
        applied_gen: i.applied_gen,
        history: i.history.iter().copied().collect(),
        survival: BUCKETS
            .iter()
            .enumerate()
            .map(|(b, lo)| {
                let s = &i.survival[b];
                let rate = if s.fired == 0 {
                    f64::NAN
                } else {
                    s.survived as f64 / s.fired as f64
                };
                let label = if *lo >= 16.0 {
                    format!("{lo:.0}+")
                } else {
                    format!("{lo:.0}-{:.0}", BUCKETS.get(b + 1).copied().unwrap_or(999.0))
                };
                (
                    label,
                    if rate.is_nan() {
                        "\u{2014}".to_string()
                    } else {
                        format!("{:.0}%", rate * 100.0)
                    },
                )
            })
            .collect(),
        observations: i.obs.len(),
    }
}

fn fmt_fit(f: f64) -> String {
    if f.is_infinite() {
        "\u{2014}".into()
    } else {
        format!("{:.2}", f)
    }
}

// Decimal -> f64 helper.
trait ToF64 {
    fn to_f64(&self) -> Option<f64>;
}

impl ToF64 for Decimal {
    fn to_f64(&self) -> Option<f64> {
        rust_decimal::prelude::ToPrimitive::to_f64(self)
    }
}

/// Vendored for the routes module.
pub fn snapshot(reg: &Registry) -> GaState {
    state_of(reg)
}
