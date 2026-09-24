# silo-cbr-native

Standalone `no_std` RAR container and decompression implementation used by the
Silo source's default `cbr-native` feature. The crate uses only
`compcol = 0.6.11` as a normal dependency, with its default features disabled
and `rar3` and `rar5` enabled.

The public API is:

```rust
pub fn list_members(input: &[u8]) -> Result<Vec<Member>, Error>;
pub fn extract_member(input: &[u8], index: usize) -> Result<Vec<u8>, Error>;
```

RAR4 and RAR5 normal and solid archives are supported for the tested LZ and
stored paths. RAR4 solid groups use compcol's solid member API. RAR5 solid
groups concatenate packed data, add unpacked file boundaries, and decode with
the maximum dictionary declared by the group. Stored members are copied and
checked directly, including stored members in an all-stored solid group.

The parser enforces these bounds before allocation or decoding:

- archive input: 16 MiB
- each unpacked member: 16 MiB
- total unpacked members: 64 MiB
- entries: 512
- RAR5 dictionary: 8 MiB

Header CRCs, RAR4 low-16 CRC32 values, and available per-file output CRC32
values are checked. RAR4 accepts EOF at a complete block boundary without an
optional end header, as do the independently verified fixtures; a partial
header is rejected. Encrypted headers and entries, split or multivolume
archives, unknown unpacked sizes, redirections, unsupported flags, and
unsupported compression versions or methods are rejected.

The first block passed to a fresh RAR3 decoder is rejected when its first
compressed-data bit identifies PPMd. Solid continuation members are passed to
the same decoder without applying that check because retained Huffman state
means their first bit is not necessarily a new block header. In compcol
0.6.11, `Ppmd7::new` receives a stream-controlled memory value and can request
up to 256 MiB. An LZ stream can also switch to PPMd midstream, so this
prototype cannot claim that all PPMd is unsupported or that every RAR3 heap
allocation is covered by the public limits. A bounded compcol API or a local
dependency patch is required before enabling stronger PPMd guarantees. The
current behavior reports the fresh-block case as unsupported and documents the
remaining midstream allocation risk.

The crate consumes the complete archive slice. The `cbr-native` Aidoku source
feature downloads the full archive for every page and sorts image members in
natural filename order. The native path has a known midstream RAR3 PPMd allocation gap and has not been
validated on an Aidoku device, so it is not fully hardened. Convert CBR to CBZ
if an archive or device cannot use this path.

From the repository root:

```sh
cd sources/multi.silo
cargo test -- test_unit_          # includes the RAR fixture tests
cargo test --no-default-features  # build without CBR support
```

This remains an experimental CBR implementation while device validation and
the midstream PPMd allocation fix are pending.

Standalone parser checks:

```sh
cd crates/cbr-native
cargo test
cargo check --target wasm32-unknown-unknown
# After generating the larger fixtures as documented in ../../experiments/cbr-wasm/README.md:
cargo test -- --ignored
```

See the [report](../../docs/research/cbr-feasibility.md) for measured costs,
the full-archive download behavior and unsupported RAR variations.
