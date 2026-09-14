// Raw message dump from Hyperliquid WS
const ws = new WebSocket('wss://api.hyperliquid.xyz/ws');
let n = 0;
ws.onopen = () => {
  console.log('OPEN, sending subscribe ETH');
  ws.send(JSON.stringify({ method: 'subscribe', subscription: { type: 'l2Book', coin: 'ETH' } }));
};
ws.onmessage = (ev) => {
  n++;
  const s = typeof ev.data === 'string' ? ev.data : '(binary)';
  if (n <= 6) console.log(`RAW[${n}]:`, s.slice(0, 500));
  else if (n % 50 === 0) console.log(`... ${n} messages so far, last:`, s.slice(0, 120));
};
ws.onerror = (e) => console.error('WS ERROR:', e.message || e);
ws.onclose = (e) => console.log('CLOSED', e.code, e.reason);
setTimeout(() => { console.log(`Total messages: ${n}`); process.exit(0); }, 8000);
