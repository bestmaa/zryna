# Private server response arrangement

This backend module emits an internal candidate for
`zryna:capability-profiles/server@0.1.0`. It does not select, start or grant a
server host. The driver must authenticate source/composition, the exact
component, its explicit host policy and limits before construction. Public
server support, listener selection and the complete #401 acceptance remain
separate work.

`emit_server_response` accepts a verified scalar program and one existing
no-argument i32 export. The compiler-produced core is embedded unchanged. A
separate fixed bridge invokes that export and rejects values outside 200–599.
It constructs fields and an outgoing response, sets the computed status,
retrieves the outgoing body and finishes it with no trailers or bytes. It then
consumes the response and outparam through `response-outparam.set`, and drops
the incoming request. An error discriminant from a supported host operation
traps before successful completion.

`Reply` invokes no external capability. `ClockRead` additionally invokes
`wasi:clocks/monotonic-clock@0.2.12.now` once, as a fixed private host-policy
proof arrangement. This is not source-level clock requirement admission. A
driver must deny that call unless its separately authenticated policy grants
it. The module contains no environment or filesystem arrangement.

The exact authenticated world preserves its four explicit capability imports
and all eight resolved imports, including HTTP types and the required IO type
dependencies. Incoming-handler resource type exports alias the imported HTTP
resource identities. They are not fresh substitute resources. Required host
operations are the fields and outgoing-response constructors, response status
setter and body getter, outgoing-body finish and response-outparam set. The
empty body arrangement creates no stream or pollable resource.

The component has three core modules: the retained scalar core, a canonical
memory/allocator module, and the response bridge. They share one fixed 65,536
byte memory, with no memory growth or start functions. The allocator reserves
the first 1,024 bytes for canonical result storage and supports bounded fresh
allocations with alignments 1, 2, 4 or 8. Exhaustion and existing-allocation
resize requests trap. Empty allocations return zero. This private arrangement
only needs allocation for a possible host error string; it does not expose a
general guest allocator. Complete component bytes are capped at 1 MiB.

Canonical operation signatures come from the authenticated WIT resolver and
must match the fixed bridge. Body results use caller-provided return storage.
Body finish carries a canonical realloc option because its error variant can
contain strings. Response-outparam set carries a memory option for the same
reason, even though its successful branch contains only an owned resource.

Before sealing or revalidation, the final-byte audit checks bounded payloads,
the exact three retained modules and the canonical arrangement replay derived
from authenticated WIT and compiler output. Modified bytes are rejected before
recursive validation or WIT decoding. It then independently validates each
core as WebAssembly 1.0, validates the complete component and decodes the exact
eight imports and one handler export. A matching digest alone is insufficient.

Server-local signature, binding, byte-envelope and final-audit failures use the
stable diagnostic `ZRYNA-W4030`. Upstream WIT authentication and scalar emission
failures retain their existing diagnostic codes. The hostile-world test checks
that a well-typed component with substituted WASI versions receives the server
audit rejection code.

The backend does not publish a response or certify host cleanup. The driver
must keep outparam output provisional, enforce request and guest-store limits,
destroy resources and the Store, and perform the lifecycle's final activity
check before returning a response. External grant admission, cancellation,
deadline interruption and supported-host execution evidence belong to that
driver boundary.
