# CBR in an Aidoku Silo source

**CBR decoding is feasible under the stated constraints.** The preferred
prototype is `cbr-native`: a new RAR container parser using `compcol 0.6.11`
for direct `no_std` RAR3/RAR5 decompression. It reads the tested RAR4 and RAR5
archives, including solid archives, through the existing Aidoku page hooks.
The source stays `wasm32-unknown-unknown`, uses alloc and Aidoku's talc
allocator, and gets network data only through Aidoku requests. It has no
std, WASI or filesystem dependency.

The proposed wasm-in-wasm route also works: `cbr-wasm` interprets an
import-free rars guest with Wasmi. It is larger and substantially slower.
The initial investigation used version 3 with both CBR features off by
default. Version 4 enables `cbr-native` in the published source. `cbr-wasm`
remains an experimental alternative; select it with `--no-default-features`
to avoid also enabling native decoding. The ZIP decoder is unchanged.
See the [version 4 release record](cbr-v4-release.md) for release checks and
deployment evidence. The version 3 measurements below are preserved.

This establishes a working source implementation, not universal RAR support
or iPhone performance. The native path still needs stronger decoder-level resource bounds and
broader compatibility testing. Enabling it for version 4 does not establish
compatibility with every RAR archive or remove the documented limits.
Conversion remains the best operational fallback for a library today.

## Verdict by approach

| Approach | Actual result | Version, size and license |
| --- | --- | --- |
| Direct `compcol` with a container adapter | **Works**, including real compressed and solid fixtures in the pinned Wasm3 engine. Preferred prototype. | `compcol 0.6.11`, MIT, no normal dependencies, `default-features=false`, `rar3` and `rar5`. Final source sizes below. |
| Direct `rars` | Default-features-disabled wasm build succeeds, but it still uses std. Not a direct no_std dependency. | `0.9.4`; about 57,845 Rust LOC including tests, codec 22,310 LOC. Manifest says MIT OR Apache-2.0; repository COPYING differs. |
| Older `rars-format` | Host check succeeds; wasm check fails in getrandom's unsupported backend. Uses std filesystem/I/O/Arc. | `0.3.2`; about 20,027 Rust LOC; declared MIT OR Apache-2.0. No qualifying final source binary. |
| `unrar-rs` | No-default build fails on missing crypto backend; wasm also fails on libc `tm`/`mktime`. Uses std. | `0.10.3`; about 77,289 Rust LOC; packaged LICENSE says GPLv3 with an UnRAR restriction. No qualifying binary. |
| `unrar` bindings | Host check succeeds. Wasm build stops on missing `clang++`; this is a toolchain failure, not proof C++ cannot target wasm. Its path-based C++/libc API still needs a port. | `unrar 0.5.8` + `unrar_sys 0.5.8`; wrapper MIT/Apache, underlying UnRAR separate terms. No qualifying binary. |
| Wasmi, no defaults | Builds and executes under the required no_std target and the pinned Wasm3 engine. | `2.0.0`, MIT OR Apache-2.0. Minimal linked Aidoku/Wasmi execution probe: **1,213,775 B**. |
| Stock npm rars guest | Wasmi validates it; empty linker fails because JS glue is required. | `@bitplane/rars 0.9.4`, **1,053,743 B**, 20 imports, initial memory 1,179,648 B; rars license discrepancy applies. |
| Stock node-unrar-js guest | Wasmi validates it; empty linker fails on Emscripten/Embind callbacks. Used successfully through its ordinary Node wrapper to verify fixtures. | `node-unrar-js 2.0.2`, **207,593 B**, 64 imports, initial memory 16,777,216 B, declared maximum 2 GiB. Wrapper MIT; UnRAR terms separate. |
| Custom rars guest + Wasmi | **Works** with a memory-only ABI and zero imports. Larger solid case exceeds the configured fuel budget. | `rars 0.9.4` + `wasmi 2.0.0`; guest and package sizes below. |
| Silo plugin | Native-process architecture permits a decoder subprocess; feasible fallback without upstream changes. No plugin was built because the source prototypes work. | No artifact, size or CPU measurement. SDK Apache-2.0; chosen decoder's license would also apply. |
| JavaScriptCore | API 0.7 implements JsContext, but the bare iOS context does not establish WebAssembly availability. Not a demonstrated route. | No decoder built or timed. Boa behavior does not prove device behavior. |
| Lazy RAR over ranges | Feasible for independent entries after indexing headers. Solid pages require earlier dictionary state. Not implemented here. | No separate artifact or timing; format analysis below. |

