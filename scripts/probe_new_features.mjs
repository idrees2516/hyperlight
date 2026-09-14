// Probe APIs needed for the new features:
// 1. Lighter market catalogue (indices 3..N)
// 2. Lighter trades REST endpoint + shape
// 3. Hyperliquid universe (which perp coins exist)
// 4. Hyperliquid WS trades subscription shape
import WebSocket from 'ws';

const log = (...a) => console.log(...a);

async function tryJson(url, opts) {
  try {
    const r = await fetch(url, opts);
    const t = await r.text();
    let j = null;
    try { j = JSON.parse(t); } catch {}
    log(`\n[GET] ${url} -> ${r.status} (${t.length}b)`);
    log(t.slice(0, 900));
    return j;
  } catch (e) {
    log(`\n[GET] ${url} -> ERR ${e.message}`);
    return null;
  }
}

// --- 1. Lighter market catalogue candidates ---
await tryJson('https://mainnet.zklighter.elliot.ai/api/v1/markets');
await tryJson('https://api.zklighter.elliot.ai/api/v1/markets');
await tryJson('https://mainnet-api.zklighter.elliot.ai/api/v1/markets');
await tryJson('https://api.lighter.xyz/api/v1/markets');
await tryJson('https://api.lighter.xyz/v1/markets');

// --- 2. Lighter trades endpoint candidates ---
await tryJson('https://mainnet.zklighter.elliot.ai/api/v1/trades?market_index=2&limit=5');
await tryJson('https://api.zklighter.elliot.ai/api/v1/trades?market_index=2&limit=5');
await tryJson('https://api.lighter.xyz/api/v1/trades?market_index=2&limit=5');

// --- 3. Hyperliquid universe ---
const meta = await tryJson('https://api.hyperliquid.xyz/info', {
  method: 'POST',
  headers: { 'Content-Type': 'application/json' },
  body: JSON.stringify({ type: 'meta' }),
});
if (meta?.universe) {
  const coins = meta.universe.map((u, i) => `${i}:${u.name}`).join(' ');
  log('\nHL universe:', coins);
}

// --- 4. Hyperliquid WS trades ---
const ws = new WebSocket('wss://api.hyperliquid.xyz/ws');
ws.on('open', () => {
  log('\n[HL WS] open');
  ws.send(JSON.stringify({ method: 'subscribe', subscription: { type: 'trades', coin: 'SOL' } }));
});
ws.on('message', (d) => {
  const t = d.toString();
  if (t.includes('trades')) {
    log('[HL WS] trades frame:', t.slice(0, 600));
    ws.close();
    process.exit(0);
  }
});
setTimeout(() => { log('[HL WS] no trades in 20s'); process.exit(0); }, 20000);
