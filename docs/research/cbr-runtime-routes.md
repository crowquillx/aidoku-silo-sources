# CBR fallback: runtime and Silo route research

As of 2026-09-13. This is a source and contract review; there was no device run or device verification.

## Runtime verdict

Aidoku API 0.7 sources are loaded by the separately pinned `AidokuRunner` package, commit [`cc4d06ff399e7169b9c647bccede7cb29bc805c6`](https://github.com/Aidoku/AidokuRunner/tree/cc4d06ff399e7169b9c647bccede7cb29bc805c6), from app commit [`73c55ffa685b9edbbd5a269c9a0de18d5dc4f43b`](https://github.com/Aidoku/Aidoku/tree/73c55ffa685b9edbbd5a269c9a0de18d5dc4f43b). Its `Interpreter` uses the Swift Wasm3 wrapper, pinned by Aidoku to [`6a64d8bfc6a235f1ee3f8b57f692494fbf72ed19`](https://github.com/Skittyblock/Wasm3/tree/6a64d8bfc6a235f1ee3f8b57f692494fbf72ed19), not Wasmer or Wasmtime. The current non-Legacy `Source` loader reads `source.json` and `main.wasm`, then creates `Interpreter` with its default **200 KiB stack**. `Interpreter.linkImports` links `env`, `std`, `defaults`, `net`, `html`, `js`, and `canvas` (UIKit builds). The app’s `Legacy/Source.swift` is only the API 0.6 compatibility path and must not be used for the API 0.7 limits.

The pinned wrapper config sets `d_m3MaxFunctionStackHeight` to **2,000**, `d_m3MaxCallDepth` to **256**, enables bounds checks, and defaults `d_m3MaxLinearMemoryPages` to **65,536**. That is the 32-bit Wasm page ceiling (4 GiB at 64 KiB/page), not a device memory budget. The Swift source does not call `resizeMemory` or set a smaller memory limit. The C runtime has a memory-limit field, but this Aidoku setup does not populate it. Actual allocation is therefore constrained by the module’s declarations, `memory.grow`, the Wasm3 ceiling, and device availability.

There is no general wall-clock timeout, fuel budget, or watchdog around API 0.7 source calls in `Interpreter`; its actor methods call Wasm functions directly. A nested decoder must bound its own work and memory. Artifact sizes and pinned-Wasm3 GCC extraction results are in the companion [CBR feasibility report](cbr-feasibility.md), which owns the final engine test and performance numbers. This runtime report does not claim device verification.

The Aidoku test runner is a separate process using **Wasmer 6.1**, with Boa **0.21** for the `js` test imports. It is not Wasmtime. The runner creates a fresh Wasmer store/module/instance per test and exposes no explicit timeout, fuel, or memory budget. Boa `context_eval` stringifies the result; `context_eval_async` and WebView functions return unimplemented errors. A passing nested test establishes Wasmer/Boa behavior only.

The API 0.7 host does register `js::context_create`, `context_eval`, `context_eval_async`, and `context_get` with JavaScriptCore, plus WebView imports. Evaluation reads a UTF-8 string from Wasm memory, calls `JSContext.evaluateScript`, and marshals the result back as a stored string/RID; `context_get` does the same for a global property. Async evaluation wraps the expression in an async IIFE and waits through a blocking bridge. This is JavaScriptCore evaluation, not a Wasm3-in-JS bridge. The host creates a bare `JSContext`; it does not inject a `WebAssembly` object. The Wasm3 wrapper documents that WebAssembly is disabled in JavaScriptCore on iOS. Do not assume a native `JSContext` has a usable `WebAssembly` global. Both the direct native decoder and the nested guest avoid JS imports and have passed the pinned Wasm3 engine; the test runner alone would not have established that result. Memory64 is also not a safe target: the production engine’s memory and page APIs are `u32` based. SIMD and other proposal support were not established by the runner result and must be treated as unverified until the exact binary is exercised by this Wasm3 build.

Primary runtime sources: [AidokuRunner `Source.swift#L104-L168`](https://github.com/Aidoku/AidokuRunner/blob/cc4d06ff399e7169b9c647bccede7cb29bc805c6/Sources/AidokuRunner/Source.swift#L104-L168), [AidokuRunner `Interpreter.swift#L24-L50`](https://github.com/Aidoku/AidokuRunner/blob/cc4d06ff399e7169b9c647bccede7cb29bc805c6/Sources/AidokuRunner/Interpreter.swift#L24-L50), [AidokuRunner `Interpreter.swift#L92-L132`](https://github.com/Aidoku/AidokuRunner/blob/cc4d06ff399e7169b9c647bccede7cb29bc805c6/Sources/AidokuRunner/Interpreter.swift#L92-L132), [AidokuRunner `JavaScript.swift#L13-L45`](https://github.com/Aidoku/AidokuRunner/blob/cc4d06ff399e7169b9c647bccede7cb29bc805c6/Sources/AidokuRunner/Imports/JavaScript.swift#L13-L45), [AidokuRunner `JavaScript.swift#L60-L137`](https://github.com/Aidoku/AidokuRunner/blob/cc4d06ff399e7169b9c647bccede7cb29bc805c6/Sources/AidokuRunner/Imports/JavaScript.swift#L60-L137), [AidokuRunner `IsolatedJSContext.swift#L10-L59`](https://github.com/Aidoku/AidokuRunner/blob/cc4d06ff399e7169b9c647bccede7cb29bc805c6/Sources/AidokuRunner/Utilities/IsolatedJSContext.swift#L10-L59), [Aidoku `Package.resolved#L5-L11`](https://github.com/Aidoku/Aidoku/blob/73c55ffa685b9edbbd5a269c9a0de18d5dc4f43b/Aidoku.xcodeproj/project.xcworkspace/xcshareddata/swiftpm/Package.resolved#L5-L11), [Wasm3 config#L19-L25](https://github.com/Skittyblock/Wasm3/blob/6a64d8bfc6a235f1ee3f8b57f692494fbf72ed19/Sources/wasm3-c/include/m3_config.h#L19-L25), [Wasm3 call-depth config#L154-L158](https://github.com/Skittyblock/Wasm3/blob/6a64d8bfc6a235f1ee3f8b57f692494fbf72ed19/Sources/wasm3-c/include/m3_config.h#L154-L158), [Wasm3 runtime wrapper](https://github.com/Skittyblock/Wasm3/blob/6a64d8bfc6a235f1ee3f8b57f692494fbf72ed19/Sources/Wasm3/Runtime.swift), [`aidoku-rs` JS imports](https://github.com/Aidoku/aidoku-rs/blob/e1320b0a2e11afb59e4dee374883a2212d325699/crates/lib/src/imports/js.rs), and [the test runner](https://github.com/Aidoku/aidoku-rs/blob/e1320b0a2e11afb59e4dee374883a2212d325699/crates/test-runner/src/imports/js.rs).

## Silo plugin process and contract

The reviewed Silo server ref is [`0416528027bee67b1fda8e78f06a3d5dfc4e4137`](https://github.com/Silo-Server/silo-server/tree/0416528027bee67b1fda8e78f06a3d5dfc4e4137). In [`internal/pluginhost/host.go#L105-L138`](https://github.com/Silo-Server/silo-server/blob/0416528027bee67b1fda8e78f06a3d5dfc4e4137/internal/pluginhost/host.go#L105-L138), plugin execution is a native executable started with Go `exec.Command(binaryPath)` and HashiCorp `go-plugin` gRPC. The host code has no seccomp, chroot, container, rlimit, or other sandbox setup. It does not invoke a shell. A plugin can shell out with its own `os/exec`, inheriting the plugin process user’s filesystem and OS privileges. Treat an installed plugin as trusted native code.

The host applies per-call deadlines from [`internal/pluginhost/handshake.go#L11-L31`](https://github.com/Silo-Server/silo-server/blob/0416528027bee67b1fda8e78f06a3d5dfc4e4137/internal/pluginhost/handshake.go#L11-L31). The relevant defaults are **10 seconds for HTTP routes**, **10 seconds for control/auth/events**, **30 seconds for metadata**, **5 minutes for analyzers and scan-source polling**, and **60 seconds for request-router calls**. The [`internal/plugins/http_proxy.go#L118-L182`](https://github.com/Silo-Server/silo-server/blob/0416528027bee67b1fda8e78f06a3d5dfc4e4137/internal/plugins/http_proxy.go#L118-L182) proxy reads the entire request body before the gRPC call and exposes no streaming or WebSocket path. A route response is limited to the contract’s filtered headers and buffered body.

The plugin takes:

- an executable named `plugin` with the SDK handshake and manifest command;
- a manifest returned by running the temporary binary as `plugin manifest` during installation, with a **5-second** probe deadline;
- registered capabilities, including `http_routes.v1` for dynamic routes and static assets;
- an HTTP route request containing method, plugin-relative path, query, selected request headers, and buffered body;
- trusted host identity headers such as `X-Silo-User-Id` and `X-Silo-User-Role`, plus profile display metadata when available;
- the SDK RuntimeHost APIs, including user-scoped library metadata.

The pinned SDK ref is [`f110653047449de7220f8fb9f7ce49ddecc7d9a0`](https://github.com/Silo-Server/silo-plugin-sdk/tree/f110653047449de7220f8fb9f7ce49ddecc7d9a0). Its [`http_routes.proto`](https://github.com/Silo-Server/silo-plugin-sdk/blob/f110653047449de7220f8fb9f7ce49ddecc7d9a0/proto/silo/plugin/v1/http_routes.proto) defines the method/path/header/query/body request and response contract; [`runtime_host.proto`](https://github.com/Silo-Server/silo-plugin-sdk/blob/f110653047449de7220f8fb9f7ce49ddecc7d9a0/proto/silo/plugin/v1/runtime_host.proto) is the host callback surface. The plugin contract does not provide a library archive file path or a raw archive streaming API. The inspected RuntimeHost surface can list user libraries/media metadata; it is not evidence that a plugin can open Silo’s private archive files directly. The SDK is Apache License 2.0 under [`LICENSE`](https://github.com/Silo-Server/silo-plugin-sdk/blob/f110653047449de7220f8fb9f7ce49ddecc7d9a0/LICENSE).

## Exact route surface

The current browser/plugin content surface is:

| Purpose | Exact path | Method/access |
| --- | --- | --- |
| Route capability discovery | `/api/v2/plugin-content/capabilities` | `GET` |
| Dynamic plugin content | `/api/v2/plugin-content/plugins/{installation_id}/*` | all nine HTTP methods; descriptor-controlled |
| Plugin static assets | `/api/v2/plugin-content/plugin-assets/{installation_id}/*` | `GET` only |
| Browser launch cookie | `/api/v2/auth/plugin-launch` | `POST`, logged-in session required |
| Legacy dynamic content | `/api/v1/plugins/{installation_id}/*` | all nine HTTP methods; descriptor-controlled |
| Legacy static assets | `/api/v1/plugin-assets/{installation_id}/*` | `GET` |

Dynamic descriptors choose the plugin-relative path, method, body/media behavior, and one of `public`, `authenticated`, or `admin` access. The host checks that class before dispatch. `authenticated` permits a logged-in user or admin; `admin` returns forbidden to an ordinary authenticated user. This check is route-level only; it is not item-level library authorization.

The plugin lifecycle/catalog API in the pinned server’s [`internal/api/router.go#L3799-L3820`](https://github.com/Silo-Server/silo-server/blob/0416528027bee67b1fda8e78f06a3d5dfc4e4137/internal/api/router.go#L3799-L3820) is under the admin-only v1 namespace:

```text
GET    /api/v1/admin/plugins/catalog
GET/PUT /api/v1/admin/plugins/catalog-settings
GET/POST /api/v1/admin/plugins/repositories
PUT/DELETE /api/v1/admin/plugins/repositories/{id}
GET/POST /api/v1/admin/plugins/installations
PUT    /api/v1/admin/plugins/installations/{id}
POST   /api/v1/admin/plugins/installations/{id}/update
PUT    /api/v1/admin/plugins/installations/{id}/config
POST   /api/v1/admin/plugins/installations/{id}/config/test
PUT    /api/v1/admin/plugins/installations/{id}/auth-binding
PUT    /api/v1/admin/plugins/installations/{id}/task-bindings/{capability_id}
DELETE /api/v1/admin/plugins/installations/{id}
POST   /api/v1/admin/plugins/uploads
POST   /api/v1/admin/plugins/uploads/chunked
PUT    /api/v1/admin/plugins/uploads/chunked/{upload_id}/chunks/{chunk_index}
POST   /api/v1/admin/plugins/uploads/chunked/{upload_id}/complete
DELETE /api/v1/admin/plugins/uploads/chunked/{upload_id}
```

There is no HTTP `/manifest` content route. [`internal/plugins/installer.go#L593-L607`](https://github.com/Silo-Server/silo-server/blob/0416528027bee67b1fda8e78f06a3d5dfc4e4137/internal/plugins/installer.go#L593-L607) probes the uploaded/downloaded executable with the `manifest` argument. A non-ZIP upload is treated as a binary; ZIP uploads are unpacked and the contained plugin binary is probed. The admin upload handler expects the multipart field `archive`.

## Authentication and library authorization

The Aidoku source client authenticates native Silo API calls with `Authorization: Bearer <token>` and sends `X-Profile-Id`; PIN-protected profiles may also require `X-Profile-Token`. Its normal read calls are `/api/{v1|v2}/user/libraries`, `/api/{v1|v2}/catalog`, and `/api/{v1|v2}/catalog/items/{id}` plus episode/media paths.

For plugin content, the Silo host consumes the bearer/session/API-key or launch-cookie credential to resolve the request before dispatch. In [`internal/plugins/http_proxy.go#L118-L182`](https://github.com/Silo-Server/silo-server/blob/0416528027bee67b1fda8e78f06a3d5dfc4e4137/internal/plugins/http_proxy.go#L118-L182) and its [`forwardedRequestHeaders` allow-list#L295-L338](https://github.com/Silo-Server/silo-server/blob/0416528027bee67b1fda8e78f06a3d5dfc4e4137/internal/plugins/http_proxy.go#L295-L338), ordinary negotiation/request headers (`Accept`, `Content-Type`, `Range`, cache validators, origin, referer, and user agent) are forwarded; `Authorization` and `X-Profile-Id` are not. The plugin instead receives trusted `X-Silo-User-Id`/role headers and selected profile name/primary metadata. Therefore a plugin must not expect to re-authenticate by reading the caller’s bearer token, and it cannot infer the active profile ID from the forwarded route request alone.

The host route check does not prove that a requested manga, library, chapter, or file belongs to that user/profile. A CBR route must enforce that mapping itself using an available user-scoped RuntimeHost/API contract. The pinned evidence does not show a plugin API for opening arbitrary Silo archive files, so the current contract does not provide a transparent replacement for the native `/ebooks/{chapter}/files/{file_id}/read` path. The work remains feasible as a trusted native plugin if the archive-byte/root-path handoff, identity/profile authorization, and Aidoku source route are defined. No server modification or plugin implementation is proposed here.

## Feasibility and blockers

Both source prototypes passed the pinned Wasm3 engine at the current API 0.7
200 KiB stack, including the real RAR4/RAR5 fixtures. This is recorded in the
[CBR feasibility report](cbr-feasibility.md). It establishes parsing,
instantiation and extraction outside iOS; practical device memory and latency
remain unmeasured. The nested interpreter has a memory/fuel boundary; the
direct decoder has size limits but no fuel counter, and a documented RAR3
PPMd allocation gap. Neither depends on JavaScriptCore WebAssembly, memory64
or SIMD.

No CBR plugin was built or measured. Its implementation would need archive
root/path configuration, identity/profile authorization and route timing under
the host's 10-second deadline. Native subprocess execution is possible, but
route-level authentication alone does not prove access to a requested file.

The direct Silo server wasm dependency is `github.com/tetratelabs/wazero v1.12.0`, used by the ebook conversion/mobitool path. It is not the plugin host and is unrelated to Aidoku’s Wasm3 runtime.

## Pinned primary sources

- Aidoku source list: [`fadd70a85a1ab425d94181b05a02f402c54a46b8`](https://github.com/crowquillx/aidoku-silo-sources/tree/fadd70a85a1ab425d94181b05a02f402c54a46b8).
- Aidoku app: [`73c55ffa685b9edbbd5a269c9a0de18d5dc4f43b`](https://github.com/Aidoku/Aidoku/tree/73c55ffa685b9edbbd5a269c9a0de18d5dc4f43b).
- AidokuRunner: [`cc4d06ff399e7169b9c647bccede7cb29bc805c6`](https://github.com/Aidoku/AidokuRunner/tree/cc4d06ff399e7169b9c647bccede7cb29bc805c6).
- `aidoku-rs`: [`e1320b0a2e11afb59e4dee374883a2212d325699`](https://github.com/Aidoku/aidoku-rs/tree/e1320b0a2e11afb59e4dee374883a2212d325699).
- Wasm3 Swift wrapper: [`6a64d8bfc6a235f1ee3f8b57f692494fbf72ed19`](https://github.com/Skittyblock/Wasm3/tree/6a64d8bfc6a235f1ee3f8b57f692494fbf72ed19).
- Silo server: [`0416528027bee67b1fda8e78f06a3d5dfc4e4137`](https://github.com/Silo-Server/silo-server/tree/0416528027bee67b1fda8e78f06a3d5dfc4e4137).
- Silo plugin SDK: [`f110653047449de7220f8fb9f7ce49ddecc7d9a0`](https://github.com/Silo-Server/silo-plugin-sdk/tree/f110653047449de7220f8fb9f7ce49ddecc7d9a0).
- Renkei provider client: [`bc27f39fdfb59eaf9722c776f1d8138885ea62ee`](https://github.com/renkei-project/renkei-provider-silo/tree/bc27f39fdfb59eaf9722c776f1d8138885ea62ee).
- Shoko plugin inspected at local commit [`f3aee09d7593009ea6237ae76765c2f7ff4df9d3`](https://github.com/Silo-Server/silo-shoko-plugin/tree/f3aee09d7593009ea6237ae76765c2f7ff4df9d3).
