// Quick probe of Hyperliquid mainnet WS
const coins = process.argv.slice(2);
console.log('Probing coins:', coins.join(','));
const ws = new WebSocket('wss://api.hyperliquid.xyz/ws');
let count = 0;
ws.onopen = () => console.log('OPEN');
ws.onmessage = (ev) => {
  const msg = JSON.parse(ev.data);
  if (msg.channel === 'l2Book') {
    const d = msg.data;
    const bids = d.levels.bids.length, asks = d.levels.asks.length;
    if (count < 8) {
      console.log(`l2Book ${d.coin} @ ${new Date(d.time).toISOString()} | bids=${bids} asks=${asks}`);
      console.log(`   bestBid: ${JSON.stringify(d.levels.bids[0])}`);
      console.log(`   bestAsk: ${JSON.stringify(d.levels.asks[0])}`);
    }
    count++;
  } else if (msg.channel === 'pong') {
    console.log('pong received');
  } else if (msg.channel === 'subscriptionResponse') {
    console.log('subscriptionResponse:', JSON.stringify(msg.data).slice(0, 200));
    // send ping to test heartbeat
    ws.send(JSON.stringify({ method: 'ping' }));
  } else {
    console.log('MSG:', JSON.stringify(msg).slice(0, 300));
  }
};
ws.onerror = (e) => console.error('WS ERROR:', e.message || e);
ws.onclose = (e) => console.log('CLOSED', e.code, e.reason);
setTimeout(() => {
  // test server-ping handling: Hyperliquid sends {"method":"ping"} periodically?
  console.log(`Total book updates: ${count}`);
  process.exit(0);
}, 10000);
// send our own ping after 5s
setTimeout(() => { try { ws.send(JSON.stringify({ method: 'ping' })); } catch(e){} }, 5000);
