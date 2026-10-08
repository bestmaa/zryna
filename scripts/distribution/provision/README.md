# Provision recipe and evidence

`recipe.mjs` owns the provisioner's exact compile-recipe checks and token
substitution. `recipeTarget(recipe, target)` checks the admitted recipe identity,
fixed Cargo command, target fields, environment, distribution marker and linker
flags before returning that target's declarations. `replaceTokens(value,
replacements)` applies the authenticated replacements and rejects any unresolved
token. Both retain the provisioner's `D422-PROVISION` error contract.

`evidence.mjs` owns architecture-receipt binding and toolchain evidence attribution.
`productionArchitecture(bytes, source)` validates the captured receipt and its
repository/commit/tree, then creates the provisional qualification representation.
`toolchains(observed, architectureBytes)` attributes each observed tool's evidence
to the architecture receipt, retaining Node's own observation evidence.

These modules depend on the existing production-recipe and source-build-receipt
validators and canonical serialization/hash helpers. They have no filesystem,
process, material-capture or pending-state authority. The entrypoint remains
`../provision-release.mjs`; it owns tool identity, material capture, compile state
and subprocess execution.

Run `node --test tests/distribution-provision-release.test.mjs` from the repository
root for injected provisioner behavior, then `pnpm release:contract` for the
existing release-boundary contracts. These tests use fixtures and injected tools;
they do not provision or publish a release.
