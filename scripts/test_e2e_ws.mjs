// End-to-end test: connect as a browser client to the backend /ws stream
const ws = new WebSocket('ws://localhost:3000/ws');
let books = 0, status = 0, lastBook = null;
ws.onopen = () => console.log('WS OPEN to backend');
ws.onmessage = (ev) => {
  const msg = JSON.parse(ev.data);
  if (msg.type === 'book') {
    books++;
    lastBook = msg;
    if (books <= 3) {
      const s = msg.stats;
      console.log(`BOOK ${msg.market}: hl=${msg.hyperliquid ? msg.hyperliquid.bids.length + '+' + msg.hyperliquid.asks.length : 'none'} ` +
        `lt=${msg.lighter ? msg.lighter.bids.length + '+' + msg.lighter.asks.length : 'none'} ` +
        `cons=${msg.consolidated ? msg.consolidated.bids.length + '+' + msg.consolidated.asks.length : 'none'} ` +
        `mid=${s.mid} spread=${s.spread}`);
    }
  } else if (msg.type === 'status') {
    status++;
    if (status === 1) console.log('STATUS:', JSON.stringify(msg));
  }
};
ws.onerror = (e) => console.error('ERROR', e.message || e);
ws.onclose = (e) => console.log('CLOSED', e.code, e.reason);
setTimeout(() => {
  console.log(`\nTotal: ${books} book events, ${status} status events in 8s (${(books/8).toFixed(1)}/s)`);
  if (lastBook) {
    const b = lastBook;
    console.log('\nFinal sample (' + b.market + '):');
    console.log('  best bid:', b.stats.best_bid, '| best ask:', b.stats.best_ask, '| mid:', b.stats.mid);
    console.log('  imbalance:', b.stats.imbalance, '| bid levels:', b.stats.bid_levels, '| ask levels:', b.stats.ask_levels);
    console.log('  hl health:', JSON.stringify(b.hyperliquid?.health));
    console.log('  lt health:', JSON.stringify(b.lighter?.health));
    console.log('  top cons bid:', JSON.stringify(b.consolidated?.bids?.[0]));
    console.log('  top cons ask:', JSON.stringify(b.consolidated?.asks?.[0]));
  }
  process.exit(0);
}, 8000);
