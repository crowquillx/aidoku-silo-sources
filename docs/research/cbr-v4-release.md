# Silo source version 4

Version 4 enables native RAR4/RAR5 comic decoding through the `cbr-native`
Cargo feature. It also fixes image requests in API-key mode using a stale
password access token. ZIP extraction and its page-range behavior are unchanged.
The source list remains at the existing Pages URL; installed clients can
update the Silo source to version 4.

The decoder has tested stored, LZ and solid paths. It accepts archives and
individual members up to 16 MiB, total declared output up to 64 MiB, up to
512 entries, and RAR5 dictionaries up to 8 MiB. It downloads the complete
archive to enumerate pages and again for each requested page. Encrypted,
split/multivolume archives and unsupported format flags return errors.

A RAR3 LZ stream can switch to PPMd midstream and allocate beyond those
limits. There is no native fuel counter, and no physical-device test was
performed. This release exposes the tested decoder with those limitations;
it does not establish universal or fully resource-bounded RAR support.
The [feasibility report](cbr-feasibility.md) contains the exact decoder,
license and performance evidence. The existing conversion script remains
available for unsupported or large archives.

The nested `cbr-wasm` alternative is optional. Build it using
`scripts/package-cbr-prototype.sh cbr-wasm`, which disables the default native
feature for that comparison package. For a CBZ-only Cargo build, use
`--no-default-features`.

The Pages workflow now watches `experiments/cbr-native/**`, the path of the
source's native decoder dependency, and runs `aidoku verify package.aix`
before publishing the source list. Local checks passed before publication. Logs are in
[`evidence/cbr-v4`](evidence/cbr-v4).

## Local release checks

```sh
cd sources/multi.silo
cargo fmt
cargo clippy --release --target wasm32-unknown-unknown
cargo test
cargo clippy --release --target wasm32-unknown-unknown --no-default-features
cargo test --no-default-features
# With python3 tests/mock_silo_v2.py running:
cargo test -- --ignored
aidoku package
aidoku verify package.aix
```

The native default passed eight source tests and two authenticated mock tests.
The CBZ-only build passed six source tests. This includes live browsing and
first-page CBZ extraction against the supplied test Silo. The mock requires
bearer/API-key and profile headers and serves CBR with its RAR content type.

The local version 4 package is **172,067 bytes**. Its `main.wasm` is
**403,229 bytes**, SHA-256
`05ced820498d6c77b876871d4a9027fa097bc6736a95e727559524a8afd5be6c`.
This is the same compiled decoder payload tested in the version 3 native
prototype. The package now contains source version 4 and is produced directly
by Aidoku's CLI; the earlier comparison package used Python ZIP repacking.
[Local package hashes](evidence/cbr-v4/local-package.json) identify both files.

Publication will use the existing `main` push workflow, then the `gh-pages`
branch. The deployed index and downloadable package will be checked after the
workflow completes.
