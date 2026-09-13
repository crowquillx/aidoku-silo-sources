# Silo source v5: optional Comic Pages plugin

Validated on 2026-09-13. Source v5 adds an optional integration with the separate
[Comic Pages plugin](https://github.com/crowquillx/silo-comic-pages). Installable
plugin releases are published for Linux amd64 and arm64; no live Silo server was
modified or received the plugin during this work.

Set **Reading → Comic Pages plugin installation ID** after installing and
configuring the plugin. Real RAR archives then use its authenticated page-list
and chunk endpoints. ZIP archives keep the existing central-directory/range
path, including ZIP archives mislabeled CBR. An empty setting preserves the
native CBR path and its v4 limits. A configured plugin error is reported without
silently switching to full-archive decoding on the device.

The plugin downloads an archive once, extracts RAR4/RAR5 including solid
archives, and caches page bytes. It rechecks Silo account/profile/chapter/file
access before serving each chunk. Tokens are in authenticated POST bodies and
headers, not page URLs or contexts. The source joins 1 MiB chunks into a page
bounded at 32 MiB. Cache revisions and profile IDs distinguish page URLs.

See the plugin's [protocol](https://github.com/crowquillx/silo-comic-pages/blob/main/docs/protocol.md)
and [validation report](https://github.com/crowquillx/silo-comic-pages/blob/main/docs/validation.md)
for exact dependency versions, licenses, executable sizes, resource limits,
fixture provenance, and measured extraction costs. The design follows the server
page APIs found in [the comparison with other sources](cbr-other-sources.md).

## Source verification

Rust 1.97.1; target `wasm32-unknown-unknown`; no `std`, WASI, or filesystem API
was added. `src/zip.rs` is unchanged from v4. All existing source traits remain.

Commands run from `sources/multi.silo`:

```sh
cargo fmt
cargo clippy --release --target wasm32-unknown-unknown
cargo test
# With python3 tests/mock_silo_v2.py already running:
cargo test -- --ignored
cargo clippy --release --target wasm32-unknown-unknown --no-default-features
cargo test --no-default-features -- --ignored
aidoku package
aidoku verify package.aix
```

The default suite passed 8 tests (3 mock tests ignored). All 3 mock tests passed
when explicitly enabled. The no-default-features mock suite passed 2 tests.
The live suite exercised the existing juniper CBZ path. The plugin mock covers
an API key taking precedence over a stale login token, preparation polling,
image chunk reassembly, truncated/error responses, and mismatched URL/profile
rejection. A separate native-plugin suite launches the actual Silo SDK gRPC
process and extracts real generated RAR4/RAR5 fixtures.

The local v5 package is 180,184 bytes; its WASM module is 423,785 bytes with
SHA-256 `e9b956c4d7d09fec458bb5f439cf9251066a953f6f7e27f9fdfa83fda6414032`.
`aidoku verify` accepts the existing minimum app version 0.7.0. The module does
not import the newer `net::set_timeout` API. GitHub Actions builds and publishes
the Pages artifact independently; release verification records its actual hash.

Aidoku's image request bridge preserves POST method and body according to the
pinned Swift code linked in the plugin protocol. No physical iOS device test or
live plugin installation was performed. These remain deployment validation
steps for an administrator choosing to install the release.
