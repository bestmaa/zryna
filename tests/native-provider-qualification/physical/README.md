# Frozen physical allocation group qualification

This internal Linux x86-64 harness executes physical allocation ordinal 4 of the frozen
`owned-shared` fixture through native-retained first, then bootstrap, plus score-43 calibration.
Both use the unchanged canonical native runner: two provider pairs/four actual observations.
Mode 6 (`0x26000004`) fails the real allocation hook before `malloc`; it is separate from
logical code 2/ordinal 2. `owned-shared-fault-2-2` supplies only the independently frozen cleanup
oracle: Cleanup(module 1, function 0, place 0), then Drop(String). Physical outcome is Allocation.

The explicit `--group string` successor executes only frozen String physical ordinals 2 and 4,
plus returned score-17 calibration: three provider pairs/six actual native observations. Selector
`0x26000002` uses `string-fault-2-1` only as its Allocation/empty-trace oracle; `0x26000004`
uses `string-fault-2-2` as its Allocation/Cleanup(module 1, function 0, place 1), Drop(String)
oracle. Empty canonical result traces remain omitted. Both cases share the frozen `text` import
edge and complete main.zry/math.zry source graph. Logical fault code 2 is never executed here.
The exact String Rust selector is separate from the existing owned-shared test. Default group
selection remains owned-shared; there is no aggregate/all-group execution selector.

The explicit `--group owned-vec` successor executes only frozen OwnedVec physical ordinal 10
(`0x2600000a`) plus score-41 calibration: two provider pairs/four actual native observations.
The exact OwnedVec Rust selector uses mode 6 at the actual allocation hook. Frozen logical row
`owned-vec-fault-2-6` supplies only the Allocation and complete ordered cleanup oracle:
Drop(Sequence), Drop(String), Cleanup(module 1, function 0, place 3), Drop(Sequence),
Drop(String), Drop(String). Repeated drops are preserved. The frozen `aggregate` import edge
and full source graph are bound independently; logical code 2/ordinal 6 is not executed.


The explicit `--group vec` slice executes frozen Vec physical ordinals 2, 3 and 4, plus
score-13 calibration: four provider pairs/eight actual native observations. Mode-6 selectors
`0x26000002`, `0x26000003` and `0x26000004` fail actual allocation hooks. Their trace-only
oracles are `vec-fault-2-1`, `vec-fault-2-2` and `vec-fault-2-3`, respectively. Ordinal 2 has
an omitted empty trace; ordinals 3 and 4 require Cleanup(module 0, function 0, place 1),
then Drop(Sequence). All fault outcomes are Allocation; logical code 2 is not executed.
Vec has only frozen main.zry and no dependency/import edge. The existing imported fixture
contract remains mandatory for owned-shared, String and OwnedVec. Vec admission discharges only its
three probes, with nine outside the group; cumulative qualification must retain separate
prior seals. The recovered OwnedVec seal independently credits its one row;
local Vec admission alone cannot establish hosted or cumulative qualification.

The explicit `--group handles` slice selects physical ordinals 1 through 5 plus score-23
calibration: six provider pairs/twelve actual native observations. Every injected outcome is
Allocation with actual mode-6 selectors `0x26000001` through `0x26000005`. Ordinals 1 and 2
use `handles-fault-2-1` only for the omitted empty trace. Ordinals 3, 4 and 5 use frozen logical
Refcount rows `handles-fault-4-1`, `handles-fault-4-2` and `handles-fault-4-3` only for cleanup
traces; their logical outcomes/selectors are never substituted. The full traces preserve
Cleanup(module 1, function 0, places 1; 3 then 1; or 5 then 3 then 1), the corresponding
Shared/Weak drops, ReleaseImplicitWeak, and ReleaseControl in order. The frozen `handles`
import and both main.zry/math.zry sources remain mandatory. Admission is all-or-nothing for
all five probes and positive calibration; synthetic hostile controls never prove native execution.

`run.py` binds the clean exact source head/tree, all tracked input bytes, pinned tools and actual
libtest executable. `verify.py` independently checks the complete observations, frozen sources,
import graph, strict manifest structure, artifact hashes and entire provider bundle equality.
Version-2 receipts additionally bind the selected group to its exact test and complete case census.
The private Linux workflow explicitly executes/admit handles only and names its archive distinctly.
No row is discharged unless the entire selected Rust execution and all observed pairs pass.
It does not reconstruct source-dependent layout/ABI digests. Hostile reader controls are synthetic
and never constitute target execution. Hosted execution is Linux-only; Windows native is unsupported.

A valid canonical frame is emitted only after `zryna_m3_finish_invocation` releases scratch records
and rejects remaining owned/control allocations. This establishes exact frozen cleanup trace and
zero-live finalization. **Physical attempt/allocation/release counts are not exposed and are not
claimed.** Such a claim requires coordinated runtime/harness/decoder telemetry; a logical drop trace
or standalone C fixture cannot substitute. Source access, process budgets, runtime ABI and public
provider selection remain unchanged. Node is pinned for the existing private/bootstrap route;
this is not installed no-Node/no-Cargo acceptance. OwnedVec admission credits only its one probe;
eleven probes lie outside that selected group. Cumulative coverage may combine the prior
owned-shared and String rows only with their separately verified sealed b6 and cd5 checkpoints,
leaving eight probes open at the OwnedVec checkpoint. A separately sealed Vec successor brings
cumulative qualification to seven of twelve, leaving the five handles probes. Local or hosted
admission alone does not assign that cumulative credit. A separately qualified and sealed handles
successor can discharge those five; allocator counts and independent installed no-Node/no-Cargo
acceptance remain separate and unclaimed.
