import WebSocket from 'ws';
const t0 = Date.now();
const el = () => ((Date.now()-t0)/1000).toFixed(1)+'s';

// Coinbase ticker channel
{
  const ws = new WebSocket('wss://ws-feed.exchange.coinbase.com');
  ws.on('open', () => ws.send(JSON.stringify({type:'subscribe',product_ids:['ETH-USD','SOL-USD'],channels:['ticker']})));
  ws.on('message', d => {
    const v = JSON.parse(d);
    if (v.type === 'ticker' && v.product_id === 'ETH-USD') console.log(`[cb ticker] ${el()}: ${JSON.stringify(v).slice(0,500)}`);
  });
  setTimeout(()=>{ws.close(); p1();}, 12000);
}
// Gate book push
let g = 0;
{
  const ws = new WebSocket('wss://api.gateio.ws/ws/v4/');
  ws.on('open', () => ws.send(JSON.stringify({time:Math.floor(Date.now()/1000),channel:'spot.order_book',event:'subscribe',payload:['ETH_USDT','20','100ms']})));
  ws.on('message', d => {
    const v = JSON.parse(d);
    if (v.event === 'update' && g++ < 2) console.log(`[gate book] ${el()}: ${JSON.stringify(v).slice(0,500)}`);
  });
  setTimeout(()=>{ws.close(); p2();}, 12000);
}
// OKX trades
{
  const ws = new WebSocket('wss://ws.okx.com:8443/ws/v5/public');
  ws.on('open', () => ws.send(JSON.stringify({op:'subscribe',args:[{channel:'trades',instId:'ETH-USDT'},{channel:'trades',instId:'SOL-USDT'}]})));
  ws.on('message', d => {
    const s = d.toString(); if (s === 'pong') return;
    const v = JSON.parse(s);
    if (v.arg?.channel === 'trades') console.log(`[okx trades] ${el()}: ${s.slice(0,400)}`);
  });
  setTimeout(()=>{ws.close(); p3();}, 12000);
}
let n = 0; const done = () => { if (++n === 3) process.exit(0); };
const p1 = done, p2 = done, p3 = done;
