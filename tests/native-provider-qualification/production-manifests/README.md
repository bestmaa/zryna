# Seven additional production BUILD closures

This private #414 qualification compares actual default/bootstrap and internal native CLIs for
the frozen ABI, bounds, enum, handles, owned-shared, weak-expired and weak-live M3 closures.
It complements the unchanged nine-source CLI proof. The source checkout must contain the
reviewed #413 API; no provider default, shared CLI, production compiler or distribution changes
are made here. The private workflow runs this separate proof on Linux and Windows.

Each host executes seven complete provider pairs and seven native create-only collision checks:
21 CLI invocations, 14 manifests and 42 target artifacts. Native executes first with an empty
runtime PATH and explicit Cargo/rustc; bootstrap uses explicit pinned Node after the owned native
bundle is recovered. Every case starts with its final output absent. Imported entries retain
their frozen wrapper/body bytes in deterministic, task-owned cache directories. That cache is
not an absent `.zryna` cold-installation claim.

Admission binds the complete source census, exact contextual import aliases and resolved targets,
canonical graph identity, closed v3 manifest schema, artifact metadata, actual file bytes, complete
CLI success bytes and create-only diagnostic. It requires all seven cases; partial evidence fails.
Both roles compare JavaScript, core WebAssembly and Linux x86-64 ELF relocatable objects on either
host. This is BUILD proof, with no native execution claim on Windows. Layout and runtime ABI
identities are shape-checked and compared; their semantics are not independently reconstructed.

ABI is source-valid. Its missing-argument B2102 conformance row rejects invocation admission;
this proof does not invent a successful runtime calibration. Bounds' runtime traps likewise do
not make its accepted source a BUILD rejection. Identical weak-live/weak-expired wrappers remain
distinct closures because their imported bodies differ. The separately classified semantic
fixtures are not silently added as successful production BUILD cases.

`run.py` consumes fresh exact-head CLIs from the existing `tests/native-cli-smoke/run_ci.py`
receipt. `verify.py --live` additionally checks live tool/binary/generated-source bytes and absence
of final output; archive admission binds retained bytes and authentic job/step evidence separately.
`verify_test.py` uses hostile synthetic controls and executes no compiler or provider. All failed
actual commands retain stdout/stderr and failed receipts; timeout cleanup is unconfirmed and can
never qualify. The archive keeps build logs and receipt metadata; hosted executable byte recovery
requires the existing separate binary artifact and is not claimed by this archive alone.

`archive.py pack` first requires live admission, then creates a proof ZIP containing exactly the
113 regular proof files and one explicit empty `empty-runtime-path/` directory entry. The private
workflow uploads this ZIP as a single file because directory uploads omit empty directories.
`archive.py admit` validates the complete archive before extracting into a fresh real destination
and applying the unchanged manifest reader. Missing directory entries fail; recovery never
reconstructs them. `archive_test.py` checks positive round trips and hostile archive mutations
without executing a compiler or provider. The existing 19 manifest-control test methods remain
separate and include positive controls as well as hostile cases.

The cumulative 12/12 Linux physical allocation qualification and previous seals remain separate.
Installed no-Node/no-pnpm/no-Cargo acceptance, exhaustive corpus closure, public activation and
full #414 acceptance remain open.
