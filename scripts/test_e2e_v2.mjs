// E2E test of the new server protocol: 12 markets, per-socket selection,
// trade tape, history appends, alerts (WS + REST), firing.
import WebSocket from 'ws';

const WS_URL = 'ws://localhost:3000/ws';
const BASE = 'http://localhost:3000';

const counts = {};
const seen = { bookMarkets: new Set(), tickerMarkets: new Set(), tradeMarkets: new Set(), histMarkets: new Set() };
let historyAppendCount = 0;
let alertFired = null;
let alertSetCount = 0;

const ws = new WebSocket(WS_URL);
const t0 = Date.now();

const log = (...a) => console.log(`[+${((Date.now() - t0) / 1000).toFixed(1)}s]`, ...a);

function tag(ev) {
  counts[ev.type] = (counts[ev.type] || 0) + 1;
  switch (ev.type) {
    case 'book': seen.bookMarkets.add(ev.market); break;
    case 'ticker': seen.tickerMarkets.add(ev.market); break;
    case 'trades': seen.tradeMarkets.add(ev.market); break;
    case 'history': seen.histMarkets.add(ev.market); break;
    case 'alert_set': alertSetCount++; break;
    case 'alert_fired': alertFired = ev.alert; break;
  }
}

ws.on('open', async () => {
  log('connected');
});

ws.on('message', async (d) => {
  const ev = JSON.parse(d.toString());
  tag(ev);

  if (ev.type === 'book' && !globalThis.gotBook) {
    globalThis.gotBook = true;
    log('first book:', ev.market, 'mid', ev.stats?.mid, '| consolidated levels:', ev.consolidated?.bids?.length, '/', ev.consolidated?.asks?.length);
    // 2) Switch market -> expect doge book + tape + history.
    ws.send(JSON.stringify({ type: 'select', market: 'doge' }));
    log('sent select doge');
  }

  if (ev.type === 'book' && ev.market === 'doge' && !globalThis.gotDoge) {
    globalThis.gotDoge = true;
    log('DOGE book arrived, mid:', ev.stats?.mid);
    // 3) Create an alert that fires immediately: below (mid + 0.5%).
    const mid = parseFloat(ev.stats.mid);
    const th = (mid * 1.005).toFixed(4);
    ws.send(JSON.stringify({ type: 'alert_create', market: 'doge', dir: 'below', price: th }));
    log(`sent alert_create below ${th} (mid ${mid})`);
  }

  if (ev.type === 'history' && ev.market === 'doge' && ev.samples?.length >= 1 && !globalThis.gotDogeHist) {
    globalThis.gotDogeHist = true;
    const s = ev.samples[ev.samples.length - 1];
    log(`DOGE history: ${ev.samples.length} samples, last t=${Math.round((Date.now() - s.t) / 1000)}s ago, mid=${s.mid}, b10=$${Math.round(s.b10)}, a10=$${Math.round(s.a10)}`);
  }
});

// after 22s: report + REST tests, then exit
setTimeout(async () => {
  log('--- summary ---');
  log('event counts:', JSON.stringify(counts));
  log('book markets:', [...seen.bookMarkets].join(','));
  log('ticker markets:', seen.tickerMarkets.size, 'unique');
  log('trade markets:', [...seen.tradeMarkets].join(','));
  log('history markets:', [...seen.histMarkets].join(','));

  // REST: markets
  const mk = await (await fetch(`${BASE}/api/markets`)).json();
  log('REST /api/markets:', mk.markets.length, 'markets ->', mk.markets.slice(0, 6).map(m => m.id).join(','), '...');

  // REST: history for sol
  const h = await (await fetch(`${BASE}/api/history?market=sol`)).json();
  log('REST /api/history sol:', h.samples?.length, 'samples');

  // REST: tape for eth
  const tp = await (await fetch(`${BASE}/api/tape?market=eth`)).json();
  log('REST /api/tape eth:', tp.trades?.length, 'trades');

  // REST: alert lifecycle
  const created = await fetch(`${BASE}/api/alerts`, {
    method: 'POST', headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ market: 'xrp', dir: 'above', price: '9999999' }),
  });
  const ca = await created.json();
  log('REST POST /api/alerts:', created.status, 'id', ca.id);
  const list = await (await fetch(`${BASE}/api/alerts`)).json();
  log('REST GET /api/alerts:', list.alerts.map(a => `#${a.id} ${a.market} ${a.dir} ${a.price} fired=${!!a.triggered_ms}`).join(' | '));
  const del = await fetch(`${BASE}/api/alerts/${ca.id}`, { method: 'DELETE' });
  log('REST DELETE:', del.status);

  log('alert fired:', alertFired ? `#${alertFired.id} ${alertFired.market} ${alertFired.dir} ${alertFired.price} @ ${alertFired.triggered_px}` : 'NO');
  log('alert_set events:', alertSetCount);

  ws.close();
  process.exit(0);
}, 22000);
