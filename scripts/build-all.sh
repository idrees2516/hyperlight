#!/usr/bin/env bash
# Full rebuild: backend (Axum) + frontend (Leptos/WASM) + assets.
set -euo pipefail
cd "$(dirname "$0")/.."
source "$HOME/.cargo/env"

echo "=== [1/2] backend (cargo release) ==="
cargo build --release -p ob-server

echo "=== [2/2] frontend (tailwind + trunk) ==="
bash scripts/build-ui.sh

echo "=== done ==="
ls -la dist/ target/release/hyperlight-server
