# Data and ownership native object emission

The public entry [`data_ownership_v1::emit_object`](mod.rs) accepts only verifier-sealed
`VerifiedMirModule` and `LinuxX8664ObjectTarget`. It preflights bounded codegen, lowers through
Cranelift, then independently audits the ELF relocatable bytes before returning
`ValidatedDataOwnershipObjectArtifact`. Native MIR verification remains a separate lower authority.

## Private areas and boundaries

| Area | Responsibility |
| --- | --- |
| [mod.rs](mod.rs) | Fixed target, declarations, codegen budget and closed object audit |
| [lower.rs](lower.rs), [control.rs](control.rs) | Verified MIR operations and CFG lowering |
| [state.rs](state.rs), [storage.rs](storage.rs) | Native value/place storage |
| [runtime.rs](runtime.rs) | Calls to the admitted ownership runtime symbol inventory |
| [clone.rs](clone.rs), [clone_cleanup.rs](clone_cleanup.rs), [drop.rs](drop.rs), [failure.rs](failure.rs) | Clone/drop and site-bound failure cleanup |

The backend consumes native MIR/ABI authority; it must not call frontend providers, semantic
lowering, the driver or another backend. Source admission, runtime library production, invocation
harnesses, GNU linking, execution and file publication are driver/runtime responsibilities. Object
emission does not imply Windows native support, arbitrary FFI or a public aggregate ABI. See
[M3 public profile](../../../../docs/M3_PUBLIC_PROFILE.md).

## Existing focused tests

The local `resource_tests` module in [mod.rs](mod.rs) checks the exact/first-extra codegen
budget. Executable evidence belongs to the driver's
[ownership conformance suite](../../../zryna-driver/src/ownership_commands/conformance.rs):

```sh
cargo test --locked -p zryna-backend-native data_ownership_v1::resource_tests
cargo test --locked -p zryna-driver ownership_commands::conformance
```

That suite covers emitted native objects through typed link/run and faults with pinned Linux
tools; unsupported hosts retain their rejection path. `pnpm m3:check` is the complete registered
cross-target gate. The backend's [M1 object tests](../lib_tests.rs) are separate evidence.
