# Controlled filesystem inspection

This feature owns portable path spelling, stable regular-file reads and the complete
bounded scan. Callers supply explicit byte limits and diagnostic identities; these
helpers do not expand admitted paths or exclusions.

| File | Responsibility and entrypoints |
| --- | --- |
| `mod.rs` | Canonical root, sorted directory enumeration, exact entry/file lookup and portable path predicates. |
| `read.rs` | `read_bounded_utf8` and expected-size/hook variants: bounded UTF-8 read with identity revalidation. |
| `identity.rs` | Safe open, current-path/same-handle state checks and unchanged Unix/Windows/unsupported-platform strategies. |
| `scan.rs` | `validate_bounded_filesystem`, recursive dispatch, scan policy/state, global resource and diagnostic budgets. |
| `entries.rs` | Excluded-entry shapes, regular-file inspection, sorted directory traversal and sibling spelling/collision checks. |

Dependencies: `same-file`, standard filesystem/I/O, Unix-only `libc`, the contract
models and shared diagnostics. Scan exclusions remain exactly contract-derived.
The scanner and entry inspector share one state; recursion never resets global budgets.

Run `cargo test --locked -p zryna-architecture`. `src/tests/controlled_read.rs` tests
replacement, mutation, UTF-8 and size boundaries; `src/tests/scan.rs` covers excluded
shapes, aggregate budgets, collisions and platform-specific links/reparse points.
`src/tests/paths.rs` covers portable spelling. Preserve the conditional platform cases.
