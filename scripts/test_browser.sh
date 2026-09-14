#!/usr/bin/env bash
# Browser E2E: server + agent-browser in one session (sandbox kills bg procs).
set -uo pipefail
cd /home/z/my-project

pkill -f hyperlight-server 2>/dev/null
sleep 1
setsid ./target/release/hyperlight-server >> server.log 2>&1 < /dev/null &
disown
sleep 4

echo "=== server up, opening browser ==="
agent-browser set viewport 1440 900
agent-browser open http://localhost:3000
agent-browser wait --load networkidle 2>/dev/null
sleep 12   # let books/tape/history arrive

echo "=== console errors ==="
agent-browser errors
echo "=== page title ==="
agent-browser get title

echo "=== screenshot 1: default view (ETH) ==="
agent-browser screenshot --full /home/z/my-project/download/shot1_eth_default.png

echo "=== open market selector ==="
# The trigger button shows the current label ETH-USD
agent-browser find text "ETH-USD" click || agent-browser snapshot -i
sleep 1
agent-browser screenshot /home/z/my-project/download/shot2_market_dropdown.png
echo "=== switch to DOGE ==="
agent-browser find text "DOGE-USD" click
sleep 6
agent-browser screenshot --full /home/z/my-project/download/shot3_doge.png

echo "=== set an alert via UI ==="
agent-browser snapshot -i -c
# find the price input & Set button via semantic locator
agent-browser find role textbox fill "0.085"
agent-browser find role button click --name "Set" || agent-browser find text "Set" click
sleep 2
agent-browser screenshot /home/z/my-project/download/shot4_alert_set.png
sleep 6
agent-browser screenshot --full /home/z/my-project/download/shot5_final.png

echo "=== console errors (final) ==="
agent-browser errors
echo "=== tape row count ==="
agent-browser eval "document.querySelectorAll('.tape-flash-buy, .tape-flash-sell, [class*=tape-flash]').length"
echo "=== svg paths (depth chart) ==="
agent-browser eval "document.querySelectorAll('svg path').length"
echo "=== done ==="
agent-browser close || true
