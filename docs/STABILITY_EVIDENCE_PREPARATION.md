# M7 stability evidence preparation — issue #418

This is a review candidate for evidence infrastructure, not an M7 closure report or a 1.0
compatibility promise. The versioned registry is `tests/stability-gates-v1.json`; its raw bytes
are SHA-256 pinned in `scripts/stability-gates/registry.mjs`. It composes current authorities
without modifying compiler behavior, shared CI, existing gate registries, limits or toolchain pins.
The authenticated roadmap and public profile documents remain the support authorities.

## Ownership and open dependencies

#418 owns evidence inventory, validation and eventual reviewed thresholds. Component defects go
back to their owning issue; no result has a waiver or skip override. #414 owns native-provider
activation, #416 generic Option/Result implementation, and #417 C interop. Their implementation
and complete conformance are prerequisites, together with completed public M4–M6 support required
by the chosen 1.0 scope. Existing #400/#410/#413 and new #401/#405 work retain their own ownership.

Every receipt has `closureStatus: "blocked"`. Registry v1 cannot accept M7 closure, even with all
current commands passing. Further blockers include final advertised-contract migration rules,
reviewed performance/memory thresholds, both-host complete gates, independently authenticated
hosted evidence, bounded fuzz/property campaigns, current dependency audit/dispositions,
independent clean-build reproduction, release provenance and independent threat-model review.
Specification tests for packages, components or interop do not establish their public activation.

## Current compatibility inventory and migration boundaries

| Contract | Existing authority / proof | Boundary preserved |
| --- | --- | --- |
| M1 language, CLI, typed results and artifacts | M1 fixed-oracle portable and Linux three-target CLI tests | Default I32V1; unsupported Boolean source stays rejected |
| M2 source, diagnostics, manifests and artifacts | Complete canonical M2 runner and manifest contract tests | Explicit control-flow-v1, manifest v2 and existing negative/resource inventory |
| M3 source, diagnostics, manifests and artifacts | Complete canonical M3 runner and public-doc tests | Explicit data-ownership-v1, manifest v3 and existing exclusions |
| Diagnostics | Diagnostic protocol-v2 hostile fixtures | Versioned closed shape, stable codes and authoritative spans |
| Providers | Provider-v4 frozen snapshots, malformed/budget/recovery/order fixtures | Bootstrap current authority; complete M1–M3 migration/parity belongs to #414 |
| Packages | Package/release and source-trust positive/negative fixtures | Scoped contract and implemented trust boundaries; chosen public M5 completion still required |
| Release inputs | Distribution receipt compatibility and protected-release evidence tests | Exact versioned receipts; a new candidate requires its own authenticated provenance |

Unchanged current inputs retain their versioned observations and exclusions. A future incompatible
change needs a separately reviewed contract version and migration policy. These preservation
rules are preparation; final 1.0 migration rules for every advertised contract remain blocked.
Package/FFI design tests are never substituted for runtime, foreign cleanup or published support.

## Collect and replay exact-revision evidence

Use a clean isolated checkout with the repository-pinned Node, pnpm and Rust, frozen dependencies,
and the native prerequisites in README. Commit new tooling before collection. Each output is a
new private directory outside the source checkout, with existing regular, trusted parents.
Existing outputs are refused; failures preserve known logs without recursive source cleanup.

```text
node --test tests/stability-gates-v1.test.mjs tests/stability-gates-source-process.test.mjs
node scripts/stability-gates/check-tests.mjs
node scripts/stability-gates/run.mjs compatibility /tmp/zryna-418-compatibility
node scripts/stability-gates/run.mjs security /tmp/zryna-418-security
node scripts/stability-gates/run.mjs performance /tmp/zryna-418-performance
node scripts/stability-gates/run.mjs all /tmp/zryna-418-all
node scripts/stability-gates/validate.mjs /tmp/zryna-418-all
```

The guarded CI runner selects evidence, source/process, interruption and selection guard suites.
All 27 exact named cases must execute once with nonzero TAP totals and no failures,
cancellations, skips or todo cases. The existing Linux/Windows `adapter-platform` matrix runs
this mandatory step after frozen dependency installation; its failures propagate through the
adapter and M0 aggregates. Portable preflight also executes selection and workflow mutation
guards. The distinct Rust insertion point used by #405 remains intact. This registration does
not collect a performance baseline or certify blocked M7 prerequisites.

Collection validates HEAD's full commit and tree, hashes every regular tracked source against its
Git blob (including files hidden by index flags), and hashes the ordered complete source inventory.
It rejects dirty, staged, untracked, linked, oversized or case-colliding source. Reinspection after
each gate and before receipt publication rejects source changes. Declared ignored build/dependency
outputs remain outside this source-byte proof; frozen dependency acquisition, clean builds and
independent artifact reproducibility are separate required evidence.

The receipt binds the registry digest, source inventory, host, lane, observed tool versions, every
registered command digest, ordered attempts, exit status/signal/error, wall time, test counts and
exact stdout/stderr sizes and hashes. The validator independently reads logs and recalculates test
counts and assessments. Unknown fields, missing/duplicate/extra/reordered gates, changed commands,
stale source, substituted logs, zero tests, cancelled/skipped/ignored proof, timeouts, output
exhaustion and waived blockers reject or record non-passing results. Exact selectors must actually
execute nonzero tests. Aggregate M0/M2/M3 also require their canonical completion banner.

