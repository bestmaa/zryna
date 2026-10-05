# Owned-shared physical allocation qualification

This internal Linux x86-64 harness executes physical allocation ordinal 4 of the frozen
`owned-shared` fixture through native-retained first, then bootstrap, plus score-43 calibration.
Both use the unchanged canonical native runner: two provider pairs/four actual observations.
Mode 6 (`0x26000004`) fails the real allocation hook before `malloc`; it is separate from
logical code 2/ordinal 2. `owned-shared-fault-2-2` supplies only the independently frozen cleanup
oracle: Cleanup(module 1, function 0, place 0), then Drop(String). Physical outcome is Allocation.

`run.py` binds the clean exact source head/tree, all tracked input bytes, pinned tools and actual
libtest executable. `verify.py` independently checks the complete observations, frozen sources,
import graph, strict manifest structure, artifact hashes and entire provider bundle equality.
It does not reconstruct source-dependent layout/ABI digests. Hostile reader controls are synthetic
and never constitute target execution. Hosted execution is Linux-only; Windows native is unsupported.

A valid canonical frame is emitted only after `zryna_m3_finish_invocation` releases scratch records
and rejects remaining owned/control allocations. This establishes exact frozen cleanup trace and
zero-live finalization. **Physical attempt/allocation/release counts are not exposed and are not
claimed.** Such a claim requires coordinated runtime/harness/decoder telemetry; a logical drop trace
or standalone C fixture cannot substitute. Source access, process budgets, runtime ABI and public
provider selection remain unchanged. Node is pinned for the existing private/bootstrap route;
this is not installed no-Node/no-Cargo acceptance. Eleven other physical probes remain open.
