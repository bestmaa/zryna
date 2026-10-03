# Native provider activation preparation (#414)

This private harness connects the native lexer/parser to the existing exact v2/v3/v4 worker
handshake. Its identity is `zryna-native-activation-harness` at the workspace version, deliberately
separate from any future supported native provider identity. No public selector, fallback,
provider default, CLI route, source serializer or frozen corpus changes.

The retained-source lane is stacked on draft #512 at
`69af4dd853ac99b5ba99f45712e5406d04077a45`. It consumes only the exported #413
capture/verify/revalidate API. Review and integration of #413 remain prerequisites; this harness
neither duplicates its resolver nor certifies its Windows or complete resource proof.

## Run from a clean checkout

Use Python 3.11 or later and repository-pinned Cargo/Rust 1.97.1. First run
`cargo fetch --locked` in the repository. Harness compilation and linting use only offline,
locked registry material with name/version/source/checksum identities from that lock. A temporary
standalone Cargo package references existing components; it adds no repository workspace member,
manifest entry, dependency edge or third-party dependency. The generated harness lock is retained
in the evidence directory. All Rust harness modules are formatted and checked by strict Clippy.

```text
python tests/native-provider-activation/runner_test.py
python scripts/run-native-provider-activation.py --evidence-dir <external-new-directory>
python scripts/run-native-provider-activation.py --retained --evidence-dir <external-new-directory>
```

`--target-dir <external-private-directory>` optionally retains a build cache for repeated local
verification. Evidence/cache directories must be outside the controlled repository. Every run
requires a clean Git checkout, pins the #413 ancestor for the retained lane, records the exact
repository/lock/binary identities, checks that the revision stays unchanged, and preserves logs
and a pass/failure receipt. Temporary build/install directories are removed only after successful execution. Failure
directories and evidence are retained for inspection; timeout cleanup terminates the POSIX process
group or requests Windows tree termination, and unconfirmed cleanup is an explicit failure. Never use another revision's binary receipt as current proof.

## Executed obligations

The frontend-only lane requires exactly 36 cases with zero failed/ignored cases. For each protocol
it admits genuine native syntax with original UTF-8/CRLF source identity, then independently rejects
wrong provider identity/version/protocol, widened module or semantic capabilities, extra handshake
fields and wrong response IDs. V3/V4 additionally reject false control-flow capabilities; V4
rejects a false ownership syntax capability. Analysis markers require successful admission to send real analysis and reject observed
analysis after a failed handshake. Marker absence has a scheduling limit; the unchanged worker
ordering checks the complete handshake before writing the analysis request. Following an admitted handshake, wrong snapshot schema/path and an
extra response frame reject at the existing worker/verifier boundary.

The retained lane requires those same 36 plus four cases. M1, imported M2 and Copy-aggregate M3
snapshots are captured before parsing, authenticated through #413, and kept alive through semantic
and IR verification and JavaScript/WebAssembly emission. Native process-worker results and retained
in-process results use the same source map and produce equal artifact bytes. M2/V4 graph identities
match their preparse seals. Persistent source mutation must either be denied by retained handles
or rejected before dispatch; no mutated authority is accepted.

The compiled executable is copied alone into a fresh private installation, hash-compared to the
built executable, and launched by absolute path from an unrelated working directory. Runtime
environment contains only an empty PATH and required Windows system-root variables. Node, pnpm,
Cargo and rustc are absent from that PATH; no adapters, packages, compiler checkout or toolchain
are installed beside the executable. Native workers start the copied executable directly through
the unchanged bounded, cleared-environment process runner. This is bounded relocated **harness**
installation/use proof. It is not a public CLI clean-machine installation or an OS sandbox against
absolute executables installed elsewhere on the host.

## Remaining acceptance gates

#414 stays open. Ordinary compiler installation/use without Node/pnpm requires coordinated CLI,
installation and architecture-gate integration after reviewed #413. Public defaults remain on the
bootstrap route until complete Linux/Windows frozen M1–M3 parity, exact diagnostics/spans, IR,
manifests and all target artifacts, complete M0–M4 gates, public clean-machine CLI smoke and
reproducibility checks pass. The existing #508 downstream-parity proof remains independent.
This harness does not claim linked execution, native executable equality, exhaustive ownership
coverage, manifests, transition comparison mode, provider migration or bootstrap deprecation.
