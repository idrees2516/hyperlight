// Verify: (1) HL WS trades data frames, (2) LT REST trades with correct params
import WebSocket from 'ws';

// --- 1. Hyperliquid WS trades frames ---
const hl = new WebSocket('wss://api.hyperliquid.xyz/ws');
hl.on('open', () => {
  console.log('[HL] open');
  for (const c of ['SOL', 'DOGE', 'WIF', 'kPEPE']) {
    hl.send(JSON.stringify({ method: 'subscribe', subscription: { type: 'trades', coin: c } }));
  }
});
let hlCount = 0;
hl.on('message', (d) => {
  const t = d.toString();
  if (t.includes('"channel":"trades"')) {
    console.log('[HL trades frame]:', t.slice(0, 700));
    if (++hlCount >= 2) hl.close();
  }
});

// --- 2. Lighter REST trades ---
const variants = [
  'https://mainnet.zklighter.elliot.ai/api/v1/trades?market_id=2&limit=3&sort_by=timestamp&sort_dir=desc',
  'https://mainnet.zklighter.elliot.ai/api/v1/trades?market_id=3&limit=3&sort_by=timestamp&sort_dir=desc',
];
for (const u of variants) {
  try {
    const r = await fetch(u);
    const t = await r.text();
    console.log(`\n[LT] ${u.split('?')[1]} -> ${r.status}`);
    console.log(t.slice(0, 1200));
  } catch (e) { console.log(`[LT] ERR ${e.message}`); }
}

setTimeout(() => process.exit(0), 25000);
