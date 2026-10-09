# Command compiler source fixtures

The `.zry` and protocol-v4 `.json` pairs retain complete source snapshots from the pinned
TypeScript-6 adapter. A provider-success snapshot does not establish semantic or runtime success.

Positive compiler cases include pure entry, owned aggregates, control flow, weak upgrade,
environment match, one literal argument with a trailing comma, an owned helper result,
and live String owners before the environment operation. Entry name/result/parameter/export
and outcome match/binding/type variants are intentionally rejected by command semantics or IR.

The source coverage suite also derives independent hostile snapshots that v4 accepts but H1
rejects, including omitted tokens inside broad parent spans. The semantic resource suite builds
independent complete source/DTO inventories for the exact 4096-declaration boundary and its first
extra item. No fixture grants a host capability or proves command execution.

The clone aggregate, vector and mixed-root pairs are pinned-provider snapshots for the cleanup
review candidate. Command lowering sends all three shapes through generic clone preparation;
their tests require the actual sealed GenericCloneInitializedPrefix before asserting exact
destination-frontier cleanup, preallocation failure, nested unwind and subsequent recovery.
Ordinary M3 legacy aggregate and vector prefix coverage uses separate M3 fixtures and authority.
Clone-relative faults skip the independently verified straight-line source construction probes;
the existing code-2 selector counts both construction and clone helper probes.
