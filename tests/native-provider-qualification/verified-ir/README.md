# Private complete verified IR observations

This standalone proof compares complete sealed M1/M2/M3 IR at the exact local revision. It
adds no production API, serializer, provider registration, CLI switch, or workflow registration.
The source reservation is this directory alone. Shared registration remains an owner dependency
if this proof is later proposed for hosted qualification.

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

The original Library machine/human handoffs could not be materialized. Their bytes and hashes
remain unverified, and original-file comparison remains pending. This inventory is newly derived
from H4, not a recovered or proven matching copy of either attachment.

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

Run from an exact clean descendant of H4 whose changes stay within this directory, with pinned
Rust/Cargo1.97.1, Node22.22.1, pnpm11.18.0 and already frozen-installed adapter dependencies:

```sh
python3 -B tests/native-provider-qualification/verified-ir/run.py \
  --evidence-dir /absolute/create-only/evidence \
  --baseline-dir /absolute/retained-unchanged-H4-corpus-evidence \
  --cargo /absolute/pinned/cargo \
  --node /absolute/pinned/node \
  --target-dir /absolute/owned-target
```

The baseline directory contains the retained unchanged H4 runner's `corpus.json` and `receipt.json`.
The runner checks their exact raw-byte binding, independently rederives the inventory, verifies
unchanged H4 authorities, builds the external locked/offline package with two jobs and the existing
harness debug0 profile, checks formatting/strict clippy, runs the collector and hostile controls,
and rechecks exact source/compiler-input/tool/binary identity afterward. Generated outputs stay
outside the checkout. Admission without a trusted exact runner receipt is structural/byte validation,
not independent attestation that a compiler executed.

This proof claims complete IR equality for its107 accepted cases. It does not claim full production
manifest parity, all injected fault outcomes, installed no-Node/no-pnpm/no-Cargo acceptance, M4/public
activation, #417 or executable FFI completion, native linking/runtime coverage, or Windows results.
Prior qualification lanes and their blocked obligations remain separate. No issue is closed and no
existing gate is weakened. Hosted registration, Windows validation and original Library comparison
remain pending.
