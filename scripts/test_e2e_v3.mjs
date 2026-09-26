// E2E v3: full verification of all four arbitrage engines + production endpoints.
// - 9 venue feeds live (books, tapes)
// - 2-leg engine: opportunities, EV-gated fills
// - Global sweep optimizer: snapshot/update/plans/fills
// - Multi-hop cycles: updates + graph telemetry
// - GA: ga_update stream
// - REST: /api/sweep, /metrics (Prometheus), /api/health
// - Graceful shutdown on SIGTERM
import WebSocket from 'ws';
import { spawn } from 'child_process';

const log = (...a) => console.log(...a);
const SERVER = spawn('/home/z/my-project/target/release/hyperlight-server', [], {
  env: { ...process.env, PORT: '3000', RUST_LOG: 'info' },
  stdio: ['ignore', 'pipe', 'pipe'],
});
let serverOut = '';
SERVER.stdout.on('data', (d) => { serverOut += d.toString(); });
SERVER.stderr.on('data', (d) => { serverOut += d.toString(); });
SERVER.on('exit', (code, sig) => log(`[server] exit code=${code} sig=${sig}`));

const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const counts = {};
let bookVenues = new Set();
let arbUpdates = 0;
let arbFills = 0;
let sweepUpdates = 0;
let sweepFills = 0;
let sweepPlans = [];
let cycleUpdates = 0;
let gaUpdates = 0;
let usdtRate = '';
let sawUsdtNon1 = false;
const events = { ticker: 0, trades: 0, arb_snapshot: 0, sweep_snapshot: 0, cycle_snapshot: 0 };

function classify(v) {
  const t = v.type;
  counts[t] = (counts[t] || 0) + 1;
  if (t === 'book') {
    for (const vb of v.venues || []) bookVenues.add(vb.venue);
  } else if (t === 'status') {
    if (v.usdt && v.usdt !== '1') sawUsdtNon1 = true;
    usdtRate = v.usdt;
  } else if (t === 'arb_update') {
    arbUpdates++;
  } else if (t === 'arb_fill_event') {
    arbFills++;
    log(`[2-LEG FILL] #${v.fill.id} ${v.fill.status} ${v.fill.market} ${v.fill.buy_venue}->${v.fill.sell_venue} pnl=${v.fill.pnl_usd} net=${v.fill.net_bps}bps`);
  } else if (t === 'sweep_update') {
    sweepUpdates++;
    sweepPlans = v.plans || [];
    if (sweepPlans.length && sweepUpdates % 5 === 1) {
      const p = sweepPlans[0];
      log(`[SWEEP PLAN] ${p.market} net=${p.net_bps}bps profit=$${p.profit_usd} ev=$${p.ev_usd} legs=${p.legs.map((l) => `${l.side} ${l.venue} $${l.notional}`).join(' + ')}`);
    }
  } else if (t === 'sweep_fill_event') {
    sweepFills++;
    log(`[SWEEP FILL] #${v.fill.id} ${v.fill.status} ${v.fill.market} pnl=${v.fill.profit_usd} net=${v.fill.net_bps}bps legs=${v.fill.legs.length}`);
  } else if (t === 'cycle_update') {
    cycleUpdates++;
  } else if (t === 'ga_update') {
    gaUpdates++;
  } else if (t === 'ticker') events.ticker++;
  else if (t === 'trades') {
    events.trades++;
    for (const tr of v.trades || []) bookVenues.add(tr.venue);
  } else if (t === 'arb_snapshot') events.arb_snapshot++;
  else if (t === 'sweep_snapshot') events.sweep_snapshot++;
  else if (t === 'cycle_snapshot') events.cycle_snapshot++;
}