Exact direct-crate commands, error excerpts, dependency graphs and core tests
are in [direct Rust evidence](cbr-direct-rust.md) and its linked probe log.
A target build alone does not prove no_std: rars is a concrete counterexample.
Its codec could be ported by replacing std I/O/error/state interfaces and
separating file, crypto and writer modules, but this task found and used the
already-no_std core instead. The deprecated rars-format crate is not that core.

## Version 3 benchmark artifacts

| Build | `main.wasm` B | `.aix` B | Package increase over final default |
| --- | ---: | ---: | ---: |
| Default CBZ | 310,581 | 130,127 | — |
| `cbr-native` | **403,229** | **170,204** | **40,077 B / 30.8%** |
| `cbr-wasm` | 1,928,039 | 556,405 | 426,278 B / 327.6% |

All three packages pass `aidoku verify`; source version is 3 in each.
The starting package was 129,860 B with a 310,219 B module. The final default
adds the shared API-key auth correction and a guard for stale CBR page context.
The ZIP implementation itself is unchanged.

[Exact artifact hashes](../../experiments/cbr-wasm/evidence/source-packages.json)
and [module interfaces](../../experiments/cbr-wasm/evidence/source-module-interfaces.json)
are saved. All three source modules import the same 19 Aidoku host functions
from defaults/env/std/net/canvas. The host namespace named `std` is Aidoku's
value/error interface, not Rust libstd or a filesystem API. There are no
WASI or JS imports. The native feature needs neither Node nor the guest build.
Package ZIP timestamps may change archive hashes on rebuild; module hashes
identify the actual compiled payloads.

## Preferred native implementation

