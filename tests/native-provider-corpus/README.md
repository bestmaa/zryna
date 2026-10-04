# Frozen native-provider downstream comparison

This private harness compares the genuine native lexer/parser with the canonical
TypeScript 6 workers against the same source maps and mandatory syntax verifiers.
It uses existing semantic, verified IR and backend APIs. It does not change
compiler selection, installation admission, source authority or public defaults.

The runner builds an external Cargo package from these sources. Every registry
package in its generated lock must have the same name, version, registry and
checksum as an entry in the repository lock. Rust/Cargo 1.97.1 and Node 22.22.1
are required; install the repository's frozen pnpm dependencies first. Generated
packages, retained artifacts, logs and receipts stay outside the checkout.

```sh
python3 -B scripts/run-native-provider-corpus.py \
  --node /absolute/path/to/node \
  --cargo /absolute/path/to/cargo \
  --target-dir /absolute/path/to/private-cargo-cache \
  --evidence-dir /absolute/path/to/new-evidence-directory
python3 -B -m unittest discover -s tests/native-provider-corpus -p runner_test.py
```

Exact-revision runs require a clean worktree descended from
`0635c19f922f7af61fa1e05b1a632b1013e908e7`. The existing #413 minimum API pin and
previous activation/CI consumer pins remain unchanged. `--prepare-only` creates
an external package during implementation and cannot produce an acceptance pass.
Successful runner admission requires complete case coverage, exact JSON types,
no failed or ignored cases and `public_activation: false`. A provider divergence
must fail and retain its actual diagnostics and logs.

The inventory includes the M1–M3 frozen registries, their pinned fixture bytes,
all 14 M2 fixture sources and all 95 M3 fixture sources. Source syntax fixtures
can be semantic negatives: a matching rejection is an executed comparison, not
semantic acceptance or an ignored test. Runtime and artifact comparisons are
recorded only when actually executed through the relevant existing APIs.

The public APIs cannot establish complete production build-manifest parity or
M2 native executable preparation from this harness. M3 private fault observers
and publication-command behavior also require coordinated integration. Linux
native execution uses the existing audited Linux x86-64 toolchain; unsupported
host execution remains explicitly blocked. None of those gaps is a passing
case.

Ordinary installation and CLI use without Node or pnpm remain open #414
requirements. The current CLI prepares the TypeScript worker unconditionally;
installed compiler admission authenticates its bundled Node/provider inventory.
A copied harness executable, direct backend execution or removal of `--node`
alone does not prove an ordinary installed compiler. Completing that requirement
needs owner coordination for CLI, driver preparation and distribution admission,
with existing identities, runtime capabilities and verifier boundaries intact.
Public activation also requires full Linux/Windows parity, applicable M0–M4
and resource gates, and reproducible installation evidence.

The initial comparison on the pinned main API exposes five M2 source-route
failures: two bare-import cases and three import-cycle cases. Bootstrap rejects
bare imports with global `ZRYNA-F1103`; native source capture returns source-spanned
`ZRYNA-F2002` for the unsupported named import. Bootstrap cycles report
`ZRYNA-D3007`, while native capture applies ownership-graph validation before v3
selection and reports `ZRYNA-D3301`. These failures are deliberately retained;
the runner exits nonzero and cannot admit them as parity or activate the provider.
Fixing that profile-specific admission behavior requires source-owner coordination.

Linux x86-64 accounts for 220 obligations. Other hosts add 20 linked
`native-platform:*` blocked obligations, producing 240 records; portable comparisons
still execute. Receipt admission binds each ID to its exact frozen source/profile,
checks owning-phase dispositions and fixed typed runtime oracles, and requires
actual native outcomes on supported hosts. Failed runs also retain verified
before/after revision, cleanliness, input, lock and executable identity checks.

A separate [private CLI smoke](../native-cli-smoke/README.md) now exercises feature-gated native
source-checkout builds through the real manifests and transactions. Its bounded nine-source
bundle comparison does not discharge this corpus's exhaustive production-manifest obligation
or the ordinary installed no-Node acceptance gap.
