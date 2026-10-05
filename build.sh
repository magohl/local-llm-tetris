#!/usr/bin/env bash
# Build the Rust crate for the browser and (optionally) serve the game.
#
#   ./build.sh            # compile + copy the .wasm next to the web files
#   ./build.sh serve      # ... then serve http://localhost:8080
set -euo pipefail
cd "$(dirname "$0")"

cargo build --release --target wasm32-unknown-unknown
mkdir -p www
cp -f target/wasm32-unknown-unknown/release/tetris.wasm www/tetris.wasm
ls -lh www/tetris.wasm

if [[ "${1:-}" == "serve" ]]; then
  port="${2:-8080}"
  echo "open http://localhost:${port}/"
  exec python3 -m http.server "$port" --directory www
fi