[`experiments/cbr-native`](../../crates/cbr-native/README.md) owns the
container parser and a small `list_members` / `extract_member` API. Its sole
normal dependency is [compcol 0.6.11](https://docs.rs/compcol/0.6.11/compcol/),
whose [manifest and source](https://github.com/KarpelesLab/compcol/tree/v0.6.11)
declare MIT, no_std and no unsafe code. RAR3 and RAR5 raw decoders are about
2,578 and 1,730 LOC; the larger crate also contains unrelated codecs which
are not enabled by this feature selection.

The new parser validates header CRCs and lengths, checks available output
CRCs, and rejects unsupported flags, encrypted/split/multivolume entries,
redirections and unknown unpacked sizes. Independent compressed members decode
without processing unrelated members. Solid groups replay their prerequisite
streams; stored entries copy their bytes directly. The source adapter filters
image names, naturally sorts them, and preserves their original archive indices.
Page requests use full-archive ranges with authentication and profile headers.
The shared image-auth helper now selects the configured API key even if a
stale password access token remains in defaults.

Declared input and member sizes are capped at 16 MiB each, total unpacked
size at 64 MiB, count at 512, and RAR5 dictionary at 8 MiB. These are **not a
complete heap or CPU boundary**. A fresh RAR3 stream beginning with PPMd is
rejected, but an LZ stream can switch to PPMd midstream; compcol 0.6.11 can
then request up to 256 MiB for its model. There is no native fuel counter or
wall-clock cancellation hook. Fixing this requires a decoder-level PPMd
allocation limit/disable option, implementable in a local dependency patch
without an upstream change. The prototype does not claim every PPMd, VM filter,
legacy compression version, RAR7 archive, mixed solid group or filename encoding
works. Read the parser's explicit errors and tests for the supported subset.

## Engine measurements

Host: Intel Core i7-8700, Linux x86_64; Rust 1.97.1, Node 22.23.2.
These are single-run desktop measurements, not iPhone latency. They exclude
network and image rendering. Small tests assert complete output-byte equality;
large engine runs check length and decoder CRCs, with complete large output
also checked by the host probe and independently by UnRAR.

Current API 0.7 [AidokuRunner](https://github.com/Aidoku/AidokuRunner/blob/cc4d06ff399e7169b9c647bccede7cb29bc805c6/Sources/AidokuRunner/Interpreter.swift#L35)
uses a **200 KiB stack**, not the legacy loader's 512 KiB. The C harness uses
the pinned [Skittyblock/Wasm3 revision](https://github.com/Skittyblock/Wasm3/tree/6a64d8bfc6a235f1ee3f8b57f692494fbf72ed19),
204,800 stack bytes, normal bounds/call-depth defaults and duplicate-function
setting 10. It links only env.print and env.abort. Local file reads belong
to the C test harness, not the source. Aidoku's Rust test runner uses Wasmer,
so these engine checks provide additional evidence of actual host compatibility.

| Fixture | Archive B | Page B | Native Wasm3 | Nested Wasmi in Wasm3 |
| --- | ---: | ---: | ---: | ---: |
| RAR4 normal, three 128×128 PNGs | 5,999 | 49,348 | 4.881 ms | 686 ms |
| RAR4 solid | 4,040 | 49,348 | 6.185 ms | 605 ms |
| RAR5 normal | 4,815 | 49,348 | 8.352 ms | 674 ms |
| RAR5 solid | 127,742 | 49,348 | 40.902 ms | 4,363 ms |
| RAR5 normal, three 512×768 PNGs; stored method | 3,542,218 | 1,180,659 | 19.425 ms | 7,475 ms |
| RAR5 solid, three 512×768 PNGs; compressed | 3,541,868 | 1,180,659 | 987.149 ms | Fuel error after 6,757 ms |

Final native benchmark module: **299,195 B**, including embedded small fixtures
and expected PNG. Small listing took 0.031–0.496 ms. Small extraction reached
5,767,168 B of outer linear memory. Large normal/solid runs reached
6,160,384 / 25,755,648 B of linear memory and 11,140 / 28,204 KiB host RSS.
The harness's `fuel_limit=0` field is an unused ABI argument on the native path;
it does not mean that native work is metered.

The table's nested measurements use the 407,709 B guest snapshot before the final
redirection-rejection fix, with an outer benchmark module of 1,835,641 B.
Small listing took 145–254 ms, small outer memory reached 5,832,704 B, and
guest memory reached 1,769,472 B. Large normal/solid runs reached
21,430,272 B outer memory and 8,323,072 B guest memory, with 30,184 / 30,052 KiB
RSS. Each nested call creates, validates and instantiates a fresh Wasmi guest;
these setup and copy costs are included. The final guest adds rejection of
RAR5 redirections before listing and extraction, preventing divergent indices.
The rebuilt guest is **407,843 B**, SHA-256
`98e64ae78c87093e0f3c5629f6cef20c650c2892509338f9f03ce7afeea124cf`, with
zero imports. A final pinned-Wasm3 smoke run also passes all four archives:
725 / 652 / 748 / 4,872 ms, with the same 5,832,704 B outer high water.
That final benchmark module is 1,835,769 B. Both snapshots' logs are retained.

Guest memory is part of outer memory; do not add the columns. Freeing decoder
state permits reuse but cannot shrink wasm linear memory. RSS includes the C
host and translated code. App networking/image buffers and concurrent requests
are not included. The engine's theoretical memory maximum does not prove that
an iOS process can safely use it. There was no physical-device test.

For diagnosis only, a larger solid nested run in Node/V8 with 12 billion fuel
completed in 21,596 ms, used about 9.29 billion fuel and reached 78,184,448 B
outer memory. The source uses **500 million fuel** and **128 MiB guest memory**
per invocation. Raising fuel would permit longer stalls; fuel is not a
wall-clock deadline. Logs for both successful and failed probes are retained
under the experiment directories.

## Nested ABI and proposals

The guest compiles rars's std-enabled in-memory APIs into a separate binary.
The Aidoku source does not link rars or std. Only `memory`, `input_alloc` and
`rar_call` are exported. The host copies archive bytes to an allocated guest
buffer; `rar_call(ptr,len,-1)` returns metadata, and a nonnegative index returns
that member's bytes. A u64 packs output length and pointer. Bounds-checked
copies bring the result back; the fresh instance is then dropped. No shared
memory, persistent archive session, JS glue, WASI or filesystem import is used.

Default-features=false alone left dead wasm-bindgen descriptor imports.
[The build script](../../experiments/cbr-wasm/build.sh) removes non-ABI exports,
then Binaryen 132.0.0 runs `remove-unused-module-elements,dce,vacuum,strip`
and asserts zero imports. An attempted getrandom `unsupported` backend failed
in getrandom 0.4.3 with a WEB_CRYPTO configuration error; the final build needs
no replacement randomness backend. No random operation is invoked.

Wasmi [2.0.0 features](https://github.com/wasmi-labs/wasmi/blob/v2.0.0/crates/wasmi/Cargo.toml)
used here are `stable`, `validate`, `portable-dispatch`, `libm`, and
`prefer-btree-collections`, with defaults disabled. Neither SIMD nor memory64
is enabled. Both stock modules validate, so their blocker is imported glue,
not an observed unsupported wasm proposal. No stock nested CPU timing is
claimed because those imports were not implemented.

## Downloads and lazy ranges

Both source prototypes download the complete archive once to list pages, then
again for each requested page: approximately **N+1 full downloads for N pages**,
before retries and prefetch. There is no persistent archive cache. The nested
rars [read_member_at implementation](https://github.com/bitplane/rars/blob/v0.9.4/crates/rars/src/lib.rs#L447)
also processes the archive on each call. The native path decodes only the
required independent member or solid group, but currently receives the same
full archive. A 28 MB CBR is rejected by the prototype's 16 MiB archive cap;
the existing larger CBZ range path is unaffected.

Responses must have the expected full length and HTTP 200 or 206. Aidoku's
network API buffers bytes before the source checks their length, so a server
lying about HEAD/range sizes can still cause a larger host allocation. These
checks are not a streaming response-memory guarantee.

The [RAR5 specification](https://www.rarlab.com/technote.htm) describes per-file
headers, data sizes, solid flags, encryption and optional quick-open records.
Non-solid archives can be indexed by range-reading headers and skipping each
packed payload. This needs multiple scattered requests rather than ZIP's
usual tail lookup; optional quick-open copies do not cover every archive.
The native parser now provides the container knowledge needed for a future
range-backed reader, but its current API accepts a complete slice.

Solid files depend on earlier dictionary state. Random page access requires
replay, retained sessions/checkpoints, or a server cache/conversion. Ranges
can avoid unrelated payloads but cannot remove this decompression dependency.
Multi-volume and encrypted headers need additional state and are rejected.

## Plugin, JS and licenses

The [runtime investigation](cbr-runtime-routes.md) records exact local Silo
and SDK revisions, primary-source lines and current AidokuRunner behavior.
Silo plugins run as native gRPC subprocesses and may invoke unrar/bsdtar.
HTTP routes are `/api/v2/plugin-content/plugins/{installation_id}/*` and the
legacy `/api/v1/plugins/{installation_id}/*`, with buffered bodies and a
10-second route deadline. Normal client credentials authenticate at Silo,
but Authorization/X-Profile-Id are not forwarded unchanged; the plugin gets
trusted user identity and profile display metadata. Chapter/profile access
is not automatically established by having a route. The RuntimeHost API does
not expose arbitrary archive bytes or filesystem paths, so a plugin needs an
explicit configured library-root mapping or another authorized file handoff.
Binary manifest probing and admin upload/catalog installation are supported.
This is feasible local work, not a requirement for an upstream PR.

AidokuRunner does link JsContext for API 0.7; an earlier look at the legacy
loader would give the wrong answer. The host evaluates strings in a bare
JavaScriptCore context, with no injected WebAssembly implementation. The Swift
wrapper documents iOS WebAssembly restrictions. No physical-device feature
probe was run, so this report does not claim a universal impossibility across
all Apple platforms. It is not a viable demonstrated replacement for the
working native decoder. Boa in the test runner is a different engine.

The native route uses MIT compcol and this repository's MIT/Apache code.
Wasmi is MIT/Apache. rars's [workspace manifest](https://github.com/bitplane/rars/blob/v0.9.4/Cargo.toml#L9)
declares MIT OR Apache-2.0, while tagged [COPYING](https://github.com/bitplane/rars/blob/v0.9.4/COPYING)
says WTFPL with an additional disclaimer; the published crate lacks matching
top-level MIT/Apache texts. Preserve this discrepancy when considering
redistribution. The nested guest is generated locally and ignored by git.
Resolved outer and guest manifest licenses are saved in the evidence JSON.

The node-unrar-js [wrapper is MIT](https://github.com/YuJianrong/node-unrar.js/blob/8c615868c9e2ec5ef2e661c5bcaee20ffad3862c/LICENSE.md),
while its [UnRAR license](https://github.com/YuJianrong/node-unrar.js/blob/8c615868c9e2ec5ef2e661c5bcaee20ffad3862c/UnRarDLL_Doc/license.txt)
and [RARLAB restrictions](https://www.rarlab.com/license.htm) are separate.
Do not infer a bundled decoder's license from its wrapper. Binaryen is a
build-time tool, not part of the source's runtime interpreter.

## Reproduction

```sh
# Repository root: preferred, no Node or guest build needed
scripts/package-cbr-prototype.sh cbr-native
# Alternative comparison package, including deterministic guest build
scripts/package-cbr-prototype.sh cbr-wasm
```

The CLI does not accept Cargo features, so the helper first runs `aidoku
package`, builds the selected feature and replaces Payload/main.wasm in a
separate package, then verifies it. The default package is retained and now includes native CBR support. The
helper disables default Cargo features when building either comparison
package. If both features are enabled in Cargo, the native adapter takes
precedence. Build with `--no-default-features` for the CBZ-only comparison.

[Native instructions](../../crates/cbr-native/README.md),
[nested instructions](../../experiments/cbr-wasm/README.md) and the
[engine harness](../../experiments/cbr-native-runner/README.md) give exact
commands. [Fixtures](../../crates/cbr-native/fixtures/README.md) contain our
own generated PNGs written to RAR by rars 0.9.4, with all extracted bytes
independently verified by node-unrar-js 2.0.2. No proprietary comic sample or
proprietary RAR writer was needed. Small files and hashes are included; larger
fixtures are reproducibly generated into ignored artifacts.

For ordinary use, the existing [conversion script](../../scripts/convert-cbr-to-cbz.sh)
remains ready. To preserve CBR originals while avoiding client CPU/repeated
network transfer, a local plugin could cache CBZ conversions or extracted
pages once it has the explicit file-access and authorization contract above.

## Validation and remaining work

These commands completed successfully during the version 3 investigation,
when the default feature set was empty. For version 4, use
`--no-default-features --features cbr-wasm` for nested testing and
`--no-default-features` for CBZ-only testing. Current release checks are in
the [release record](cbr-v4-release.md).

```sh
cd sources/multi.silo
cargo fmt
cargo clippy --release --target wasm32-unknown-unknown
cargo test
aidoku package
aidoku verify package.aix
cargo clippy --release --target wasm32-unknown-unknown --features cbr-native
cargo test --features cbr-native
cargo clippy --release --target wasm32-unknown-unknown --features cbr-wasm
cargo test --features cbr-wasm
cargo clippy --release --target wasm32-unknown-unknown --all-features
# With tests/mock_silo_v2.py running:
cargo test -- --ignored
cargo test --features cbr-native -- --ignored
cargo test --features cbr-wasm -- --ignored
```

Default source: **6 passed**, plus **1 mock test**. Each CBR feature:
**8 passed**, plus **2 mock tests**. Both feature runs retain the live browse
and first-page CBZ tests against the supplied test server. RAR tests assert
natural order with archive indices [2,1,0], exact page bytes for all four
fixtures, HTTP 200/206, malformed/truncated input and resource errors. The
mock requires bearer/API-key authentication and profile p1, and deliberately
leaves a stale access token configured in API-key mode. The live test decodes
the supplied small CBZ; no claim is made that a real server CBR was tested.

The native parser additionally passes **8 host tests**, **1 generated-large
fixture test**, and a no_std wasm check. The outer Wasmi probe passes **3
release tests**, including stock-module validation and exact fixture output.
The guest's redirection regression passes on the host target. Directly running
the unprepared guest's wasm unit-test binary fails on retained wasm-bindgen
descriptors; the documented prepared-guest tests pass. The full nested source
fixture test takes much longer in the debug test build (139 seconds for the
suite) than the optimized release engine measurements.

There is a test-host limitation at the successful `process_page_image` boundary:
`ImageRef::new` in the pinned test runner decodes its argument as an image and
cannot construct an ImageRef holding raw RAR data. The successful path is tested
through the real HTTP request, source context and decoder, while a public
processing-hook test checks clean failure for malformed responses at both
200 and 206. This is **not** a successful raw-RAR ImageRef round trip. The app's
raw-response behavior supplied in the task, the existing CBZ hook, and the
pinned-Wasm3 decoder tests support feasibility; physical-device validation
remains necessary. No upstream runner was changed to make tests pass.

Review found and fixed stale API-key image auth and RAR5 redirection index
mismatch. A suggested mandatory RAR4 end-header check was tested and rejected:
the independently UnRAR-verified valid fixtures omit that optional header.
The added regression covers both complete forms and rejects a partial end
header. As with other formats permitting EOF at a member boundary, the parser
cannot detect loss of entire final members if no end marker or external length
reveals it. Expected HTTP archive lengths still catch truncated responses.

Follow-up work after the version 4 release: constrain RAR3 PPMd allocation and native work,
exercise a broader independently created CBR corpus (including names, filters,
services and solid boundaries), test on an iOS device, and decide whether to
implement lazy ranges or a server cache for repeated archive downloads.
The initial investigation made no upstream issue/PR, plugin installation or
Silo working-tree edit. Its version 3 packages were local. The subsequent
version 4 release is documented separately.
