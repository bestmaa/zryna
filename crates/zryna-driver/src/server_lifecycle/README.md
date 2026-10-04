# Internal server lifecycle candidate (#401)

This module is a lifecycle-only implementation candidate for the driver. It is loaded by the
registered Cargo test target `server_lifecycle` and has no public selector or production runtime
call site. It does not establish
server component emission, a verified component identity, HTTP bindings, a capability grant,
guest execution, or supported host evidence. Issue #401 remains open.

The unchanged pinned world is `zryna:capability-profiles/server@0.1.0`, exporting
`wasi:http/incoming-handler@0.2.12`. Its explicit imports are monotonic clock, wall clock,
outgoing HTTP and secure random, all at `0.2.12`. It has no filesystem or environment import.
The WIT closure and grant composition must be authenticated before a future runtime adapter
starts this lifecycle. PR #513, inspected read-only at
`0445169bb98d738a214de7ebcd914240aec73acb`, supplies command-specific implementation-candidate
mechanics, not a reviewed server grant or execution interface. No code from that branch is
imported here.

## Internal behavior

Startup validates every explicit limit before creating one owned deadline worker. This slice
admits already-decoded `GET` and `POST` requests, a printable ASCII origin path beginning with
`/` (at most 256 bytes, no space, control byte or fragment), a byte body, and a host monotonic
deadline. It is not a raw HTTP parser. Headers, streaming, keep-alive, trailers, outgoing HTTP,
DNS, listeners and arbitrary HTTP frameworks are not implemented by this module.

The fixed upper bounds are 64 live request reservations, 65,536 incoming body bytes,
65,536 outgoing body bytes, 8 MiB reserved body/metadata storage, and a five-second request
lifetime. Configuration may lower these limits; zero and the first extra unit reject. Before
copying input, admission charges its exact method, path and body lengths plus the entire
configured maximum response size. Guest linear memory, stack and fuel are separate future
runtime obligations, not measured by this buffer ledger.

The registry owns buffers and at most one transferred adapter resource per request. Resource
transfer is an ownership operation only: the future trusted adapter must independently verify
and grant its resource before transferring it. Request and cancellation handles cannot recover
or clone the resource. This candidate supplies no guest capability; a denied callback destroys
the request. A second resource transfer rejects and drops only the rejected resource.

Cancellation is idempotent. Request identifiers never reuse a live or retired identifier within
one host, and handles retain their original registry identity across repeated host starts.
Dropping a request revokes it. A deadline worker autonomously removes expired requests, wakes
when an earlier deadline is admitted, and destroys their buffers and resources without another
callback. Callback and response boundaries independently check expiry. This does not interrupt
running guest code: a future runtime must enforce fuel, memory and epoch interruption itself.

A response consumes the request, admits a status in 200–599 and at most the configured body
limit, and prepares its bytes while holding the publication/cancellation lock. Invalid responses
also destroy the request. Cancellation that removes the request before preparation prevents a
response; cancellation after that consuming claim is idempotent. After resource destruction,
successful publication rechecks host activity
and expiry under that lock. Termination or expiry during cleanup suppresses the response even if
response preparation started first. A successful final check orders the response before later
termination. HTTP outparam binding and wire publication remain future adapter obligations.

Resources are destroyed outside the registry lock. Their quota remains charged until actual
destruction completes. Shutdown stops admission, destroys outstanding entries, joins the owned
worker, and waits for concurrent cancellation/destruction before returning success. Resource
destructor failure stops the host and detaches all sibling requests in one locked transition,
then destroys them outside the mutex and returns `Host`; it never
certifies successful cleanup. Trusted adapter destructors must be bounded and must not retain
or recursively shut down their own host. Fatal process termination and operating-system failure
do not acquire a cleanup guarantee from Rust destructors.

## Verification and integration seam

The dedicated harness tests real owned buffers, actual threads, autonomous expiry, competing
admission and publication, instrumented resource destruction, blocked-destructor quota reuse,
shutdown waiting, panic recovery and repeated startup. A one-shot test-only stop hook pauses
outside the mutex to deterministically check sibling revocation before cleanup and termination
during response destruction; reentrant destruction checks that no mutex is held and no resource
is destroyed twice. All original 21 cases remain, with three termination regressions added.
The reentrancy assertion joins the owned deadline worker at the stopped boundary first,
so unrelated worker contention cannot be mistaken for a mutex held across destruction.
Those observations are distinct from
WASI component execution, imported-capability enforcement and supported-host conformance.

Normal Cargo execution with the repository-pinned Rust toolchain is:

```sh
cargo test --locked -p zryna-driver --test server_lifecycle
```

The target is also discovered by the unchanged workspace test command in Linux and Windows M0.
Registration does not establish Windows execution evidence until that hosted target actually runs.
Standalone execution remains available:

```sh
rustc --edition 2024 --test -D warnings \
  crates/zryna-driver/src/server_lifecycle/harness.rs -o /tmp/zryna-server-lifecycle-tests
/tmp/zryna-server-lifecycle-tests --test-threads=2
```

Runtime wiring requires a
reviewed server artifact and incoming-handler adapter, reviewed grant admission and revocation,
per-store memory/fuel/deadline limits, and exact-revision Linux and Windows execution evidence.
The unchanged enclosing issue still requires its fixed server corpus, public local-only example,
conformance, documentation and host evidence before any public support claim.
