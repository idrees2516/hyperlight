import WebSocket from 'ws';
const ws = new WebSocket('wss://api.gateio.ws/ws/v4/');
ws.on('open', () => ws.send(JSON.stringify({time:Math.floor(Date.now()/1000),channel:'spot.order_book',event:'subscribe',payload:['ETH_USDT','20','100ms']})));
ws.on('message', d => {
  const v = JSON.parse(d);
  if (v.event === 'update') { console.log('KEYS:', Object.keys(v.result).join(',')); process.exit(0); }
});
setTimeout(()=>process.exit(1), 11000);
