# Nested RAR prototype

> **Archived.** The Silo source no longer has the `cbr-wasm` feature; it ships
> only the native decoder in [`crates/cbr-native`](../../crates/cbr-native/README.md).
> The source-side commands below apply to the source as it was when this
> comparison ran. The shared RAR fixtures now live in
> [`crates/cbr-native/fixtures`](../../crates/cbr-native/fixtures/README.md).

This experiment proves that an Aidoku `no_std` source can interpret an
import-free RAR decoder. The source feature is `cbr-wasm`, an opt-in comparison
backend. Source version 4 enables the native `cbr-native` backend by default.
Both CBR paths download the full archive for every page. The source-level
limits are a 16 MiB archive, 16 MiB per unpacked page, 64 MiB total unpacked
members, and 512 entries. The native parser also limits RAR5 dictionaries to
8 MiB. See [the report](../../docs/research/cbr-feasibility.md) for measured
costs and limits. No Aidoku device validation has been completed.

Prerequisites are Rust with `wasm32-unknown-unknown`, Node/npm, and the installed
Aidoku CLI/test runner. Cargo and npm lockfiles pin the dependencies.

From the repository root, build and verify a local experimental package:

```sh
scripts/package-cbr-prototype.sh cbr-wasm
```

This produces `sources/multi.silo/package-cbr-wasm.aix`. The script first
packages the default source, then builds with
`--no-default-features --features cbr-wasm` and replaces `Payload/main.wasm` in
a separate package. This is necessary because the
installed `aidoku package` command does not accept Cargo features. Both
packages use source version 4. The regular `package.aix` uses the native CBR
default; `package-cbr-wasm.aix` selects this opt-in backend. The default source
builds without the nested guest or Node dependencies.
Generated Wasm, packages, npm modules and target directories are ignored.

To reproduce the isolated experiment:

```sh
cd experiments/cbr-wasm
./build.sh
cargo test --release -- --nocapture
node verify-fixtures.cjs
node benchmark.mjs
```

`build.sh` installs the pinned npm packages, copies stock guest binaries for
import/validation tests, builds the custom guest with Rust, removes non-ABI
exports using Binaryen, asserts zero imports, and builds the outer `no_std`
probe. `guest/src/lib.rs` uses rars in a separate guest; no `rars` dependency
is linked into the Aidoku source itself.

The source's own feature tests run from its crate directory:

```sh
cd ../../sources/multi.silo
cargo test --no-default-features --features cbr-wasm
cargo clippy --release --target wasm32-unknown-unknown --no-default-features --features cbr-wasm
python3 tests/mock_silo_v2.py
# In a second terminal, from the same directory:
cargo test --no-default-features --features cbr-wasm -- --ignored
```

The mock integration exercises authenticated RAR chapter enumeration, natural
page order, page requests and byte decoding. The fixture tests include RAR4
and RAR5, normal and solid compression, and truncated/invalid data. Read the
source tests for the exact assertions; this is not a full RAR conformance suite.

For the pinned engine check, use the exact Wasm3 source and its
current 200 KiB runtime stack. This C harness needs GCC and libm on Linux,
not Swift or an iOS device. The host harness reads local test files; the
source and guest still have no filesystem imports. This check does not count as
Aidoku device validation.

```sh
# Still in experiments/cbr-wasm:
git clone https://github.com/Skittyblock/Wasm3.git artifacts/Wasm3
git -C artifacts/Wasm3 checkout 6a64d8bfc6a235f1ee3f8b57f692494fbf72ed19
gcc -O3 -Dd_m3MaxDuplicateFunctionImpl=10 -Dd_m3HasWASI=0 \
  -Iartifacts/Wasm3/Sources/wasm3-c/include \
  wasm3-benchmark.c artifacts/Wasm3/Sources/wasm3-c/*.c \
  -lm -o artifacts/wasm3-benchmark
artifacts/wasm3-benchmark \
  target/wasm32-unknown-unknown/release/silo_cbr_wasm_probe.wasm
```

The harness creates a Wasm3 runtime with 204,800 stack bytes, links only
`env.print` and `env.abort`, and runs extraction/byte-equality assertions.
WASI is disabled in the harness build because neither module imports it.
The module's raw imports can also be inspected with Node:

```sh
node --input-type=module -e '
import fs from "node:fs";
const m = new WebAssembly.Module(fs.readFileSync("artifacts/rar-guest.wasm"));
console.log(WebAssembly.Module.imports(m), WebAssembly.Module.exports(m));
'
```

For larger fixtures and resource limits:

```sh
node generate-fixtures.cjs --large
node verify-fixtures.cjs --large
node benchmark-large.mjs
/usr/bin/time -v artifacts/wasm3-benchmark \
  target/wasm32-unknown-unknown/release/silo_cbr_wasm_probe.wasm \
  artifacts/large/rar50-normal.cbr 2 500000000
/usr/bin/time -v artifacts/wasm3-benchmark \
  target/wasm32-unknown-unknown/release/silo_cbr_wasm_probe.wasm \
  artifacts/large/rar50-solid.cbr 2 500000000
```

The large solid case is expected to stop at the fuel limit. A numeric result
of 4,294,967,295 in the C harness, or -1 in JavaScript, is the error sentinel,
not a four-gigabyte output. The source maps that class of interpreter failure
to a readable error. `benchmark-large.mjs` also tries a much larger diagnostic
fuel budget; that value is never used by the source feature.

[Fixture provenance](../../crates/cbr-native/fixtures/README.md), SHA-256 hashes, import/memory logs,
license metadata and timing output are included in `evidence/`. Early logs
are retained for failed experiments as well; final `*-200k.log` and
`nested-tests.log` files are the current runtime evidence.
