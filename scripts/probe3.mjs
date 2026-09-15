import WebSocket from 'ws';
const ws = new WebSocket('wss://api.gateio.ws/ws/v4/');
const now = () => Math.floor(Date.now()/1000);
let n = 0;
ws.on('open', () => {
  ws.send(JSON.stringify({time:now(),channel:'spot.trades',event:'subscribe',payload:['ETH_USDT']}));
  ws.send(JSON.stringify({time:now(),channel:'spot.trades',event:'subscribe',payload:['SOL_USDT']}));
});
ws.on('message', d => {
  const s = d.toString();
  const v = JSON.parse(s);
  if (v.channel === 'spot.trades' && v.event === 'update' && n++ < 3) {
    console.log(s.slice(0, 600));
  }
});
setTimeout(() => process.exit(0), 15000);
