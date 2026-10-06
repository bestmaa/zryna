# Private complete verified IR observations

This standalone proof compares complete sealed M1/M2/M3 IR at the exact local revision. It
adds no production API, serializer, provider registration or CLI switch. The combined revision
registers this proof in the existing private Linux/Windows lane through exactly three reviewed CI
files; the collector and receipt implementation remain reserved to this directory.

The independently derived inventory uses pinned H4 Git commit
`0a5f86b77a84c37e19e0dd59388c8a782d385131`. An unchanged exact-head legacy corpus run
accepted 107 IR cases: M1 source1/run3; M2 source4/run20; M3 source47/context14/run15/
runtime-invalid2/ABI-invalid1. These include 66 source/context occurrences and 52 fixture groups.
An ABI invocation rejection does not erase accepted source IR. Source syntax/semantic negatives,
fault cases without observations, and blocked obligations are not promoted to accepted cases.

Each case binds its explicit entrypoint, canonical FileIds, logical paths, original Git source
origins, exact UTF-8 bytes/lengths/hashes, named import targets and all import spans. Physical M3
source cases reproduce the sorted legacy traversal's mutable `conformance/math.zry` overlay at
that case, retaining the immutable dependency-body Git origin. Contexts that repeat a path retain
separate IDs and snapshot hashes. `inventory-review.json` is the separate read-only reviewer's
complete independent rederivation; it approves this inventory only.

**Graph provenance remains explicit:** 103 graph slots have independently derived hashes;
85 are also observed in the unchanged H4 corpus; 18 invocation cases lack H4 graph observations;
4 M1 cases are inapplicable. The exact 18 missing-H4-observation obligations remain open in the
inventory and admission report. The new collector captures all103 graph hashes, source closures,
targets and spans through existing private APIs and checks each against the independent derivation.
Those new observations cannot be relabeled as observations from the unchanged H4 receipt.
Graph encoding does not include targets or spans, so the reader checks them separately.

The complete canonical source/context/graph projection matches the original producer-retained
handoff, SHA256 `9e88ad02c8531da0d8563ed920ab0f52423754559b2557c6ee88dcac3af18b5a`.
Library attachment materialization remains blocked. This inventory is independently derived
from H4; it is not a recovered copy of either Library attachment.

The combined proof preserves the owner source at `38521fe59fbe70f07157129d75a51a3ed6be877f`
and uses the available sealed H4 baseline retained in `baseline-H4/`. Its raw corpus digest is
`87f65ad8140e6abc966c2551e464b33ab3b4d0ed2fef7084e843c63eec87e6d9`; its runner digest is
`8928b63be528a8b669ad48b941118b4d42a541db90265ca98b8772a06c97393d`.
All107 cases, contexts, import spans/targets, graph hashes and historical observation provenance
match the owner's independently derived inventory; only baseline receipt digest bindings changed.
These are historical Linux H4 receipts, not observations executed on the current revision or host.
The owner's separately reported plain-run `N4009` failure and subreaper success remain unmaterialized;
the combined qualification neither replaces those originals nor claims they were recovered.

The collector uses the genuine pinned TypeScript6.0.3 workers and native lexer/parser plus the
mandatory syntax verifier. Native/bootstrap closure discovery is independent; downstream pairs
use the same retained original SourceMap and revalidate it before and after lowering. It writes
both complete observations even on IR divergence. Each observation includes one unmodified whole
`VerifiedProgram` Debug value plus typed getter projections, including private/empty functions,
source authority, data declarations, ABI, places, blocks, instructions, terminators, cleanup,
backend views, nominal identity and both layouts where applicable. It neither sorts away these
values nor strips nominal identities to make providers agree.

This is a bounded test observation, **not a canonical IR wire format**. Debug bytes are bound to
this repository/compiler revision and same-run nominal SourceMap identities; their hashes are
artifact integrity receipts, not stable cross-run IR identities. The independent reader parses
the pinned Debug structure, checks its field/topology/source authority against getters, validates
retained source/context bytes, rejects missing/extra files, and admits exact provider-byte equality.
The whole sealed Debug is the complete parity authority. The reader independently checks direct
getter facts, source authority and topology. Derived drop/backend observations use exact same-run
pair comparison; the reader does not replay ownership state or independently recompute those
verifier-derived facts. Hostile controls refresh artifact hashes before testing divergence and
both-sided omissions.
These are receipt controls, not runtime/fault/OS execution evidence.

Run from an exact clean descendant of qualified H5 `ec0cab5b4669dedd3e41fddda45449db34f73ce0`,
whose further changes stay within this directory and the three explicit CI registration files.
The runner still requires every pinned H4 compiler/fixture authority to remain unchanged. Use pinned
Rust/Cargo1.97.1, Node22.22.1, pnpm11.18.0 and already frozen-installed adapter dependencies:

```sh
python3 -B tests/native-provider-qualification/verified-ir/run.py \
  --evidence-dir /absolute/create-only/evidence \
  --baseline-dir "$PWD/tests/native-provider-qualification/verified-ir/baseline-H4" \
  --cargo /absolute/pinned/cargo \
  --rustup /absolute/pinned/rustup \
  --node /absolute/pinned/node \
  --target-dir /absolute/owned-target
```

