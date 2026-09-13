#!/usr/bin/env bash
set -euo pipefail
cd -- "$(dirname -- "${BASH_SOURCE[0]}")"
npm ci --ignore-scripts
mkdir -p artifacts
cp node_modules/@bitplane/rars/browser/wasm/rars_wasm_bg.wasm artifacts/rars-npm.wasm
cp node_modules/node-unrar-js/dist/js/unrar.wasm artifacts/unrar-npm.wasm
(cd guest && cargo build --locked --release --target wasm32-unknown-unknown)
node prepare-guest.mjs guest/target/wasm32-unknown-unknown/release/silo_rar_guest.wasm artifacts/rar-guest.wasm
cargo build --locked --release --target wasm32-unknown-unknown
