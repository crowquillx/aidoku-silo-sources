# Silo — Aidoku Sources

An [Aidoku](https://aidoku.app) source for reading manga and comics from a
self-hosted [Silo](https://siloserver.org/) media server.

## Add the repository

Add this URL as a source list in Aidoku (**Settings → Source Lists**):

```
https://crowquillx.github.io/aidoku-silo-sources/index.min.json
```

Or open the [repository page](https://crowquillx.github.io/aidoku-silo-sources/)
and tap **Add Repository**. Requires **Aidoku 0.7** or newer.

## Requirements

- A Silo server with at least one library of type **`manga`**. Silo scans both
  manga and western comics (`.cbz`/`.cbr`) into `manga`-type libraries, so both
  appear here. Other library types (movies, TV, ebooks, audiobooks) are ignored.
- A Silo account with access to that library.

## Configuration

Open the source settings in Aidoku:

| Setting | Description |
|---|---|
| **Server URL** | Your Silo base URL, e.g. `https://silo.example.com`. |
| **API Version** | `Auto-detect` (recommended), `v2`, or `v1 (deprecated)`. |
| **Image Quality** | Artwork size requested from the server. |
| **Use API Key** | Sign in with an API key instead of a username and password. |
| **Log in** | Your Silo username and password. |
| **API Key** | An unscoped Silo API key (`sa_...`), used when **Use API Key** is on. |
| **Profile** | Optional profile name or ID. Blank uses the primary profile. |
| **Profile PIN** | Required only for PIN-protected profiles. |
| **Mark chapters read on Silo** | Marks a chapter read on Silo when it is opened or downloaded in Aidoku. |
| **Use Comic Pages plugin** | Extracts CBR pages on your server with the [Comic Pages plugin](https://github.com/crowquillx/silo-comic-pages) when it is installed. On by default. |
| **Comic Pages plugin installation ID** | Optional. Needed only on v1 servers, where the plugin can't be detected. |

API keys never expire and skip profile PIN prompts, so they are the most
low-maintenance way to connect a reader.

The source caches its session, profile and detected API version, so most calls
make no extra authentication requests. Changing the server, account, API key,
profile or PIN discards the cached session.

### API versions

Both Silo APIs are supported:

- **v2** (`/api/v2`) is the current, in-development contract. Use it when your
  server supports it.
- **v1** (`/api/v1`) is **deprecated**. It still works on current servers but
  will return `410 Gone` once Silo 1.0 removes the legacy API, at which point
  the v1 option stops working. The source labels it as deprecated for this
  reason.

`Auto-detect` probes `/api/v2/system/info` and remembers a v2 result. It falls
back to the other version when one answers `404` or `410 Gone`.

### Server extraction for CBR

The source can use the separately installed
[Comic Pages plugin](https://github.com/crowquillx/silo-comic-pages) (v0.2.0 or
newer). Install and configure the plugin in Silo. On v2 servers the source finds
it through Silo's user plugin list, so there is nothing to enter; older plugin
versions or v1 servers need the installation ID in the source's Reading
settings. CBR chapters then use server extraction and the source downloads
individual images. CBZ chapters retain the ZIP range reader.

The source sends its current Silo token and profile in authenticated POST bodies
to the plugin on the configured Silo server. The plugin checks access before
serving cached pages. Tokens are absent from page URLs and page contexts. Large
images arrive in 1 MiB chunks, which the source joins without recompression.
The page limit is 32 MiB. If the plugin's cache expires, reopen the chapter.

Turn off **Use Comic Pages plugin** to use the built-in CBR decoder and its
limits. A failure from a detected or configured plugin is reported directly, so
a large archive does not silently fall back to downloading and decoding on the
device.

## Features

- Browse one entry per manga library, plus an "All Manga" listing.
- Home page built from the server's own library sections; each section opens
  its full item list.
- Search by title, with filters for author, genre, year, and sort order.
- Series details: cover, backdrop (as an alternate cover), overview, authors,
  genres, and publication status.
- Chapter list with volume/chapter numbers derived from the server.
- Lazy page loading for CBZ: only the ZIP central directory and the current
  page's byte range are fetched over HTTP.
- Progress sync: opening a chapter marks it read on Silo (toggleable). Aidoku
  requests page lists for downloads too, so downloading a chapter also marks it
  read.

## Known limitations

- **CBZ and CBR.** Silo exposes no per-page image endpoint and serves a whole
  chapter archive. For CBZ, the source reads the ZIP directory and fetches
  each page's byte range. The native `no_std` CBR decoder
  ([`crates/cbr-native`](crates/cbr-native/README.md)) is enabled by default
  through the `cbr-native` feature. It reads tested RAR4 and RAR5
  normal and solid archives. Without the Comic Pages plugin, opening a CBR
  chapter downloads the archive once and decodes every page before the first
  one shows; reading then makes no further requests. Downloaded CBR chapters
  are stored as PNG pages, which can be larger than the original images.
- The native CBR decoder limits the archive to 16 MiB, each unpacked page to
  16 MiB, total unpacked members to 64 MiB, entries to 512, and RAR5
  dictionaries to 8 MiB. A known RAR3 PPMd gap remains: if an LZ stream
  switches to PPMd midstream, compcol 0.6.11 can request up to 256 MiB. The
  stated limits do not cover that allocation. No Aidoku device validation has
  been completed, so this CBR support is not fully hardened.
- If a device or archive is incompatible, convert CBR to CBZ as a fallback
  (see below). The [measured CBR report](docs/research/cbr-feasibility.md) and
  the [comparison with other sources](docs/research/cbr-other-sources.md)
  describe the costs and other sources' server extraction and page APIs.
- A ZIP mislabeled with a `.cbr` extension still works: the format is detected
  from the file's magic bytes, not its extension.
- **Manga library type only.** Silo has no separate `comic` type, so both manga
  and comics come from `manga` libraries. EPUB/PDF `ebook` libraries are
  intentionally not exposed (Aidoku renders images, not ebooks).
- **Publication-status filtering** is not available: Silo's catalog `status`
  field is an internal match state, not the ongoing/completed status.
- **Read/unread state** flows from Aidoku to Silo when opening a chapter.
  Aidoku's source API has no hook for page-level progress, so in-chapter
  position is not written back to Silo.

### Converting CBR to CBZ

To convert a CBR library to CBZ as a fallback, run the included helper and then
rescan the library in Silo:

```sh
scripts/convert-cbr-to-cbz.sh --apply --delete /path/to/manga-library
```

It needs one of `unar`, `unrar`, `7z`, or `7zz` plus `zip`; without `--apply`
it only reports what it would do.

## Development

```sh
rustup target add wasm32-unknown-unknown
cargo install --git https://github.com/Aidoku/aidoku-rs aidoku-cli
cargo install --git https://github.com/Aidoku/aidoku-rs aidoku-test-runner

cd sources/multi.silo
cargo test -- test_unit_          # offline unit tests
cargo test --no-default-features -- test_unit_  # build without CBR support
aidoku package      # produces package.aix
aidoku verify package.aix
aidoku build package.aix --name "Silo Sources"   # local source list in public/
```

The v2 code paths are covered by ignored `test_mock_*` tests that run against
a mock server enforcing Silo's v2 contract (auth and profile headers, sort and
rule grammar):

```sh
python3 tests/mock_silo_v2.py &
cargo test -- --ignored test_mock_
```

The remaining `test_live_*` tests point at a real test Silo instance.

[`.github/workflows/build.yaml`](.github/workflows/build.yaml) runs formatting,
clippy, the unit tests and the mock tests on pull requests and pushes. On
`main` it also rebuilds the source list and deploys it to GitHub Pages.

## License

Dual-licensed under either of [Apache-2.0](LICENSE-APACHE) or
[MIT](LICENSE-MIT), at your option.
