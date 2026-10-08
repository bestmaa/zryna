# Deterministic validation diagnostics

`ValidationDiagnostics` owns the validation-wide budget, reserved terminal error and
halt state. `architecture_error` preserves the existing error construction and portable
path rendering. `validation_report` sorts by code, path and message before constructing
the existing public `ValidationReport`; `is_valid` retains its empty-diagnostics rule.

Dependencies: `zryna-diagnostics`, Serde serialization and standard paths. The scanner
uses this same accumulator in addition to its own scan budget. Incomplete inspection
never becomes a successful report.

Run `cargo test --locked -p zryna-architecture`. The deterministic validation-wide and
reserved-terminal budget tests are in `src/tests/scan.rs`; the full-checkout test in
`src/tests/paths.rs` consumes the public report.
