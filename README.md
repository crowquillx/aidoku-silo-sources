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
| **Auth Mode** | `Username & Password` or `API Key`. |
| **Log in** | Your Silo username and password. |
| **API Key** | An unscoped Silo API key (`sa_...`) instead of a password login. |
| **Profile** | Optional profile name or ID. Blank uses the primary profile. |
| **Profile PIN** | Required only for PIN-protected profiles. |
| **Mark chapters read on Silo** | Marks a chapter read on Silo when opened in Aidoku. |

API keys never expire and skip profile PIN prompts, so they are the most
low-maintenance way to connect a reader.

### API versions

Both Silo APIs are supported:

- **v2** (`/api/v2`) is the current, in-development contract. Use it when your
  server supports it.
- **v1** (`/api/v1`) is **deprecated**. It still works on current servers but
  will return `410 Gone` once Silo 1.0 removes the legacy API, at which point
  the v1 option stops working. The source labels it as deprecated for this
  reason.

`Auto-detect` probes `/api/v2/system/info` and falls back to v1.

## Features

- Browse one entry per manga library, plus an "All Manga" listing.
- Home page built from the server's own library sections.
- Search by title, with filters for author, genre, year, and sort order.
- Series details: cover, backdrop (as an alternate cover), overview, authors,
  genres, and publication status.
- Chapter list with volume/chapter numbers derived from the server.
- Progress sync: opening a chapter marks it read on Silo (toggleable).

## Known limitations

- **CBZ only.** Silo exposes no per-page image endpoint; it serves a whole
  chapter archive, so the source downloads the `.cbz` and extracts pages
  on-device. **`.cbr` (RAR) archives are not supported** and show a clear error
  — a pure-Rust RAR decoder that runs in Aidoku's WebAssembly sandbox is not
  practical. Convert `.cbr` files to `.cbz`, or read them in the Silo web app.
- **Manga library type only.** Silo has no separate `comic` type, so both manga
  and comics come from `manga` libraries. EPUB/PDF `ebook` libraries are
  intentionally not exposed (Aidoku renders images, not ebooks).
- **Publication-status filtering** is not available: Silo's catalog `status`
  field is an internal match state, not the ongoing/completed status.
- **Read/unread state** flows from Aidoku to Silo when opening a chapter.
  Aidoku's source API has no hook for page-level progress, so in-chapter
  position is not written back to Silo.

## Development

```sh
rustup target add wasm32-unknown-unknown
cargo install --git https://github.com/Aidoku/aidoku-rs aidoku-cli
cargo install --git https://github.com/Aidoku/aidoku-rs aidoku-test-runner

cd sources/multi.silo
cargo test          # unit tests + live tests against a Silo server
aidoku package      # produces package.aix
aidoku verify package.aix
aidoku build package.aix --name "Silo Sources"   # local source list in public/
```

The live tests point at a test Silo instance. The v2 code paths are covered by
an ignored test that runs against a mock server mirroring Silo's v2 contract:

```sh
python3 tests/mock_silo_v2.py &
cargo test -- --ignored
```

The source list is rebuilt and deployed to GitHub Pages by
[`.github/workflows/build.yaml`](.github/workflows/build.yaml) on every push to
`main`.

## License

Dual-licensed under either of [Apache-2.0](LICENSE-APACHE) or
[MIT](LICENSE-MIT), at your option.
