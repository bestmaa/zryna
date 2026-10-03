# Internal native-recipe prerequisite checks

This repository-only #405 slice consumes the existing #361 canonical plan and
its typed `zryna.native-compile-adapter.v0` compilation step. It introduces no
package manifest, #168 provenance field, serialized recipe schema, CLI selector,
executor, cache admission or publication path. The current driver continues to
reject package recipes. PR #506's source-only trust regressions remain separate.

`describeNativeRecipe` independently validates the complete plan against the
branded #168 package authority, takes an immutable private snapshot, and derives
a domain-separated review digest. The digest binds the whole plan key, exact
step, executable and declared tool inventory, target/ABI/runtime, caller policy,
environment, typed input allowlist, object identity and final output allowlist.
It is an internal review identity, not an execution capability. Cloned or forged
reviews cannot enter the later checks. No host executable path or raw arguments
are accepted; the existing closed invocation adapter remains authoritative.

`preflightNativeRecipe` checks the caller mode first, preserves earlier #168/#361
validation failures, verifies the complete source material closure, denies
pure-source and playground recipes, checks exact declared tool/input byte maps,
and requires the independent caller's expected recipe digest. The digest must
come from operator policy, never a package or colocated checksum. Even an exact
match then rejects as `TRUST-NATIVE`: the native appendix still has
`provisional-pending-364` status, and #417 plus actual host enforcement have no
authenticated integration capability here. Caller-supplied approval, sandbox
and FFI booleans cannot unlock execution. Policy reason identifiers are internal
contract categories, not new public compiler diagnostic codes.

`preflightNativeProvenance` compares the exact recipe/plan/mode and target-qualified
output inventory against bytes, then rejects as `TRUST-PROVENANCE`. Matching
checksums do not prove actual input use, an authenticated builder, OS isolation,
object/ELF audits or publication. Its comparison arguments are internal values,
not a #168 wire extension. Observed policy is deliberately unavailable: an
environment clear, timeout or process group is not a sandbox.

The checks perform no acquisition, process start, filesystem write, cache reuse
or publication. Material-byte comparison alone does not prove retained executable
handles, source containment, sysroot materialization or hostile OS execution.
Dedicated tests use synthetic bytes and independently constructed malformed
records. They cover approval transplant, executable substitution, exact allowlists,
unknown modes, undeclared inputs, duplicate reads, raw argument injection, forged
provenance and real prior-state preservation in fresh roots. They do not execute
C/FFI fixtures or prove filesystem/network/process/timeout isolation.

Run with pinned Node 22.22.1 after the required frozen dependency installation:

```sh
node --test tests/native-recipe-identity.test.mjs tests/native-recipe-materials.test.mjs tests/native-recipe-provenance.test.mjs
node scripts/run-native-recipe-tests.mjs
pnpm package:contract
pnpm build-plan:contract
pnpm structure:check
pnpm preflight
pnpm m0:check
```

The guarded runner selects these three files and requires all 17 exact test names
to pass once with nonzero TAP totals and no failures, cancellations, skips or todo
cases. Both existing Linux and Windows Rust CI authorities run it unconditionally;
their results remain required by the M0 aggregate. Portable preflight registers
the selection and workflow mutation guards. This does not admit recipe execution.

Integration stays with the #417 and driver owners. Before enabling execution,
review the native appendix version/status, a retained exact host executable and
typed adapter capability, relevant ABI/foreign-resource conformance, complete
process-tree OS enforcement and resource limits, audited object/output inventory,
cleanup, cache/publication admission, and a separately reviewed #168 evidence
version that authenticates actual inputs and observed policy. Required independent
hostile execution tests and exact Linux/Windows gates remain acceptance obligations.
No security-setting change or public activation is authorized by this slice.
