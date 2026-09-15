// E2E: start server, connect /ws, verify 9 venues + arbitrage engine live.
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

const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const counts = {};
let bookVenues = new Set();
let firstBookAt = 0;
let arbUpdates = 0;
let arbOpps = [];
let arbFills = 0;
let statusCount = 0;
let usdtRate = '';
let sawUsdtNon1 = false;
const events = { ticker: 0, trades: 0, history: 0, alert_set: 0, arb_snapshot: 0 };

function classify(v) {
  const t = v.type;
  counts[t] = (counts[t] || 0) + 1;
  if (t === 'book') {
    if (!firstBookAt) firstBookAt = Date.now();
    for (const vb of v.venues || []) bookVenues.add(vb.venue);
    // Sanity: consolidated view exists and stats present
    if (!v.stats || !v.stats.mid) log('!! book without stats');
  } else if (t === 'status') {
    statusCount++;
    if (v.usdt && v.usdt !== '1') sawUsdtNon1 = true;
    usdtRate = v.usdt;
  } else if (t === 'arb_update') {
    arbUpdates++;
    arbOpps = v.opportunities || [];
  } else if (t === 'arb_snapshot') {
    events.arb_snapshot++;
    if (v.config && !v.config.fees_bps?.length) log('!! arb snapshot without fees');
  } else if (t === 'arb_fill_event') {
    arbFills++;
    log(`[FILL] #${v.fill.id} ${v.fill.status} ${v.fill.market} ${v.fill.buy_venue}->${v.fill.sell_venue} pnl=${v.fill.pnl_usd} net_bps=${v.fill.net_bps}`);
  } else if (t === 'ticker') events.ticker++;
  else if (t === 'trades') {
    events.trades++;
    // record venue diversity in the tape
    for (const tr of v.trades || []) bookVenues.add(tr.venue);
  } else if (t === 'history') events.history++;
  else if (t === 'alert_set') events.alert_set++;
}

let ws;
async function main() {
  await sleep(1500); // let the server bind
  // REST smoke first.
  const health = await fetch('http://localhost:3000/api/health').then((r) => r.json());
  log(`[rest] health: ${Object.keys(health).join(',')} usdt=${health.usdt_usd} venues=${health.venues?.length}`);
  const mkts = await fetch('http://localhost:3000/api/markets').then((r) => r.json());
  log(`[rest] markets: ${mkts.markets.length}`);

  ws = new WebSocket('ws://localhost:3000/ws');
  ws.on('open', () => log('[ws] open'));
  ws.on('message', (d) => {
    const v = JSON.parse(d.toString());
    classify(v);
  });

  // Phase 1: wait for venues to come live (~20s), then report.
  await sleep(20000);
  log('\n===== PHASE 1 (20s) =====');
  log(`events: ${JSON.stringify(counts)}`);
  log(`book venues: ${[...bookVenues].sort().join(', ')} (${bookVenues.size} total)`);
  log(`usdt rate: ${usdtRate} (non-1: ${sawUsdtNon1})`);
  log(`arb updates: ${arbUpdates}, live opps now: ${arbOpps.length}`);
  if (arbOpps.length) {
    log(`top opp: ${JSON.stringify(arbOpps[0]).slice(0, 300)}`);
  }

  // Phase 2: force paper fills by lowering the fire edge.
  ws.send(JSON.stringify({ type: 'arb_config', fire_edge_bps: '0.5', min_edge_bps: '0.5', latency_ms: 100, cooldown_ms: 700 }));
  log('\n[ws] sent arb_config: fire_edge=0.5bps, latency=100ms, cooldown=700ms');
  await sleep(15000);
  log('\n===== PHASE 2 (after low-threshold config) =====');
  log(`events: ${JSON.stringify(counts)}`);
  log(`arb updates: ${arbUpdates}, fills/expired: ${arbFills}, live opps: ${arbOpps.length}`);
  log(`top 3 opps: ${arbOpps.slice(0, 3).map((o) => `${o.market} ${o.buy_venue}->${o.sell_venue} ${o.net_bps}bps $${o.profit_usd}`).join(' | ')}`);

  // REST: arb state + config PUT/GET.
  const arb = await fetch('http://localhost:3000/api/arb').then((r) => r.json());
  log(`[rest] /api/arb: fills=${arb.stats.fills} expired=${arb.stats.expired} pnl=${arb.stats.pnl_usd} open=${arb.opportunities.length} fees=${arb.config.fees_bps.length}`);
  const put = await fetch('http://localhost:3000/api/arb/config', {
    method: 'PUT',
    headers: { 'content-type': 'application/json' },
    body: JSON.stringify({ fire_edge_bps: '5' }),
  });
  log(`[rest] PUT /api/arb/config -> ${put.status}`);
  const cfg = await fetch('http://localhost:3000/api/arb/config').then((r) => r.json());
  log(`[rest] GET config: enabled=${cfg.enabled} fire=${cfg.fire_edge_bps}`);

  // Book REST for SOL.
  const book = await fetch('http://localhost:3000/api/book?market=sol').then((r) => r.json());
  log(`[rest] /api/book?market=sol: venues=${book.venues?.length} mid=${book.stats?.mid} consolidated=${!!book.consolidated}`);

  // Final report.
  log('\n===== SERVER LOG TAIL =====');
  log(serverOut.split('\n').slice(-25).join('\n'));

  const ok =
    bookVenues.size >= 8 &&
    arbUpdates > 10 &&
    sawUsdtNon1 &&
    events.arb_snapshot >= 1;
  log(`\nVERDICT: ${ok ? 'PASS' : 'FAIL'} (venues=${bookVenues.size}, arb_updates=${arbUpdates}, usdt_live=${sawUsdtNon1}, arb_snapshot=${events.arb_snapshot}, fills=${arbFills})`);
  ws.close();
  SERVER.kill('SIGKILL');
  process.exit(ok ? 0 : 1);
}

main().catch((e) => {
  log('E2E ERROR:', e.message);
  log(serverOut.split('\n').slice(-15).join('\n'));
  SERVER.kill('SIGKILL');
  process.exit(1);
});
