# Private tooling stage

`../stage.rs` retains the existing create-only no-follow file writes, owner-private
stage, descriptor identities, state and digest revalidation, and replacement-safe
cleanup. `v4.rs:stage()` carries the exact captured v4 modules into three retained
directories using those same operations. Cleanup removes fixed module keys before
their directories; unknown entries and replacements retain the existing refusal.
The explicit captured layout is retained in the stage: legacy has exactly nine
files and no `v4` directory; modular has exactly 28 files. Layout validation occurs
before stage creation. Empty or partial modular captures cannot select legacy.

`inventory.rs` supplies exact directory contents and fixed file key/name mappings.
Its v4 names come from `../capture/v4.rs:V4_MODULES`; unknown paths do not acquire a
provider role. Root inventories and cleanup keys use the retained layout, never
filesystem presence. The entrypoints and working-directory selectors are unchanged.

Run `cargo test --locked -p zryna-driver --lib tooling_execution` to verify capture,
module tampering, staged execution, directory inventory and cleanup. Distribution
provider staging is tested separately with `--lib distribution::stage::tests`.
These unit fixtures do not establish restricted-host or release qualification.
