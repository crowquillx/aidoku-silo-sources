# Historical direct Rust RAR decoding results

> These results were captured against source version 3, when both CBR features
> were default-off. Source version 4 now enables `cbr-native` by default and
> keeps `cbr-wasm` opt-in. The measurements below are retained unchanged.

Research captured on 2026-09-13 for the `wasm32-unknown-unknown` Aidoku source.
The question was whether a small Rust RAR container and decompression core could
replace the isolated std-enabled guest. **Yes:** the usable direct `no_std` core is
[`compcol 0.6.11`](https://crates.io/crates/compcol/0.6.11), but it decodes
member streams only and does not parse RAR containers. It is now usable through
the new [`silo-cbr-native` container parser](../../crates/cbr-native/README.md).
The `cbr-native` source feature lists and extracts the real RAR4/RAR5 fixtures,
including solid archives. The [combined report](cbr-feasibility.md) records
source package sizes, pinned-Wasm3 measurements and remaining limitations.

Exact commands and compact output are retained in
[`experiments/cbr-direct-rust/results.log`](../../experiments/cbr-direct-rust/results.log).
The probe crate pins every candidate in
[`experiments/cbr-direct-rust/Cargo.toml`](../../experiments/cbr-direct-rust/Cargo.toml)
and used `/tmp/cbr-direct-rust-target` for disposable build artifacts.

## Current source status

The direct compcol/container implementation is the native CBR backend in
source version 4 and is enabled by default. It reads tested RAR4 and RAR5
normal and solid archives. A CBR page request downloads the full archive for
each page. The native path limits archives to 16 MiB, each unpacked page to
16 MiB, total unpacked members to 64 MiB, entries to 512, and RAR5 dictionaries
to 8 MiB.

A known RAR3 PPMd gap remains. If an LZ stream switches to PPMd midstream,
compcol 0.6.11 can request up to 256 MiB, outside the stated native limits.
No Aidoku device validation has been completed, so the native CBR path is not
fully hardened. Convert CBR to CBZ when this support is unsuitable.

## `rars 0.9.4`

The published crate is version `0.9.4`, Rust 1.87, and declares
`MIT OR Apache-2.0` in its [crates.io metadata](https://crates.io/crates/rars/0.9.4)
and [manifest](https://raw.githubusercontent.com/bitplane/rars/v0.9.4/crates/rars/Cargo.toml).
Its published manifest has no `[features]` section. Consequently,
`default-features = false` has no meaningful effect on the crate’s use of Rust
`std` or on its target dependencies.

Normal dependencies resolve to AES 0.9, aho-corasick 1.1, getrandom 0.4,
HMAC 0.13, SHA-1 0.11, SHA-2 0.11, and zeroize 1.8/1.9; non-wasm targets
additionally select rayon 1.12. The production source has no `#![no_std]`
declaration and uses `std`. Its full Rust source is approximately 57,845 lines
including tests, with about 22,310 in `codec` and 1,098 in `crypto`.

The [ArchiveReader API](https://docs.rs/rars/0.9.4/rars/struct.ArchiveReader.html)
opens `&[u8]` through `ArchiveReader::read` and owns a `Vec<u8>` through
`ArchiveReader::read_owned`. The [Archive API](https://docs.rs/rars/0.9.4/rars/enum.Archive.html)
provides `members()` and `read_member_at(index, password)`, returning
`Result<Option<Vec<u8>>>`. There is no required `Cursor` API at this layer;
the low-level parsers also consume slices. The direct host test generated a
RAR5 archive and read its second member with `read(&bytes)` and
`read_member_at(1, None)`, passing one test.

The exact wasm probe also passed:

```text
CARGO_TARGET_DIR=/tmp/cbr-direct-rust-target \
  cargo build --target wasm32-unknown-unknown \
  --no-default-features --features rars --lib
Finished `dev` profile
```

That is a **std-on-wasm** build, not a `no_std` build. `cargo tree` shows the
wasm-bindgen/getrandom wasm-js path even with `default-features = false`.
Trying to force the unsupported getrandom backend failed in getrandom 0.4.3:

```text
error[E0599]: no associated function or constant named `WEB_CRYPTO`
found for struct `error::Error` in getrandom-0.4.3/src/error.rs:185
```

The isolated guest uses these in-memory APIs and removes dead
wasm-bindgen descriptors after linking. That produces the import-free guest;
this direct probe does not claim that the rars crate itself is no_std.

## `rars-format 0.3.2`

[`rars-format 0.3.2`](https://crates.io/crates/rars-format/0.3.2) is the older,
deprecated standalone implementation behind the higher-level crate. Its
metadata declares `MIT OR Apache-2.0`, its published package has about 20,027
Rust source lines, and it uses `std::fs`, `std::io`, and `Arc`. The host check
passed, but the wasm check failed in getrandom 0.4.3 with:

```text
The wasm32/64-unknown-unknown are not supported by default; you may need to
enable the "wasm_js" crate feature ...
```

It is neither a small no_std container core nor a direct replacement for the
custom guest.

## `unrar-rs 0.10.3`

[`unrar-rs 0.10.3`](https://crates.io/crates/unrar-rs/0.10.3) exposes
`RarArchive::open(reader: impl Read + Seek + Send + 'static)`, so
`Cursor<Vec<u8>>` is accepted, and indexed entries can be copied into a
`Vec<u8>`. It has approximately 77,289 Rust source lines and a large
std-oriented graph: blake2s-simd, crc-fast, filetime, libc, memchr, rayon,
reed-solomon, rmp-serde, serde, sha2, subtle, unicode normalization, and a
selectable crypto backend. Recovery code uses filesystem APIs; there is no
no_std marker.

The package uses a `LICENSE` file rather than a normal manifest SPDX field.
That file states GPLv3 with the unRAR license restriction for the RAR engine;
other files are described as GPLv3. This is a distribution blocker unless the
licensing position is accepted separately.

The no-default host check fails because no crypto backend is selected. The wasm
check fails with the same backend diagnostic plus wasm libc errors:

```text
unrar-rs needs a crypto backend: enable feature crypto-aws-lc (native,
default), crypto-host (wasm...), or crypto-rust)
error[E0412]: cannot find type `tm` in crate `libc`
error[E0425]: cannot find function `mktime` in crate `libc`
```

Even with a crypto selection, this remains a large std-oriented graph and does
not qualify as a small no_std source dependency.

## `unrar 0.5.8`

[`unrar 0.5.8`](https://crates.io/crates/unrar/0.5.8) declares
`MIT OR Apache-2.0` and has matching `LICENSE-MIT` and `LICENSE-APACHE` files.
It is only about 1,439 Rust source lines, but wraps
[`unrar_sys 0.5.8`](https://crates.io/crates/unrar-sys/0.5.8), a bundled C/C++
UnRAR implementation, and its API uses filesystem paths. The host check passed.
The wasm check stopped in the C build script:

```text
Compiler family detection failed ... failed to find tool "clang++"
cc-rs: failed to find tool "clang++"
```

This is not a pure Rust or no_std route and would add a native toolchain and
UnRAR licensing review.

## `compcol 0.6.11`

[`compcol 0.6.11`](https://crates.io/crates/compcol/0.6.11) is the strongest
direct core candidate. Its [manifest](https://github.com/KarpelesLab/compcol/blob/v0.6.11/Cargo.toml)
declares MIT, `#![no_std]`, and `#![forbid(unsafe_code)]`; it has no normal
dependencies. `rar3` enables alloc and PPMd, while `rar5` enables alloc. The
whole source is approximately 89,014 lines, but the RAR3 and RAR5 decoder
modules themselves are about 2,578 and 1,730 lines respectively.

The exact no_std target check passed:

```text
CARGO_TARGET_DIR=/tmp/cbr-direct-rust-target \
  cargo check --target wasm32-unknown-unknown --no-default-features \
  --features compcol-rar3,compcol-rar5 --lib
Finished `dev` profile
```

Its [documentation](https://docs.rs/compcol/0.6.11/compcol/) describes RAR3/RAR5
as raw compressed member-stream decoders. The caller must parse archive headers,
locate blocks, supply the unpack size, and handle the container’s flags,
checksums, encryption, and volumes. The core does not expose an `ArchiveReader`
or `read_member_at` equivalent. Upstream decoder tests passed independently:
31 RAR3 tests, 20 RAR5 tests, and 53 library tests.
The subsequent [raw stream probe](../../experiments/cbr-native-probe/results.log)
passed all small archive payloads, and the
[large probe](../../experiments/cbr-native-probe/large-results.log) passed all
four larger fixtures with complete byte comparisons. Stored entries need a
direct copy; RAR5 decoding needs the dictionary size from the container header,
rather than the decoder's default. Both details are handled in the new parser.

The standalone parser checks headers and CRCs, enumerates members, and feeds
independent entries or solid groups to compcol. It has eight host tests plus
an ignored large-fixture test and builds for the required no_std target.
The [engine harness](../../experiments/cbr-native-runner/) also passes the
actual pinned Wasm3 engine with its 200 KiB stack. It is now the preferred
prototype, rather than a hypothetical future port. Its limits do not cover
every RAR3 PPMd allocation: an LZ stream can switch to PPMd midstream and
request up to 256 MiB in compcol 0.6.11. This needs a decoder-level bound
before treating the native prototype as a resource-hardened general decoder.

## Verdict

`rars 0.9.4` compiles with `default-features = false` for wasm, but that means
std-on-wasm because the published crate has no feature switch for no_std. It
opens bytes through `ArchiveReader::read`/`read_owned`, enumerates with
`members()`, and extracts an archive-order member with `read_member_at`; a
Cursor is unnecessary. `rars-format` and `unrar-rs` do not provide a bounded
no_std route. `unrar` depends on bundled C/C++.

The direct compcol/container implementation is the current native CBR backend
for source version 4 and is enabled by default. It avoids nested interpretation
and rars's licensing discrepancy, and has measured real-fixture execution. The
custom std guest also works and remains available through the opt-in `cbr-wasm`
feature. It provides a Wasmi memory/fuel boundary, but costs substantially more
CPU and package space. No upstream issue or PR was created.

The rars license evidence needs an explicit distribution decision. The crate
metadata says `MIT OR Apache-2.0`, while the tagged repository’s
[COPYING file](https://raw.githubusercontent.com/bitplane/rars/v0.9.4/COPYING)
says “WTFPL licensed with 1 extra clause” and includes “don't blame me.” The
published package did not contain a matching top-level MIT or Apache license
text. This report therefore does not call rars unambiguously MIT/Apache; the
declared dual grant and repository COPYING text must both be accounted for.
