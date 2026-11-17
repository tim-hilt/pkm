#!/usr/bin/env bash
# Usage: [SCREEN=filters|filter] [OUT=path] [WIDTH=1280] [HEIGHT=800] scripts/screenshot-web.sh
# Serves the web app's dev server and captures the Gallery with Playwright's
# Chromium (WebGPU on) into $OUT (default /tmp/wt-web.png). The wasm must be
# built (`make web-wasm`); `make screenshot DEVICE=web` does it first.
set -euo pipefail
cd "$(dirname "$0")/../web"

OUT=${OUT:-/tmp/wt-web.png}
# Bun's dev server listens on $PORT.
export PORT=${PORT:-3917}
bun scripts/dev.ts >/tmp/wt-web-server.log 2>&1 &
server=$!
trap 'kill $server 2>/dev/null || true' EXIT
for _ in $(seq 50); do
    curl -sf "http://localhost:$PORT/" >/dev/null && break
    sleep 0.2
done
node scripts/screenshot.mjs "$OUT"