let ws;
async function main() {
  await sleep(1500);

  // REST smoke.
  const health = await fetch('http://localhost:3000/api/health').then((r) => r.json());
  log(`[rest] health keys: ${Object.keys(health).join(',')} usdt=${health.usdt_usd} arb=${JSON.stringify(health.arb)} sweep=${JSON.stringify(health.sweep)}`);

  ws = new WebSocket('ws://localhost:3000/ws');
  ws.on('open', () => log('[ws] open'));
  ws.on('message', (d) => classify(JSON.parse(d.toString())));

  // Phase 1: wait for venues + engines to come live.
  await sleep(22000);
  log('\n===== PHASE 1 (22s, default config) =====');
  log(`events: ${JSON.stringify(counts)}`);
  log(`book venues: ${[...bookVenues].sort().join(', ')} (${bookVenues.size} total)`);
  log(`usdt rate: ${usdtRate} (non-1: ${sawUsdtNon1})`);
  log(`2-leg updates=${arbUpdates} | sweep updates=${sweepUpdates} plans=${sweepPlans.length} | cycle updates=${cycleUpdates} | ga updates=${gaUpdates}`);
  if (sweepPlans.length) {
    const p = sweepPlans[0];
    log(`top sweep plan: ${p.market} net=${p.net_bps}bps profit=$${p.profit_usd} ev=$${p.ev_usd} notional=$${p.notional} legs=${p.legs.length}`);
  }

  // Phase 2: zero-fee sim mode -> exercise all fill pipelines.
  ws.send(JSON.stringify({
    type: 'arb_config',
    fire_edge_bps: '0.5',
    min_edge_bps: '0.5',
    latency_ms: 100,
    cooldown_ms: 700,
    fees_bps: [
      ['hyperliquid', '0'], ['lighter', '0'], ['binance', '0'], ['bybit', '0'],
      ['okx', '0'], ['kraken', '0'], ['coinbase', '0'], ['bitstamp', '0'], ['gate', '0'],
    ],
  }));
  log('\n[ws] arb_config: fire=0.5bps, ALL FEES=0 (sim), latency=100ms, cooldown=700ms');
  await sleep(18000);
  log('\n===== PHASE 2 (zero-fee sim: fill pipelines) =====');
  log(`events: ${JSON.stringify(counts)}`);
  log(`2-leg fills=${arbFills} | sweep fills=${sweepFills} | sweep updates=${sweepUpdates}`);

  // Phase 3: restore realistic fees.
  ws.send(JSON.stringify({
    type: 'arb_config',
    fire_edge_bps: '8',
    min_edge_bps: '3',
    fees_bps: [
      ['hyperliquid', '45'], ['lighter', '0'], ['binance', '10'], ['bybit', '10'],
      ['okx', '10'], ['kraken', '26'], ['coinbase', '60'], ['bitstamp', '40'], ['gate', '20'],
    ],
  }));
  log('\n[ws] restored realistic fees');
  await sleep(3000);

  // REST: sweep + metrics + ga + cycles.
  const sweep = await fetch('http://localhost:3000/api/sweep').then((r) => r.json());
  log(`[rest] /api/sweep: type=${sweep.type} fills=${sweep.stats.fills} expired=${sweep.stats.expired} pnl=${sweep.stats.pnl_usd} open=${sweep.stats.open_plans} plans=${sweep.plans.length}`);
  if (sweep.plans.length) {
    const p = sweep.plans[0];
    log(`  plan: ${p.market} ${p.legs.map((l) => `${l.side}:${l.venue.short || l.venue}`).join(',')} net=${p.net_bps}bps profit=$${p.profit_usd}`);
  }
  const metrics = await fetch('http://localhost:3000/metrics').then((r) => r.text());
  const metricNames = [...new Set(metrics.split('\n').filter((l) => l && !l.startsWith('#')).map((l) => l.split(' ').slice(0, 1)))];
  log(`[rest] /metrics: ${metrics.split('\n').filter((l) => l && !l.startsWith('#')).length} samples, families: ${[...new Set(metricNames.map((m) => m[0].split('{')[0]))].join(', ')}`);
  const ga = await fetch('http://localhost:3000/api/ga').then((r) => r.json());
  log(`[rest] /api/ga: gen=${ga.state.generation} obs=${ga.state.observations} pop=${ga.state.population}`);
  const cyc = await fetch('http://localhost:3000/api/cycles').then((r) => r.json());
  log(`[rest] /api/cycles: nodes=${cyc.stats.graph_nodes} edges=${cyc.stats.graph_edges} neg_cycle=${cyc.stats.negative_cycles}`);

  log('\n===== SERVER LOG TAIL =====');
  log(serverOut.split('\n').slice(-18).join('\n'));

  // Graceful shutdown: SIGTERM must produce the shutdown message.
  const ok1 =
    bookVenues.size >= 8 &&
    arbUpdates > 10 &&
    sweepUpdates > 10 &&
    cycleUpdates > 5 &&
    gaUpdates >= 1 &&
    sawUsdtNon1 &&
    events.sweep_snapshot >= 1;
  const ok2 = arbFills >= 1 || sweepFills >= 1;
  log(`\ncheck A (streams): ${ok1 ? 'PASS' : 'FAIL'} venues=${bookVenues.size} arb=${arbUpdates} sweep=${sweepUpdates} cycles=${cycleUpdates} ga=${gaUpdates} usdt=${sawUsdtNon1} sweep_snapshot=${events.sweep_snapshot}`);
  log(`check B (fills): ${ok2 ? 'PASS' : 'FAIL'} 2leg=${arbFills} sweep=${sweepFills}`);

  ws.close();
  await sleep(300);
  SERVER.kill('SIGTERM');
  await sleep(2500);
  const graceful = serverOut.includes('shutdown signal received');
  log(`check C (SIGTERM graceful): ${graceful ? 'PASS' : 'FAIL'}`);
  SERVER.kill('SIGKILL');

  const ok = ok1 && ok2 && graceful;
  log(`\nVERDICT: ${ok ? 'PASS' : 'FAIL'}`);
  process.exit(ok ? 0 : 1);
}

main().catch((e) => {
  log('E2E ERROR:', e.message);
  log(serverOut.split('\n').slice(-15).join('\n'));
  SERVER.kill('SIGKILL');
  process.exit(1);
});
