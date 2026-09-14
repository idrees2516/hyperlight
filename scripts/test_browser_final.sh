#!/usr/bin/env bash
# Final browser verification with accumulated history.
set -uo pipefail
cd /home/z/my-project

pkill -f hyperlight-server 2>/dev/null
sleep 1
setsid ./target/release/hyperlight-server >> server.log 2>&1 < /dev/null &
disown
echo "server started; letting history accumulate for 100s..."
sleep 100

agent-browser set viewport 1440 900
agent-browser open http://localhost:3000
agent-browser wait --load networkidle 2>/dev/null
sleep 8

echo "=== errors ==="
agent-browser errors
agent-browser screenshot --full /home/z/my-project/download/final1_eth.png

echo "=== dropdown with dimmed backdrop ==="
agent-browser find text "ETH-USD" click
sleep 1
agent-browser screenshot --full /home/z/my-project/download/final2_dropdown.png
agent-browser find text "SOL-USD" click
sleep 6

echo "=== set an alert that fires (below, above mid) ==="
agent-browser find role textbox fill "999999"
agent-browser find role button click --name "below" || agent-browser find text "below" click
agent-browser find role button click --name "Set" || true
sleep 3
agent-browser screenshot /home/z/my-project/download/final3_alert_toast.png
sleep 7
agent-browser screenshot --full /home/z/my-project/download/final4_sol_full.png

echo "=== chart sanity: svg path lengths ==="
agent-browser eval "Array.from(document.querySelectorAll('svg path')).map(p => p.getAttribute('d')?.length || 0).join(',')"
echo "=== tape rows ==="
agent-browser eval "document.body.innerText.split('\\n').filter(l => l.match(/^\\d\\d:\\d\\d:\\d\\d$/)).length"
echo "=== console errors final ==="
agent-browser errors
agent-browser close || true
echo done
