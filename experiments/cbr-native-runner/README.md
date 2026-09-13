# Native decoder in Aidoku's engine

This `no_std` harness links the native parser and Aidoku's allocator. It uses
the C Wasm3 benchmark driver from `../cbr-wasm` so both decoders run with the
same 200 KiB stack and engine revision. The source module has no filesystem
or WASI imports; only the host driver reads test files.

First build the pinned C driver using the commands in
[the nested experiment](../cbr-wasm/README.md). Then, from this directory:

```sh
cargo build --locked --release --target wasm32-unknown-unknown
../cbr-wasm/artifacts/wasm3-benchmark \
  target/wasm32-unknown-unknown/release/silo_cbr_native_runner.wasm
# Generate and independently verify large fixtures using ../cbr-wasm/README.md.
/usr/bin/time -v ../cbr-wasm/artifacts/wasm3-benchmark \
  target/wasm32-unknown-unknown/release/silo_cbr_native_runner.wasm \
  ../cbr-wasm/artifacts/large/rar50-normal.cbr 2 0
/usr/bin/time -v ../cbr-wasm/artifacts/wasm3-benchmark \
  target/wasm32-unknown-unknown/release/silo_cbr_native_runner.wasm \
  ../cbr-wasm/artifacts/large/rar50-solid.cbr 2 0
```

The last argument is unused by the native decoder. The shared driver prints
it as `fuel_limit`; native work has no fuel counter. Small tests compare the
entire page output with the original PNG. The large driver records length,
timing and memory; the parser checks CRCs, and `../cbr-native-probe` compares
all large output bytes on the host. The benchmark module embeds the small
fixtures, so its size is not the size of a release source.

`*-final.log` records the current measurements. Earlier logs preserve the
initial successful engine probe. The full source's package sizes and test
results are in [the report](../../docs/research/cbr-feasibility.md).
