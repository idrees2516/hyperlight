#!/usr/bin/env bash
# Build the Leptos/WASM frontend: Tailwind CSS + trunk -> dist/
set -euo pipefail
cd "$(dirname "$0")/../crates/ui"

echo "[ui] building tailwind css..."
if [ -x ./node_modules/.bin/tailwindcss ]; then
  ./node_modules/.bin/tailwindcss -i style.css -o tailwind.out.css --minify
else
  npx -y @tailwindcss/cli -i style.css -o tailwind.out.css --minize 2>/dev/null || \
  npx -y @tailwindcss/cli -i style.css -o tailwind.out.css --minify
fi

echo "[ui] building wasm bundle (trunk)..."
# NO_COLOR=1 in this sandbox trips a clap bug in trunk 0.21.14; override it.
env NO_COLOR=false trunk build --release

cp -f tailwind.out.css ../../dist/tailwind.out.css
echo "[ui] done -> ../../dist/"
