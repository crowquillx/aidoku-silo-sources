# RAR5 redirection regression

`rar5-redirection.hex` is an original synthetic archive encoded as hex, under
this repository's MIT OR Apache-2.0 license. It contains a valid-CRC RAR5 main
header, a redirection named `link` pointing to `page.png`, a stored `page.png`
member containing the five ASCII bytes `image`, and an end header.

It exercises a mismatch in rars 0.9.4: member metadata includes a redirection
while extraction skips it. The guest therefore rejects redirections before
either listing or extracting. The fixture is a container regression input,
not a valid PNG comic page. Its host test first parses it and confirms that
ordinary extraction succeeds, then asserts the guest's explicit rejection.

From `experiments/cbr-wasm/guest`:

```sh
cargo test --target x86_64-unknown-linux-gnu
```

Use the appropriate host target on another platform. The guest's direct wasm
unit test binary retains wasm-bindgen descriptors and cannot be run by the
Aidoku test runner; the outer experiment tests the prepared, import-free guest.
