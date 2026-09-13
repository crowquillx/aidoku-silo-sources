# CBR/RAR page handling in other sources

This is a source-code comparison for Silo version 4. It treats `cbr-native` as
enabled by default, with the existing documented constraints: a 16 MiB archive
and per-member limit, a complete archive response for native RAR listing and
page extraction, RAR3 PPMd allocation that can exceed the declared limits, and
no physical-device validation. The Silo release notes record those facts and
the approximate `N+1` full downloads for an `N`-page CBR
([release notes](cbr-v4-release.md), [download path](cbr-feasibility.md#downloads-and-lazy-ranges)).

The comparison separates three locations for archive work:

- **Source decoding:** the Aidoku source requests archive bytes and its WASM code
  extracts the page image bytes for the app to display.
- **Server extraction:** the server opens its local archive and exposes a page
  image endpoint; the Aidoku source only returns URLs.
- **Native app local support:** the app opens a file already on the device.

The Komga, Kavita and Suwayomi integrations below are Swift sources built
into Aidoku. LANraragi is a Rust WASM source in the community repository.
Their inspected page paths all request images from the server.

## Comparison

| Implementation | Page/API path | Where archive work runs | Auth, range, and cache observations relevant to Silo |
|---|---|---|---|
| **Komga server + official Aidoku source** | Aidoku first calls `GET /api/v1/books/{bookId}/pages`, then returns `GET /api/v1/books/{bookId}/pages/{pageNumber}`; unsupported media can add `?convert=png` ([Aidoku source](https://github.com/Aidoku/Aidoku/blob/73c55ffa685b9edbbd5a269c9a0de18d5dc4f43b/Aidoku/Core/Sources/BuiltIn/Komga/KomgaSource.swift#L192-L220)). | **Komga server.** Its `RarExtractor` opens a local `Path` with Junrar, rejects encrypted and multivolume RAR, and reads the selected entry into a byte array ([extractor](https://github.com/gotson/komga/blob/9707edaadfb472968d5333b735557908aa649544/komga/src/main/kotlin/org/gotson/komga/infrastructure/mediacontainer/divina/RarExtractor.kt#L24-L64)); Junrar and Commons Compress are server dependencies ([build](https://github.com/gotson/komga/blob/9707edaadfb472968d5333b735557908aa649544/komga/build.gradle.kts#L96-L100)). | The Aidoku source uses HTTP Basic auth ([helper](https://github.com/Aidoku/Aidoku/blob/73c55ffa685b9edbbd5a269c9a0de18d5dc4f43b/Aidoku/Core/Sources/BuiltIn/Komga/KomgaHelper.swift#L14-L21)). The image controller requires `PAGE_STREAMING`; the common controller handles conditional requests with `If-None-Match`/ETag before returning page bytes ([controller](https://github.com/gotson/komga/blob/9707edaadfb472968d5333b735557908aa649544/komga/src/main/kotlin/org/gotson/komga/interfaces/api/rest/BookController.kt#L476-L502), [conditional response](https://github.com/gotson/komga/blob/9707edaadfb472968d5333b735557908aa649544/komga/src/main/kotlin/org/gotson/komga/interfaces/api/CommonBookController.kt#L138-L182)). The inspected path does not establish a range contract. |
| **Kavita server + official Aidoku source** | Aidoku gets chapter metadata from `GET /api/Series/chapter?chapterId=...`, then creates `GET /api/Reader/image?chapterId=...&page=...&apiKey=...&extractPdf=true` for each page ([Aidoku source](https://github.com/Aidoku/Aidoku/blob/73c55ffa685b9edbbd5a269c9a0de18d5dc4f43b/Aidoku/Core/Sources/BuiltIn/Kavita/KavitaSource.swift#L212-L230)). | **Kavita server.** `.cbr`/`.rar` is routed to SharpCompress; ZIP/CBZ takes the .NET ZIP path first, with SharpCompress as fallback ([archive selection](https://github.com/Kareadita/Kavita/blob/d77d956b9551227d8be2ee488b08f14aa3a341e5/Kavita.Services/ArchiveService.cs#L42-L66), [dependency](https://github.com/Kareadita/Kavita/blob/d77d956b9551227d8be2ee488b08f14aa3a341e5/Kavita.Services/Kavita.Services.csproj#L54-L54)). The server extracts image entries to a chapter cache, guarded by a per-chapter semaphore ([extraction](https://github.com/Kareadita/Kavita/blob/d77d956b9551227d8be2ee488b08f14aa3a341e5/Kavita.Services/ArchiveService.cs#L519-L549), [cache fill](https://github.com/Kareadita/Kavita/blob/d77d956b9551227d8be2ee488b08f14aa3a341e5/Kavita.Services/CacheService.cs#L140-L181)). | The page endpoint requires chapter access and returns the cached file. The Aidoku URL embeds the API key; the helper can also send a bearer token or cookie ([auth helper](https://github.com/Aidoku/Aidoku/blob/73c55ffa685b9edbbd5a269c9a0de18d5dc4f43b/Aidoku/Core/Sources/BuiltIn/Kavita/KavitaHelper.swift#L14-L28)). Kavita emits private one-hour cache headers, ETags, and `enableRangeProcessing` for the physical page file ([endpoint](https://github.com/Kareadita/Kavita/blob/d77d956b9551227d8be2ee488b08f14aa3a341e5/Kavita.Server/Controllers/ReaderController.cs#L79-L102), [file response](https://github.com/Kareadita/Kavita/blob/d77d956b9551227d8be2ee488b08f14aa3a341e5/Kavita.Server/Controllers/BaseApiController.cs#L70-L83)). |
| **LANraragi server + community Aidoku source** | The source calls `GET /api/archives/{id}/files`; returned page paths are relative URLs for `GET /api/archives/{id}/page?path=...` ([source](https://github.com/Aidoku-Community/sources/blob/cc3360eaff6a762b5d1d5dd9ee7b9ea94c42bb1f/sources/multi.lanraragi/src/lib.rs#L222-L248), [API schema](https://github.com/Difegue/LANraragi/blob/db3106900d90e07f5723da9ed933cc0399a5670d/tools/openapi.yaml#L2189-L2248), [page schema](https://github.com/Difegue/LANraragi/blob/db3106900d90e07f5723da9ed933cc0399a5670d/tools/openapi.yaml#L2320-L2349)). | **LANraragi server.** File listing uses libarchive with all filters and formats enabled, filters image entries, and natural-sorts them ([listing](https://github.com/Difegue/LANraragi/blob/db3106900d90e07f5723da9ed933cc0399a5670d/lib/LANraragi/Utils/Archive.pm#L350-L405)). Page extraction uses `Archive::Libarchive::Peek` on demand ([single-file extraction](https://github.com/Difegue/LANraragi/blob/db3106900d90e07f5723da9ed933cc0399a5670d/lib/LANraragi/Utils/Archive.pm#L552-L564)). | Reader JSON constructs the page URLs after listing the local archive ([reader JSON](https://github.com/Difegue/LANraragi/blob/db3106900d90e07f5723da9ed933cc0399a5670d/lib/LANraragi/Model/Reader.pm#L43-L82)). Extracted page bytes are cached under `page/{id}/{path}` in the server page cache ([page cache](https://github.com/Difegue/LANraragi/blob/db3106900d90e07f5723da9ed933cc0399a5670d/lib/LANraragi/Model/Archive.pm#L239-L301)). The source sends a base64-encoded API key as `Authorization: Bearer ...`; the deployment’s OpenAPI routing also accepts session/pass-disabled modes ([source auth](https://github.com/Aidoku-Community/sources/blob/cc3360eaff6a762b5d1d5dd9ee7b9ea94c42bb1f/sources/multi.lanraragi/src/lib.rs#L83-L93), [routing auth](https://github.com/Difegue/LANraragi/blob/db3106900d90e07f5723da9ed933cc0399a5670d/lib/LANraragi/Utils/Login.pm#L10-L29)). The inspected API does not establish range processing. |
| **Suwayomi server + official Aidoku source** | Aidoku posts a GraphQL mutation to `/api/graphql`; `fetchChapterPages` returns URLs of the form `/api/v1/manga/{mangaId}/chapter/{chapterIndex}/page/{index}` ([Aidoku request](https://github.com/Aidoku/Aidoku/blob/73c55ffa685b9edbbd5a269c9a0de18d5dc4f43b/Aidoku/Core/Sources/BuiltIn/Suwayomi/SuwayomiSource.swift#L216-L241), [server payload](https://github.com/Suwayomi/Suwayomi-Server/blob/d10e000e1fdcac6f3c84d002f0b459c90c1b00f3/server/src/main/kotlin/suwayomi/tachidesk/graphql/mutations/ChapterMutation.kt#L405-L465)). | **Server page route for remote sources.** The page handler requires an authenticated user and returns image bytes from `Page.getPageImageServe`; for an HTTP source, that loads the source’s image URL and stores the response in a chapter cache ([route](https://github.com/Suwayomi/Suwayomi-Server/blob/d10e000e1fdcac6f3c84d002f0b459c90c1b00f3/server/src/main/kotlin/suwayomi/tachidesk/manga/controller/MangaController.kt#L465-L501), [remote-page cache](https://github.com/Suwayomi/Suwayomi-Server/blob/d10e000e1fdcac6f3c84d002f0b459c90c1b00f3/server/src/main/kotlin/suwayomi/tachidesk/manga/impl/Page.kt#L108-L140)). This remote path does not put a RAR decoder in Aidoku. | The server page URL carries `Cache-Control: max-age=1 day`; the Aidoku helper supports Basic auth, cookie, token, and simple login ([server headers](https://github.com/Suwayomi/Suwayomi-Server/blob/d10e000e1fdcac6f3c84d002f0b459c90c1b00f3/server/src/main/kotlin/suwayomi/tachidesk/manga/controller/MangaController.kt#L489-L501), [Aidoku auth](https://github.com/Aidoku/Aidoku/blob/73c55ffa685b9edbbd5a269c9a0de18d5dc4f43b/Aidoku/Core/Sources/BuiltIn/Suwayomi/SuwayomiHelper.swift#L14-L23)). |
| **Suwayomi/Tachidesk local source** | Local server readers use `GET /api/v1/manga/{mangaId}/chapter/{chapterIndex}/page/{index}` ([route registration](https://github.com/Suwayomi/Suwayomi-Server/blob/d10e000e1fdcac6f3c84d002f0b459c90c1b00f3/server/src/main/kotlin/suwayomi/tachidesk/manga/MangaAPI.kt#L58-L82)). | **Server process, local-file path.** The local source recognizes `zip`, `cbz`, `rar`, and `cbr`, uses Commons Compress through `ZipPageLoader`, and uses Junrar through `RarPageLoader`; each RAR page is extracted with `rar.extractFile` ([allowlist and imports](https://github.com/Suwayomi/Suwayomi-Server/blob/d10e000e1fdcac6f3c84d002f0b459c90c1b00f3/server/src/main/kotlin/eu/kanade/tachiyomi/source/local/io/Archive.kt#L1-L11), [page list](https://github.com/Suwayomi/Suwayomi-Server/blob/d10e000e1fdcac6f3c84d002f0b459c90c1b00f3/server/src/main/kotlin/eu/kanade/tachiyomi/source/local/LocalSource.kt#L344-L374), [RAR loader](https://github.com/Suwayomi/Suwayomi-Server/blob/d10e000e1fdcac6f3c84d002f0b459c90c1b00f3/server/src/main/kotlin/eu/kanade/tachiyomi/source/local/loader/RarPageLoader.kt#L1-L53)). The server dependency list explicitly includes Commons Compress and Junrar ([build](https://github.com/Suwayomi/Suwayomi-Server/blob/d10e000e1fdcac6f3c84d002f0b459c90c1b00f3/server/build.gradle.kts#L81-L85)). | Local archive page streams are placed in `LocalSource.pageCache`; the common page path returns those streams. This is a direct server-side decoder reference, but it assumes a local file and does not solve Silo’s remote-range protocol. |
| **Aidoku app local file source (official app baseline)** | Local chapters become `PageContent.zipFile(url, filePath)`, which is converted to the legacy page model as an archive URL plus member path ([page bridge](https://github.com/Aidoku/Aidoku/blob/73c55ffa685b9edbbd5a269c9a0de18d5dc4f43b/Aidoku/Extensions/AidokuRunner/AidokuRunner.swift#L407-L416)). | **Native app, local ZIP only in the scanned path.** `LocalFileManager` imports ZIPFoundation, allows only `cbz`/`zip`, and enumerates the local archive ([local manager](https://github.com/Aidoku/Aidoku/blob/73c55ffa685b9edbbd5a269c9a0de18d5dc4f43b/Aidoku/Core/Sources/BuiltIn/Local/LocalFileManager.swift#L8-L23), [ZIP read](https://github.com/Aidoku/Aidoku/blob/73c55ffa685b9edbbd5a269c9a0de18d5dc4f43b/Aidoku/Core/Sources/BuiltIn/Local/LocalFileManager.swift#L133-L169)). | This is a native local archive API, not a remote CBR implementation. Within this checkout, the RAR hits found by the scoped search were filename-parser fixtures; the local-file decoder path shown here is ZIPFoundation. |

## What this suggests for Silo

1. **Keep the two Silo paths explicit.** The existing CBZ reader already reads the ZIP tail and central directory
   and fetches the compressed member with byte ranges
   ([page listing](../../sources/multi.silo/src/lib.rs#L189-L224), [directory ranges](../../sources/multi.silo/src/lib.rs#L467-L490), [page ranges](../../sources/multi.silo/src/lib.rs#L435-L450)). The
   server-backed implementations above all make the page image the contract;
   they do not require a client to understand RAR.

2. **Native CBR still downloads the whole archive.** Silo’s v4 code first downloads the complete RAR to enumerate
   members, then publishes the archive read URL with page context
   ([listing](../../sources/multi.silo/src/lib.rs#L124-L171)). For each native page,
   the image request asks for `bytes=0-total-1`, while the decoder still requires
   the returned body length to equal the complete archive
   ([request](../../sources/multi.silo/src/lib.rs#L420-L448), [decoder](../../sources/multi.silo/src/rar_native.rs#L43-L67)). Keep that full-download cost prominent in the v4 documentation.

3. **Make the limits operationally clear.** State that the 16 MiB archive and
   member limits are hard gates, that a `206` response is still expected to
   contain the entire archive for native CBR, and that PPMd can exceed the
   declared memory limits when a RAR3 LZ stream switches compression
   modes. The release has no device validation and no native fuel or wall-clock
   boundary, so failure behavior and the existing conversion path matter more
   than claiming broad RAR compatibility.

4. **Use server page URLs whenever the server provides them.** For Komga,
   Kavita, LANraragi, and Suwayomi, the Silo analogue would be a page-list
   capability plus authenticated image URLs. The observed cache patterns are
   useful targets: conditional ETag responses (Komga), a serialized extraction
   cache with range-capable files (Kavita), per-page server cache keys
   (LANraragi), and a one-day page response cache (Suwayomi). Silo currently lacks that endpoint. A local Silo plugin or conversion cache
   would be the closest equivalent, subject to the file-access and profile
   authorization constraints in the [plugin investigation](cbr-runtime-routes.md).
   This can be developed in our repository without an upstream PR.

5. **Preserve authentication on every derived page request.** Komga uses
   Basic auth, Suwayomi uses several session/token modes, and LANraragi’s source
   sends its encoded API key as a Bearer header. Kavita’s built-in source puts
   the API key in the page URL while its helper also supports bearer/cookie
   authorization. Silo’s native page request already carries its stored bearer
   token and profile headers ([Silo auth](../../sources/multi.silo/src/lib.rs#L521-L538)); this must remain true when a page URL is produced from archive metadata.

## Search scope and pinned revisions

The read-only corpus was eight shallow clones under
`/tmp/aidoku-cbr-research-20260913`, inspected at these checkout heads on
2026-09-13. A head is pinned here rather than described as a release tag:

| Repository | Revision |
|---|---|
| [Aidoku/Aidoku](https://github.com/Aidoku/Aidoku/tree/73c55ffa685b9edbbd5a269c9a0de18d5dc4f43b) | `73c55ffa685b9edbbd5a269c9a0de18d5dc4f43b` |
| [Aidoku-Community/sources](https://github.com/Aidoku-Community/sources/tree/cc3360eaff6a762b5d1d5dd9ee7b9ea94c42bb1f) | `cc3360eaff6a762b5d1d5dd9ee7b9ea94c42bb1f` |
| [gotson/komga](https://github.com/gotson/komga/tree/9707edaadfb472968d5333b735557908aa649544) | `9707edaadfb472968d5333b735557908aa649544` |
| [Kareadita/Kavita](https://github.com/Kareadita/Kavita/tree/d77d956b9551227d8be2ee488b08f14aa3a341e5) | `d77d956b9551227d8be2ee488b08f14aa3a341e5` |
| [Difegue/LANraragi](https://github.com/Difegue/LANraragi/tree/db3106900d90e07f5723da9ed933cc0399a5670d) | `db3106900d90e07f5723da9ed933cc0399a5670d` |
| [Suwayomi/Suwayomi-Server](https://github.com/Suwayomi/Suwayomi-Server/tree/d10e000e1fdcac6f3c84d002f0b459c90c1b00f3) | `d10e000e1fdcac6f3c84d002f0b459c90c1b00f3` |
| [Suwayomi/Tachidesk-Sorayomi](https://github.com/Suwayomi/Tachidesk-Sorayomi/tree/df37f4ce700a7db7c971301d0adc65b4f0721a6f) | `df37f4ce700a7db7c971301d0adc65b4f0721a6f` |
| [mihonapp/mihon](https://github.com/mihonapp/mihon/tree/2d1a8bbeff996ef6615c904e019ca9794e8d2096) | `2d1a8bbeff996ef6615c904e019ca9794e8d2096` |

The search was scoped to those checkouts and the Silo working tree. The
commands used were:

```sh
rg --files <checkout> | rg -i '(rar|cbr|cbz|zip|archive|page|reader)'
rg -n -i '(junrar|unrar|libarchive|sharpcompress|commons-compress|zip4j|miniz|archive|rar[45]|ppmd|content-range|accept-ranges|/api/.*/page|/pages/|page_url|pageUrl|download)' <checkout>
rg -n -i '(rar|cbr|cbz|zip|archive|ppmd|native.?cbr|16.?mib|full.?archive|device)' sources/multi.silo docs/research
```

A follow-up dependency check found 135 `Cargo.toml` files under the community
`sources/` directory and no matches for `unrar`, `rars`, or `compcol` in those
manifests:

```sh
rg -n -i '(unrar|rars|compcol)' \
  /tmp/aidoku-cbr-research-20260913/aidoku-community-sources/sources \
  --glob Cargo.toml
```

This rules out those named decoder dependencies in that snapshot, not custom
archive implementations or dependencies in other source repositories.

For the broad source search, generated `build`, `target`, `node_modules`, and
lock-file noise was excluded where present. This is a corpus statement, not a
global claim about all archive implementations. Sorayomi was scanned because
it is the adjacent Tachidesk client, but the server’s page route and local
source contain the relevant archive behavior. Mihon was pinned and scanned as
an adjacent reader reference; it was not used to add another main comparison
row.

## Uncertainties

The server projects delegate format coverage to packaged libraries and native
builds; these source inspections do not prove support for every RAR4/RAR5
feature, solid layout, encryption mode, filename encoding, or PPMd stream. The
Komga controller inspection establishes ETag handling but not a range response
contract. LANraragi’s OpenAPI operations mark the files/page operations with
`security: []`, while the global OpenAPI routing callback can still apply its
configured API/session checks; deployment configuration decides the effective
policy. No upstream repository, issue, PR, or the Silo server working tree was
modified, and no build or device validation was performed for this report.
