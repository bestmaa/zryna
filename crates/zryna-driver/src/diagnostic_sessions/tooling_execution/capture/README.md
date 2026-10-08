# Authenticated worker capture

`../capture.rs` owns bounded descriptor-based capture, source identity checks, and
the pinned TypeScript dependency graph. `v4.rs` enumerates the exact 19 extracted
adapter modules and compares captured bytes with this build's `include_bytes!`
sources. It uses the existing no-follow capture path for checkout and installed
roots and retains the worker's aggregate 128 KiB source bound across the split.
The outer checkout closure still enforces its existing 16 MiB aggregate bound.

`V4_MODULES` also supplies fixed file names to private staging and its exact
inventory. It provides no filesystem or execution authority itself.
`pins.rs` retains dependency manifest, runtime digest, and graph checks.

Run `cargo test --locked -p zryna-driver --lib tooling_execution` for source and
installed missing/mutated-module rejection, immutable staged execution,
replacement refusal, inventory checks, and cleanup. These are ordinary unit
fixtures, not restricted-host qualification or release acceptance.
