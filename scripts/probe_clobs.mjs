// Probe all candidate CLOB venues for ETH + SOL book & trade streams.
// Verifies: connectivity, subscribe ack, message shape, rates over ~35s.
import WebSocket from 'ws';

const log = (...a) => console.log(...a);
const t0 = Date.now();
const el = () => ((Date.now() - t0) / 1000).toFixed(1) + 's';

function probe(name, url, setup, handlers = {}, ms = 35000) {
  return new Promise((resolve) => {
    const state = { counts: {}, first: {}, done: false };
    let ws;
    try {
      ws = new WebSocket(url, { handshakeTimeout: 12000 });
    } catch (e) {
      log(`[${name}] constructor error: ${e.message}`);
      return resolve(null);
    }
    const finish = (why) => {
      if (state.done) return;
      state.done = true;
      try { ws.close(); } catch {}
      resolve({ name, why, counts: state.counts, first: state.first });
    };
    ws.on('open', () => {
      log(`[${name}] OPEN ${el()}`);
      try { setup(ws, state); } catch (e) { log(`[${name}] setup error: ${e.message}`); }
    });
    ws.on('message', (data, isBinary) => {
      const raw = isBinary ? '<binary>' : data.toString();
      let v = null;
      try { v = JSON.parse(raw); } catch {}
      const key = v ? (v.channel || v.stream || v.event || v.type || v.op || v.method || (v.e) || 'json') : (isBinary ? 'binary' : 'text');
      state.counts[key] = (state.counts[key] || 0) + 1;
      const c = state.counts[key];
      if (c <= 2) {
        const s = raw.length > 600 ? raw.slice(0, 600) + `...(${raw.length}b)` : raw;
        log(`[${name}] msg#${c} key=${key} ${el()}: ${s}`);
        if (c === 1) state.first[key] = true;
      }
      try { handlers.onMsg?.(ws, v, raw, state); } catch (e) { log(`[${name}] handler error: ${e.message}`); }
    });
    ws.on('ping', () => { /* ws lib auto-pongs */ });
    ws.on('error', (e) => { log(`[${name}] ERROR ${el()}: ${e.message}`); finish('error:' + e.message); });
    ws.on('close', (code, reason) => { log(`[${name}] CLOSE ${el()} code=${code} ${reason}`); finish('close:' + code); });
    setTimeout(() => finish('timeout-ok'), ms);
  });
}

const probes = [];

// 1) Binance combined stream: depth20 + trades for ETH + SOL
probes.push(probe('binance', 'wss://stream.binance.com:9443/stream?streams=ethusdt@depth20@100ms/solusdt@depth20@100ms/ethusdt@trade/solusdt@trade', (ws) => {
  // combined streams need no subscribe message (streams in URL)
}, {
  onMsg: (ws, v) => {
    if (v?.data?.e === 'trade') { /* shape logged */ }
  }
}));

// 2) Bybit v5 spot: orderbook.50 + publicTrade for ETHUSDT + SOLUSDT
probes.push(probe('bybit', 'wss://stream.bybit.com/v5/public/spot', (ws) => {
  ws.send(JSON.stringify({ op: 'subscribe', args: ['orderbook.50.ETHUSDT', 'orderbook.50.SOLUSDT', 'publicTrade.ETHUSDT', 'publicTrade.SOLUSDT'] }));
}));

// 3) OKX v5 public: books5 + trades for ETH-USDT + SOL-USDT
probes.push(probe('okx', 'wss://ws.okx.com:8443/ws/v5/public', (ws) => {
  ws.send(JSON.stringify({ op: 'subscribe', args: [
    { channel: 'books5', instId: 'ETH-USDT' }, { channel: 'books5', instId: 'SOL-USDT' },
    { channel: 'trades', instId: 'ETH-USDT' }, { channel: 'trades', instId: 'SOL-USDT' },
  ] }));
  setInterval(() => { try { ws.send('ping'); } catch {} }, 20000);
}));

// 4) Kraken v2: book (depth 100) + trade for ETH/USD + SOL/USD + USDT/USD book (quote rate)
probes.push(probe('kraken', 'wss://ws.kraken.com/v2', (ws) => {
  ws.send(JSON.stringify({ method: 'subscribe', params: { channel: 'book', symbol: ['ETH/USD', 'SOL/USD', 'USDT/USD'], depth: 100 } }));
  ws.send(JSON.stringify({ method: 'subscribe', params: { channel: 'trade', symbol: ['ETH/USD', 'SOL/USD'] } }));
  setInterval(() => { try { ws.send(JSON.stringify({ method: 'ping' })); } catch {} }, 20000);
}));

// 5) Coinbase Exchange: level2 + matches for ETH-USD + SOL-USD
probes.push(probe('coinbase', 'wss://ws-feed.exchange.coinbase.com', (ws) => {
  ws.send(JSON.stringify({ type: 'subscribe', product_ids: ['ETH-USD', 'SOL-USD'], channels: ['level2', 'matches'] }));
}));

// 6) Bitstamp: order_book + live_trades for ethusd + solusd
probes.push(probe('bitstamp', 'wss://ws.bitstamp.net', (ws) => {
  for (const ch of ['order_book_ethusd', 'order_book_solusd', 'live_trades_ethusd', 'live_trades_solusd']) {
    ws.send(JSON.stringify({ event: 'bts:subscribe', data: { channel: ch } }));
  }
  setInterval(() => { try { ws.send(JSON.stringify({ event: 'bts:heartbeat' })); } catch {} }, 15000);
}));

// 7) Gate.io v4: spot.order_book + spot.trades for ETH_USDT + SOL_USDT
probes.push(probe('gate', 'wss://api.gateio.ws/ws/v4/', (ws) => {
  const now = () => Math.floor(Date.now() / 1000);
  ws.send(JSON.stringify({ time: now(), channel: 'spot.order_book', event: 'subscribe', payload: ['ETH_USDT', '20', '100ms'] }));
  ws.send(JSON.stringify({ time: now(), channel: 'spot.order_book', event: 'subscribe', payload: ['SOL_USDT', '20', '100ms'] }));
  ws.send(JSON.stringify({ time: now(), channel: 'spot.trades', event: 'subscribe', payload: ['ETH_USDT'] }));
  ws.send(JSON.stringify({ time: now(), channel: 'spot.trades', event: 'subscribe', payload: ['SOL_USDT'] }));
  setInterval(() => { try { ws.send(JSON.stringify({ time: now(), channel: 'spot.ping' })); } catch {} }, 15000);
}));

const results = await Promise.all(probes);
log('\n===== SUMMARY =====');
for (const r of results) {
  if (!r) continue;
  log(`${r.name}: end=${r.why} counts=${JSON.stringify(r.counts)}`);
}
