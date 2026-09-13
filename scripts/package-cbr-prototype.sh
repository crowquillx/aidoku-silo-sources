#!/usr/bin/env bash
# Build a local package with the requested CBR backend.
set -euo pipefail
repo_dir=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)
backend=${1:-cbr-native}
case "$backend" in
  cbr-native) ;;
  cbr-wasm) "$repo_dir/experiments/cbr-wasm/build.sh" ;;
  *) printf 'Usage: %s [cbr-native|cbr-wasm]\n' "$0" >&2; exit 2 ;;
esac
cd -- "$repo_dir/sources/multi.silo"
aidoku package
cargo build --locked --release --target wasm32-unknown-unknown --no-default-features --features "$backend"
python3 - "$backend" <<'PY'
import sys
from pathlib import Path
from zipfile import ZIP_DEFLATED, ZipFile
backend = sys.argv[1]
package = f'package-{backend}.aix'
wasm = Path('target/wasm32-unknown-unknown/release/silo.wasm').read_bytes()
with ZipFile('package.aix') as base, ZipFile(package, 'w', ZIP_DEFLATED) as output:
    for info in base.infolist():
        data = wasm if info.filename == 'Payload/main.wasm' else base.read(info)
        output.writestr(info, data)
print('Created', Path(package).resolve())
print('Experimental main.wasm bytes:', len(wasm))
print('Experimental package bytes:', Path(package).stat().st_size)
PY
aidoku verify "package-$backend.aix"
