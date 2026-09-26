#!/usr/bin/env bash
# Browser verification of the Global Sweep Optimizer panel (one bash session).
set -uo pipefail
cd /home/z/my-project

pkill -f hyperlight-server 2>/dev/null
sleep 1
setsid ./target/release/hyperlight-server >> server.log 2>&1 < /dev/null &
disown
sleep 5
echo "=== server up (waiting for venues + engines) ==="
sleep 15

agent-browser set viewport 1440 900
agent-browser open http://localhost:3000
agent-browser wait --load networkidle 2>/dev/null
sleep 12

echo "=== console errors ==="
agent-browser errors

echo "=== panels present ==="
agent-browser eval "document.body.innerText.includes('Cross-Venue Arbitrage Engine')"
agent-browser eval "document.body.innerText.includes('Global Sweep Optimizer')"
agent-browser eval "document.body.innerText.includes('Multi-Hop Swap Arbitrage')"
agent-browser eval "document.body.innerText.includes('Genetic Optimizer')"

echo "=== zero-fee sim mode for visible activity ==="
# Enable sim mode via REST to fill the engines with data.
curl -s -X PUT http://localhost:3000/api/arb/config -H 'content-type: application/json' \
  -d '{"fire_edge_bps":"0.5","min_edge_bps":"0.5","latency_ms":100,"cooldown_ms":700,"fees_bps":[["hyperliquid","0"],["lighter","0"],["binance","0"],["bybit","0"],["okx","0"],["kraken","0"],["coinbase","0"],["bitstamp","0"],["gate","0"]]}' > /dev/null
sleep 12

echo "=== scroll to sweep card and capture ==="
agent-browser eval "document.querySelectorAll('h3, [class*=card-title]').length"
agent-browser eval "
  const cards = [...document.querySelectorAll('*')].filter(e => e.textContent.includes('Global Sweep Optimizer') && e.textContent.length < 4000);
  const el = cards.sort((a,b) => a.textContent.length - b.textContent.length)[0];
  el ? el.scrollIntoView({block:'start'}) : null;
  'scrolled'
"
sleep 3
agent-browser screenshot /home/z/my-project/download/sweep_panel.png

echo "=== capture full page too ==="
agent-browser screenshot --full /home/z/my-project/download/sweep_full_page.png

echo "=== restore realistic fees ==="
curl -s -X PUT http://localhost:3000/api/arb/config -H 'content-type: application/json' \
  -d '{"fire_edge_bps":"8","min_edge_bps":"3","fees_bps":[["hyperliquid","45"],["lighter","0"],["binance","10"],["bybit","10"],["okx","10"],["kraken","26"],["coinbase","60"],["bitstamp","40"],["gate","20"]]}' > /dev/null

echo "=== done ==="
pkill -f hyperlight-server 2>/dev/null
true
