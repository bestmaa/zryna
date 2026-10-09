# Zryna native MIR

Typed straight-line native values lowered from `VerifiedProgram` or supplied as explicit raw
compiler claims. This root API is the complete implemented M1 profile.

The separate internal `data_ownership_v1` module lowers sealed M3 IR into Linux x86-64
layout-bound raw MIR and independently verifies types, places, CFG edges, cleanup and the exact
ownership-runtime symbol inventory. It emits no object and grants no link or process capability.

The separate internal `native_c_v0` module admits complete machine claims against the genuine
native C extension IR. Its independent verifier never calls the lowering producer. The immutable
seal retains original source, captured declaration/library materials, dual layouts and the runtime
issuer. It proves exact System V INTEGER lanes (six registers then distinct eight-byte stack
slots), low-width Boolean checks, sixteen-byte outgoing alignment and non-overlapping zeroed
caller output slots whose logical initialization follows the exact status-zero call.

Source values, argument order, safe/raw entry, conditional reservations and non-null registration
remain complete. Explicit private preparation preserves String byte retention, stride-four i32
reads/range checks/stride-one packing, distinct stride-four zero-extended foreign copies and
issuer-specific allocation faults. Every terminal plan ends loans before reverse mixed-owner
releases, stops immediately on release failure and transfers a result only after cleanup. Process
failure supplies no cleanup promise. Only total scalar functions have public C signatures.

The machine authority itself emits no native object. The separate backend scalar-export path
consumes this seal for total public scalar definitions, while imports and private entries remain
plans. The MIR introduces no execution instance, foreign
ledger, runtime allocation, linking, support activation or public selector. Those #417 gates and
the full tiny-C Linux failure/cleanup fixture remain required. SQLite and Rust-through-C-shim
are later independent library pilots. Focused tests are `cargo test --locked
-p zryna-native-mir --test native_c_v0`; exact-revision receipts must accompany execution claims.
Machine bounds derive from existing admitted IR cardinalities. Each effect admits at most 32
ordinary actions (the admitted 25 boundary checks plus fixed call/status/commit actions); each
terminal edge adds one action per loan, two per drop and one finish. Production checks the
complete source-derived sum for overflow before cloning claims. Independent admission checks
raw counts against those exact retained source bounds before traversing terminal actions.
No separate complete-program ceiling narrows the accepted source policy.

`raw::Module` and its nested raw types are never backend-authoritative. `verify` consumes those
claims and is the only constructor of `VerifiedMirModule`; the verified wrapper exposes only
immutable function/value/operation views and cannot be recovered as raw or mutated. `lower` sees
only sealed Universal IR views, builds the same raw shape, and passes it through that verifier.

The current verified profile proves:

- bounded symbols matching `[A-Za-z_][A-Za-z0-9_]*`, with exact and ASCII-case-folded uniqueness;
- one fixed, non-variadic scalar ABI v1 Linux x86-64 System V convention;
- `i32` parameter, result, definition, literal, and wrapping-add types;
- explicit dense value IDs defined exactly once in canonical slot order;
- existing same-function operands that strictly precede each use, plus iterative cycle rejection;
- an existing result whose type matches the signature; and
- fixed per-function, module, symbol, and diagnostic budgets.

MIR is SSA-like rather than a Universal IR tree: repeated uses, `add(value, value)`, and bounded dead
values are valid. Raw diagnostics are global and identify only bounded function/value ordinals.

The verifier also constructs and retains the authoritative scalar ABI v1 module. Verified function
views expose only its exact Linux symbol and public convention, so object codegen cannot create a
competing name mapping. The current straight-line MIR remains a foundation representation.

The separately versioned [M2 native MIR profile](../../docs/M2_NATIVE_MIR.md) implements an internal
raw-to-verified block, call, symbol, Boolean, and terminator boundary without changing these root
types. Its lowering accepts only sealed Universal `ControlFlowV1`, maps every identity and operation
one-for-one, and independently reseals the complete program. M2 object emission, public Boolean
wrappers, linking, execution, FFI, and product linking remain later gates.
