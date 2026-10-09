# Protocol v4 boundary regression tests

Every Rust file here is reachable only through the facade's `#[cfg(test)] mod tests`.
`mod.rs` contains the original source/raw fixtures shared by the test groups. Production
verification remains outside this directory. No original assertion or test input is removed.

| Test file | Coverage / exact Cargo filter |
| --- | --- |
| `wire.rs` | Round-trip binding, adapter shorthand, closed/bounded/duplicate JSON: `v4::tests::wire` |
| `source.rs` | Version/source claims, fabricated children, import quoting, identifiers: `v4::tests::source` |
| `resources.rs` | Every raw collection's exact/first-extra limit: `v4::tests::resources` |
| `arenas.rs` | Top-level interleaving/names, type graph/depth, places, array/integer syntax: `v4::tests::arenas` |
| `diagnostics.rs` | Sensitive names and deterministic bounded diagnostics: `v4::tests::diagnostics` |

Run `cargo test --locked -p zryna-syntax v4::` to execute all 14 original v4 tests.
The adapter shorthand fixture remains under repository `tests/m3-fixtures/`; only relative
`include_str!`/`include_bytes!` paths change with relocation. No new fixture dependency is added.
The central repository structure policy owns exact test-file classifications; this extraction
keeps every Rust file, including tests, at or below 300 physical lines without editing that policy.