The baseline directory contains the exact historical H4 runner's `corpus.json` and `receipt.json`.
The TypeScript identity includes both the frozen `@typescript/typescript6`6.0.2 shim and its
locked `@typescript/old` alias pointing to the actual TypeScript6.0.3 compiler. Both complete
package trees and the original Node binary are bound before and after the proof.
The runner checks their exact raw-byte binding, independently rederives the inventory, verifies
unchanged H4 authorities, builds the external locked/offline package with two jobs and the existing
harness debug0 profile, checks formatting/strict clippy, runs the collector and hostile controls,
and rechecks exact source/compiler-input/tool/binary identity afterward. Generated outputs stay
outside the checkout. Admission without a trusted exact runner receipt is structural/byte validation,
not independent attestation that a compiler executed.

The reader validates structure and byte equality of authenticated compiler observations; it cannot
independently attest that arbitrary matching forged IR came from the compiler. Live source/binary
bindings and the original hosted archive/log chain supply that execution provenance.

This proof claims complete IR equality for its107 accepted cases. It does not claim full production
manifest parity, all injected fault outcomes, installed no-Node/no-pnpm/no-Cargo acceptance, M4/public
activation, #417 or executable FFI completion, native linking/runtime coverage, or Windows results.
Prior qualification lanes and their blocked obligations remain separate. No issue is closed and no
existing gate is weakened. Actual hosted qualification, Windows validation and original Library
comparison remain pending.


The independent admission reader binds six original command captures, the complete tracked compiler
source, both frozen TypeScript package trees, the original collector binary and the historical
baseline. The admission controls challenge 28 coherently rehashed metadata/source/stream forgeries
against an admitted real proof without running a compiler or provider again.

Tool capability admission records the original lexical selectors and their complete bounded
resolution chains, including intermediate symlinks and supported Windows junctions. Canonical
ordinary executable bytes and identities are bound before and after seven metadata queries and
each of the six proof commands. Cargo proxy metadata runs through its original `cargo` filename;
the actual compiler commands use the pinned `rustup which` paths. The artifact reader retains its
strict no-link/reparse policy unchanged. Recorded capability snapshots detect observed changes;
they do not provide atomic executable isolation or reconstruct a departed host's tool identity.
Live admission recaptures every selected capability. Original authenticated runner receipt/log/ZIP
bindings remain required for historical execution provenance.

The 34 modeled capability controls and thirteen additional real-proof capability admission controls
are separate from the existing 49 IR, 28 admission and 37 archive controls. Runner and tamper-control
receipts use compact JSON within the same 2MiB bound; tamper controls must remain under that bound
so a size rejection cannot masquerade as the intended mutation rejection. All capability records
remain inside the original receipt, preserving the closed 393-file archive census.

An open-handle rejection reports the original seven compared metadata fields, their exact
differences, the Python implementation/version and any available birth-time observations.
The bounded rejection report does not normalize timestamps or retry a failed comparison;
regular-file, size, identity and before/after stability checks remain enforced. Separate
thirteen diagnostic controls verify this report without crediting an unobserved Windows run. A report
must identify the actual host discrepancy before any proposed cross-platform comparison repair.

The dedicated current IR matrix job retains the original six IR proof steps and adds capability
controls before live admission. The qualified CLI/corpus job retains its original commands. Both
jobs retain the original 40-minute Linux and 60-minute Windows bounds, with pinned frozen setup
and separate fresh targets; no qualification step is omitted to fit the time budget.

The original archive contains exactly 393 regular files and their explicit parent directory entries.
Packing requires live admission; recovery validates the complete original ZIP before creating an
output, preserves every original member byte, rejects links/aliases/extra or missing entries, and
admits the recovered proof against the exact current source. Limits are 32MiB per ordinary member,
128MiB for the retained binary, 256MiB aggregate and 32MiB for the compressed ZIP. These archive
controls include Windows path and file-handle behavior; modeled controls do not claim a Windows run.

The separate `tool_provenance.py` diagnostic records the tool-selection paths used by the IR
runner, every original path component's link/reparse metadata, the unchanged strict guard's
original outcome, and stable canonical regular-tool bytes. It only queries pinned versions and
`rustup which`; it does not build a collector, execute a provider or admit IR. Its JSON is explicitly
`UNQUALIFIED` and retained in a separately named diagnostic artifact, outside the closed 393-file
acceptance archive. Modeled link/reparse controls do not establish a hosted path observation.

H6's Linux live admission failed while rereading a tool capability after its 107-case collector and
28 admission controls passed. The original error omitted the offending row, and packaging did not
run. The original IR receipt is identified by its hash in the authenticated log but was not uploaded;
that hash cannot recover its bytes or establish the offending path. A successor diagnostic can
establish a reproduced path/component on its own authenticated host. It cannot reconstruct the
departed original VM or promote the failed H6 admission to a pass. The link/reparse guard remains
intact, and every acceptance obligation still applies after any reviewed correction.