Exit 0 means the selected nonempty current-support lane passed and its receipt is consistent;
it never means M7 or 1.0 passed. Exit 1 records a failed/blocked lane or rejected input. Performance
cannot currently return a pass, because its reviewed baseline is absent. A partial lane records
the uncollected lanes. These local receipts are integrity-bound observations, not signatures or
proof of an honest runner: independent hosted provenance and review are deliberately mandatory.
Replaying a Linux receipt on Windows is refused; each host validates its own source-bound receipt.

## Performance methodology and resource policy

The first workload invokes the existing exact portable M1 fixed-oracle CLI test. It compiles and
runs the three wrapping-i32 fixtures on JavaScript/WebAssembly and checks their results. One
successful warmup precedes five measured executions. Cargo/build-cache, architecture checks,
test harness and runtime startup are included; this is end-to-end compile/run suite cost, not
an isolated compiler microbenchmark. There is no unsupported workload or changed semantic oracle.

Linux measurement requires the direct regular `/usr/bin/time` GNU tool, monotonic parent wall
milliseconds and its `%M` maximum-resident-set KiB observation. This is the wait/resource-accounting
high-water measurement, not peak concurrent summed memory for every descendant. Missing GNU time
is blocked. Windows memory methodology and separately isolated compile/run workloads remain open.
One maximum, median and normalized `(max - min) / median` spread are computed for wall and RSS.
Numeric ceilings and maximum spread must be positive and finite; exceeding either fails. The
comparator has independent tests at the ceiling, above it, and beyond variance limits.

Registry v1 thresholds are explicitly `null`. No timing collected here can teach its own passing
baseline. Before a future version freezes limits, reviewers must select workloads, CPU/OS and
isolation policy, pinned tool/binary identities, warm/cold-cache policy, memory method, repeated
independent baseline runs, maximum wall/RSS limits and variance policy. These measurement scopes
must match the exact candidate; unrelated machines or commands cannot provide its baseline.

## Threat boundaries and remaining proof

| Boundary | Current harness evidence | Still required for closure |
| --- | --- | --- |
| Untrusted source/provider/IR/MIR | Full source, syntax, frontend, IR, MIR and driver suites including ignored boundary tests | Both-host execution, bounded fuzz/property campaign and #414/#416 completion |
| Paths, processes, caches and cleanup | Driver hostile suites; no-shell bounded runner, exact source inventory and regular evidence reader | Complete component-owner isolation/cleanup tests and independent threat-model review |
| Packages and components | Package/trust and WIT capability contract negative tests | Chosen public M4/M5 support with real capability/isolation/cleanup conformance |
| Foreign resources | Explicit #417 blocker | Wrong library/allocator, partial failure, repeated release and reverse-client proof |
| Release/supply chain | Protected-release and release-evidence rejection fixtures | Current dependency audit, independent clean builds, artifact comparison and authenticated source-to-release chain |

The runner executes only trusted registry commands using literal argument vectors, no stdin and
bounded combined output. Scoped SIGINT/SIGTERM handlers remain active through cleanup and evidence
writing. The first signal wins; repeated signals do not restart cleanup, and disposed handlers
are removed. Linux freezes the live owned group and captures descendants, including separate
descendant groups, through bounded `/proc` ancestry and PID/start-time identity checks before
killing the known tree. Capture is limited to 8,192 process records, 16 passes and one second;
unreadable or changed identities, exhausted capture, or unconfirmed removal fail cleanup. The
existing ten-second cleanup deadline, command deadlines and combined output ceiling remain.
Windows uses bounded direct `taskkill /T /F`. Regression tests deliver actual OS signals on Linux;
Windows dispatches the Node signal-handler events and executes real tree termination. Native
Windows console/CI hard-termination coverage remains a separate closure requirement. SIGKILL,
forced host termination and descendants that sever observable ancestry before capture cannot be
made safe by JavaScript signal handlers; the runner is not a native execution sandbox.

Handled interruption stops collection before another gate or performance sample starts. Known
raw logs and completed results are retained in `interruption.json` with format
`zryna.stability-interruption.v1`, the first signal, actual process error/terminal observations,
source reinspection and the uncollected inventory. CLI status is 130 for SIGINT or 143 for SIGTERM.
This journal is incomplete failed evidence: it does not fabricate missing attempts or test counts,
does not write a qualifying `receipt.json`, and cannot pass the versioned lane receipt validator.
Cleanup uncertainty is retained as `CLEANUP`; confirmed cancellation is `ECANCELED`, with a null
exit code and the parent signal. A missing executable also has a null exit code, rather than a
negative libuv spawn status incorrectly presented as a child process exit.
This runner and existing compiler tests are not an OS sandbox for hostile arbitrary native code.
Evidence reads reject persistent links and detect final-file replacement/modification; concurrently
hostile ancestor swaps and arbitrary Windows reparse attributes need stronger platform-specific
capability readers and are outside this preparation reader's proof. Use private trusted directories.

Supported collection hosts are Linux x86-64 and Windows x86-64. Linux provides native execution;
Windows retains portable JS/Wasm and explicit native unavailability. Other architectures, native
Windows output, full C interop, generic public support and broader package/component/tooling claims
remain subject to their owning contracts and dependencies. Local passing tests cover only these
scoped models and never imply universal security, performance or stable support.
