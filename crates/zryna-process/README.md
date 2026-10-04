# Zryna process creation

This dependency-free foundation owns the process-wide exclusion between Linux x86-64
private executable snapshot writers and Zryna-owned child creation. Driver native/runtime
commands, frontend workers, architecture Cargo probes and in-process test helpers share
one gate. The writer closes its file before releasing the guard. Spawn releases the guard
before waiting for child exit or consuming output. Poison fails closed.

The `test-observation` feature exposes read-only, thread-specific pending-spawn observations
for deterministic regressions against real callers. It does not bypass or change the gate.
Independent embedding-application process creation remains outside this cooperative policy.
