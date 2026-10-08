# Data and ownership JavaScript emission

[`emit_data_ownership`](mod.rs), re-exported by the backend, accepts sealed M3 IR and the matching
`VerifiedOwnershipRuntimeAbi`. It checks the type universe, both layout fingerprints and helper
mapping, then emits deterministic bounded ESM as `JavaScriptArtifact`, or a diagnostic. It counts
the complete output before reserving and rendering its source.

## Private areas and boundaries

| Area | Responsibility |
| --- | --- |
| [mod.rs](mod.rs) | Authority checks, byte bound, deterministic rendering and export wrappers |
| [instructions.rs](instructions.rs) | Verified functions/instructions and scalar boundary emission |
| [runtime.rs](runtime.rs) | Explicit ownership runtime helpers embedded in generated source |
| [observation.js](observation.js) | Deterministic ownership observation support |

The backend consumes verified IR/layout/runtime ABI and never calls a frontend, semantics, driver
or another backend. It does not resolve packages, read ambient source, publish files, select a
Node/browser host or implement host capability approval. Explicit generated ownership behavior
must not depend on JavaScript engine GC timing. Public owned-value ABI support is not implied;
see [M3 public profile](../../../../docs/M3_PUBLIC_PROFILE.md).

## Existing focused tests

The existing executable M3 evidence is in the driver's
[ownership conformance suite](../../../zryna-driver/src/ownership_commands/conformance.rs),
rather than a local test module in this directory:

```sh
cargo test --locked -p zryna-driver ownership_commands::conformance
```

This suite composes JavaScript, WebAssembly and native targets with fixed observations and faults;
it needs the repository-pinned runtime/toolchain described in the root README. The complete
registered cross-target gate is `pnpm m3:check`. Do not substitute the backend's M1/M2 unit tests
for M3 execution evidence.
