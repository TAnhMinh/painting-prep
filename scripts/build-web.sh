#!/usr/bin/env bash
# Builds prep-wasm and generates its ES-module glue into web/pkg/ (spec §5, D22).
# Needs the wasm-bindgen CLI at exactly the version in Cargo.lock:
#   cargo install wasm-bindgen-cli --version <version> --locked
set -euo pipefail
cd "$(dirname "$0")/.."

want=$(cargo metadata --format-version 1 --locked \
    | python3 -c 'import json,sys; print(next(p["version"] for p in json.load(sys.stdin)["packages"] if p["name"] == "wasm-bindgen"))')
have=$(wasm-bindgen --version 2>/dev/null | awk '{print $2}' || true)
if [[ "$have" != "$want" ]]; then
    echo "error: wasm-bindgen CLI is '${have:-missing}', Cargo.lock needs $want." >&2
    echo "Install it with: cargo install wasm-bindgen-cli --version $want --locked" >&2
    exit 1
fi

cargo build --release --locked -p prep-wasm --target wasm32-unknown-unknown
wasm-bindgen --target web --no-typescript --out-dir web/pkg \
    target/wasm32-unknown-unknown/release/prep_wasm.wasm
ls -l web/pkg
