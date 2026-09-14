// Verify Lighter WS: 12 order_book + 12 trade channel subscriptions on one session
import WebSocket from 'ws';

const markets = [0,1,2,3,4,5,6,7,8,9,10,11];
const ws = new WebSocket('wss://mainnet.zklighter.elliot.ai/stream');
let obCount = 0, tradeCount = 0;
const t0 = Date.now();

ws.on('open', () => console.log('[LT] socket open'));
ws.on('message', (d) => {
  const t = d.toString();
  let m; try { m = JSON.parse(t); } catch { return; }
  if (m.type === 'connected') {
    for (const idx of markets) {
      ws.send(JSON.stringify({ type: 'subscribe', channel: `order_book/${idx}` }));
      ws.send(JSON.stringify({ type: 'subscribe', channel: `trade/${idx}` }));
    }
    console.log('[LT] connected; subscribed 12 order_book + 12 trade channels');
  } else if (m.type === 'subscribed/order_book') { obCount++; }
  else if (m.type === 'subscribed/trade') { console.log('[LT] trade sub ack:', m.channel); }
  else if (m.type === 'update/trade') {
    if (tradeCount < 3) console.log('[LT TRADE]', m.channel, JSON.stringify(m.trades?.[0])?.slice(0, 300));
    tradeCount += (m.trades?.length || 0) + (m.liquidation_trades?.length || 0);
  } else if (m.type === 'error') { console.log('[LT] ERROR MSG:', t.slice(0, 200)); }
  if (Date.now() - t0 > 20000) { /* keep counting */ }
});

setTimeout(() => {
  console.log(`\n[LT] 20s summary: ob_subs=${obCount}, trades=${tradeCount}`);
  process.exit(0);
}, 20000);
