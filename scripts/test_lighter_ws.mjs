// Quick probe of Lighter mainnet WS - test multiple market ids
const markets = process.argv.slice(2).map(Number);
console.log('Probing markets:', markets.join(','));
const ws = new WebSocket('wss://mainnet.zklighter.elliot.ai/stream');
let count = 0;
ws.onopen = () => console.log('OPEN');
ws.onmessage = (ev) => {
  const msg = JSON.parse(ev.data);
  if (msg.type === 'connected') {
    console.log('CONNECTED, subscribing to', markets.length, 'markets');
    for (const m of markets) {
      ws.send(JSON.stringify({ type: 'subscribe', channel: `order_book/${m}` }));
    }
  } else if (msg.type === 'ping') {
    ws.send(JSON.stringify({ type: 'pong' }));
  } else if (msg.type === 'subscribed/order_book' || msg.type === 'update/order_book') {
    const ob = msg.order_book || {};
    const bids = (ob.bids || []).length, asks = (ob.asks || []).length;
    const bestBid = (ob.bids || [])[0], bestAsk = (ob.asks || [])[0];
    if (count < 12 || msg.type === 'subscribed/order_book') {
      console.log(`${msg.type} | ${msg.channel} | bids=${bids} asks=${asks}`);
      if (bestBid) console.log(`   bestBid: ${JSON.stringify(bestBid)}`);
      if (bestAsk) console.log(`   bestAsk: ${JSON.stringify(bestAsk)}`);
    }
    count++;
  } else {
    console.log('MSG:', JSON.stringify(msg).slice(0, 300));
  }
};
ws.onerror = (e) => console.error('WS ERROR:', e.message || e);
ws.onclose = (e) => console.log('CLOSED', e.code, e.reason);
setTimeout(() => { console.log(`Total updates: ${count}`); process.exit(0); }, 10000);
