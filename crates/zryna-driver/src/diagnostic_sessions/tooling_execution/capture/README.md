# Authenticated worker capture

`../capture.rs` owns bounded descriptor-based capture, source identity checks, and
the pinned TypeScript dependency graph. `v4.rs` enumerates the exact 19 extracted
adapter modules and compares captured bytes with this build's `include_bytes!`
sources. It uses the existing no-follow capture path for checkout and installed
roots and retains the worker's aggregate 128 KiB source bound across the split.
The outer checkout closure still enforces its existing 16 MiB aggregate bound.

Installed capture also preserves the exact original v0.2.3 worker SHA-256 pin
(`80ceea20ee7953a1eb8f85a4724075a6ec801983ceb16ee83752313bb3740a8a`).
`CapturedV4::Legacy` carries that nine-file form; `Modular` carries the current
entrypoint and all 19 pinned modules. Legacy plus any `v4` entry is rejected;
missing or changed modular files never fall back. Checkout capture is modular
only. Both installed forms retain the existing independently verified trusted-root
precondition, unchanged limits, dependency pins and no-follow file capture.

`V4_MODULES` also supplies fixed file names to private staging and its exact
inventory. It provides no filesystem or execution authority itself.
`pins.rs` retains dependency manifest, runtime digest, and graph checks.

Run `cargo test --locked -p zryna-driver --lib tooling_execution` for source and
installed missing/mutated-module rejection, immutable staged execution,
replacement refusal, inventory checks, and cleanup. These are ordinary unit
fixtures, not restricted-host qualification or release acceptance.
The historical fixture reads the exact repository source with `git show v0.2.3`;
focused tests require that tag, as provided by the full CI checkout.
