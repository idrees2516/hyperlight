#!/usr/bin/env bash
# Browser E2E for the 9-venue + arbitrage build (one bash session).
set -uo pipefail
cd /home/z/my-project

pkill -f hyperlight-server 2>/dev/null
sleep 1
setsid ./target/release/hyperlight-server >> server.log 2>&1 < /dev/null &
disown
sleep 5
echo "=== server up (waiting for venues + arb data) ==="
sleep 15

agent-browser set viewport 1440 900
agent-browser open http://localhost:3000
agent-browser wait --load networkidle 2>/dev/null
sleep 10

echo "=== console errors ==="
agent-browser errors
echo "=== venue badges in header (live count) ==="
agent-browser eval "document.body.innerText.split('\n').filter(l => /live/i.test(l)).length"
echo "=== arb panel present ==="
agent-browser eval "document.body.innerText.includes('Cross-Venue Arbitrage Engine')"

echo "=== screenshot 1: full default view (ETH) ==="
agent-browser screenshot --full /home/z/my-project/download/arb1_eth_full.png

echo "=== switch to SOL ==="
agent-browser find text "ETH-USD" click || true
sleep 1
agent-browser find text "SOL-USD" click || true
sleep 6
agent-browser screenshot --full /home/z/my-project/download/arb2_sol_full.png

echo "=== arb panel: enable zero-fee sim to generate visible activity ==="
# Scroll to the arb card and screenshot it alone
agent-browser eval "document.body.innerText.includes('Engine Configuration')"
agent-browser screenshot --full /home/z/my-project/download/arb3_config.png

echo "=== venue tab switching ==="
agent-browser find text "Consolidated" click || true
sleep 1
# click a venue tab (e.g. Binance) if present
agent-browser find text "Binance" click || true
sleep 2
agent-browser screenshot --full /home/z/my-project/download/arb4_binance_tab.png

echo "=== svg chart sanity ==="
agent-browser eval "Array.from(document.querySelectorAll('svg path')).map(p => (p.getAttribute('d')||'').length).join(',')"

echo "=== tape rows with venue chips ==="
agent-browser eval "document.body.innerText.split('\n').filter(l => /^\d\d:\d\d:\d\d$/.test(l)).length"

echo "=== final console errors ==="
agent-browser errors
agent-browser close || true
pkill -f hyperlight-server 2>/dev/null
echo done
