#!/usr/bin/env bash
# HyperLight boot script — run by the platform container at start
# (see /start.sh: "custom dev script" branch). Serves the pre-built
# release binary + static frontend bundle on :3000 so the public
# proxy (Caddy :81 -> :3000) has a live backend 24/7.
#
# The binary and dist/ are committed to the repo, so no Rust toolchain
# is needed at boot. If the binary is somehow missing, we attempt a
# rebuild when cargo is available, then keep retrying.

cd /home/z/my-project
export PORT="${PORT:-3000}"
export RUST_LOG="${RUST_LOG:-info}"

mkdir -p /home/z/my-project/logs

# Make sure we have a runnable binary; rebuild only as a fallback.
if [ ! -x ./bin/hyperlight-server ]; then
  echo "[hyperlight-boot] committed binary missing, attempting rebuild..."
  if source "$HOME/.cargo/env" 2>/dev/null && command -v cargo >/dev/null 2>&1; then
    if cargo build --release -p ob-server 2>&1 | tee -a logs/dev.log; then
      mkdir -p bin && cp -f target/release/hyperlight-server bin/
    fi
  fi
fi

# Keep serving forever — restart on crash, wait out port contention.
while true; do
  if [ -x ./bin/hyperlight-server ]; then
    echo "[hyperlight-boot] starting HyperLight on 0.0.0.0:${PORT}" >> logs/dev.log
    ./bin/hyperlight-server >> logs/dev.log 2>&1
    code=$?
    if [ "$code" = "0" ]; then break; fi   # graceful shutdown (SIGTERM at pre-stop)
    echo "[hyperlight-boot] exited code=${code}; restarting in 5s" >> logs/dev.log
  else
    echo "[hyperlight-boot] no binary available yet; retrying in 30s" >> logs/dev.log
  fi
  sleep "${HYPERLIGHT_RETRY_S:-5}"
done
