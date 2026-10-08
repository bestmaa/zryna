# Exact provider file inventories

`providers.rs` holds the fixed nine-file tagged-release inventory and the fixed
28-file main-candidate inventory. `Distribution::provider_paths()` selects them
using the existing source-ref/version check; it rejects unknown or mismatched
refs. It changes no supported protocol, source identity, version or recipe rule.

`../manifest.rs` uses the selection for required payloads and provider roles.
`../stage.rs` uses the same selection for immutable private copying and hash/state
revalidation. The corresponding JavaScript lists and selector live in
`scripts/distribution/inventory.mjs` and acquisition uses the already admitted ref.

Run `cargo test --locked -p zryna-driver --lib distribution` and
`node --test tests/distribution-inventory.test.mjs tests/release-qualification-acquisition.test.mjs`.
These fixture checks cover legacy/main selection, incomplete/extra paths and
mutation/cleanup. They do not establish release or host qualification.
