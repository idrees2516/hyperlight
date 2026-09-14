#!/usr/bin/env bash
# HyperLight dev harness: ensures the release binary exists, then serves on :3000.
# If the port is momentarily busy (another instance), retries instead of dying.
set -euo pipefail
cd "$(dirname "$0")/.."

BIN="./target/release/hyperlight-server"
if [ ! -x "$BIN" ]; then
  echo "[dev] release binary missing — building..."
  cargo build --release -p ob-server
fi

if [ ! -f "./dist/index.html" ]; then
  echo "[dev] frontend bundle missing — building..."
  bash scripts/build-ui.sh || echo "[dev] WARN: frontend build failed, serving whatever exists"
fi

export PORT="${PORT:-3000}"

# Restart-loop: keep serving across crashes; if the port is briefly taken,
# wait for it to free instead of exiting (platform dev-server friendly).
while true; do
  echo "[dev] starting HyperLight backend on 0.0.0.0:${PORT}"
  "$BIN" && break
  code=$?
  # exit code 0 = graceful shutdown (ctrl-c): stop the loop.
  if [ "$code" = "0" ]; then break; fi
  echo "[dev] server exited with code ${code}; restarting in 3s..."
  sleep 3
done
