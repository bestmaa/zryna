# Private bounded loopback transport

This module completes the independently implementable transport and pure-source portion of
#401, in the existing `server_runtime` dedicated test target. It is not registered in the shipped
driver library or CLI. The public selector, accepted #400 grant composition, granted/denied local
example and reviewed supported-host evidence remain integration dependencies. No new public
profile or grant approval is established here.

Preparation consumes one immutable `SourceMap` snapshot through the normal authenticated
TypeScript worker, source-bound syntax verification, semantic lowering, verified IR and the
independent final component audit. Only the no-argument `i32` export can compute a status
200–599; the pinned WASI handler constructs an empty response body. This transport always chooses
`Reply`, empty requests and `deny_all`. Neither source bytes nor configuration can request clock,
environment, filesystem, random or outgoing network authority. The private runtime's separate
clock-policy probes remain distinct from accepted source/interface grant admission.

Configuration is strict JSON, at most 1024 bytes, rejecting duplicate/unknown fields. It names
only a literal `127.0.0.1` socket address (port 0 requests an ephemeral port), 1–64 total accepted
attempts, 64–8192 header bytes, 1–65536 body bytes, a 1–5000ms absolute request deadline and a 1–30000ms
service lifetime. Request time cannot exceed service time. Revalidation precedes binding, including
typed configuration substitution. Serving is serial: one listener and at most one accepted
connection, held through framing, guest execution, publication and cleanup. Malformed attempts
also consume the total-attempt budget. Idle acceptance and incomplete frames cannot outlive the
service deadline. A stop handle interrupts socket I/O and revokes any registered guest; joining
`Bound::run` certifies graceful cleanup.

One HTTP/1.1 request is served per connection. `GET`/`POST`, a printable relative path up to 256
bytes, matching literal `Host`, one canonical decimal `Content-Length`, and `Connection: close`
are required. Headers are ASCII, at most 32 fields, with case-insensitive duplicate rejection.
Transfer coding, upgrade, expectations, trailers, folded headers, LF-only framing, oversized or
truncated frames and already-buffered extra data are rejected before creating a Store. Later
bytes cannot create another request because the connection closes after this single frame; there
is no shared proxy hop, keepalive, streaming body, HTTP/2 or TLS support. Unknown bounded headers
are discarded and do not supply guest authority.

Exact-boundary loopback tests send a 65536-byte opaque body, a complete 8192-byte header
(including its final CRLFCRLF), and 32 fields through the authenticated production frontend and
real guest Store. Separate attempts add the first extra body byte, header byte or field while
holding the other dimensions at their maxima. Each rejection creates no Store and is followed
by a valid maximum-size recovery request. Cumulative owned-input bytes, Store/resource creation
and destruction, zero live socket handles and transport reservations, and final listener closure
prove admission and cleanup. Terminal TCP resets are allowed for early rejection; timeouts are
failures. These cases retain the existing request, service and attempt limits.

The same real transport accepts a 256-byte path, rejects the first extra path byte before Store
creation, and recovers at the maximum body, complete header and field count. `POST` plus that path
charges all 260 routing bytes. A separate service successfully handles all 64 permitted attempts
at these intersecting limits, destroying 64 Stores and 320 resources, with zero live inputs,
socket handles or reservations after every attempt. Its listener closes before the earliest
possible service expiry, proving attempt-budget exhaustion followed by actual joining and socket
closure. Path and body rejection are composed transport/lifecycle evidence; these cases do not
isolate the framing checks from the lifecycle's duplicate bounds.

Transport reserves `header_bytes + body_bytes + 1024` bytes before framing. Header-name slots
borrow the bounded header buffer, with no copied-name map. The extra 1024 bytes cover 32 borrowed
name slots, bounded routing copies and the fixed response. The raw header is dropped before body
allocation; the decoded transport input is dropped once the registry and worker own their two
separately charged copies. Guest memory, fuel, resource and callback bounds remain the existing
runtime envelope. These are bounds on specified objects, not all allocator overhead, compiler/JIT
heap or OS TCP buffers/backlog.

The execution worker destroys its Store and resources and joins before publication. The consuming
commit holds the lifecycle registry lock through bounded writing and socket cleanup, ordering
publication against cancellation. Cancellation before commit suppresses all response bytes;
cancellation after commit begins sets the transport stop flag, interrupts I/O and waits for bounded
cleanup. Bytes already written cannot be recalled. Skipped publication callbacks are destroyed
outside the registry lock before the entry is retired. During earlier guest cancellation/expiry,
the independently charged transport reservation and serial admission gate remain held until the
actual socket closes, even if guest teardown has already detached its lifecycle entry. A new
accepted socket never reuses transport quota while any previous handles or buffers remain.

`corpus/status.zry` and `corpus/config.json` drive a real private child-process/client corpus.
The fixture receives bounded explicit source/configuration bytes over stdin; it adds no weaker
filesystem admission implementation. The selector `server-loopback-private-v1` belongs solely to
this unshipped test protocol. Its readiness reader has an absolute 40s bound and 4096-byte line cap.
The process uses the existing spawn gate and a Unix process group or Windows kill-on-close job.
The Unix production frontend owns a separate group: before readiness, the fixture supervisor
allows its existing 30s session plus 2s cleanup budget to finish before terminating the fixture.
Readiness is emitted after normal frontend cleanup. Graceful receipts inspect actual listener/socket/buffer/Store
destruction. Forced termination proves OS descriptor/connection reclamation only, not guest Drop.

Run the production-provider, loopback, exact-boundary, malformed-frame, cancellation, deadline, publication,
repeated-start and child-process corpus with:

```sh
cargo test --locked -p zryna-driver --test server_runtime
cargo test --locked -p zryna-driver --test server_lifecycle
```

Both require the repository's frozen adapter dependencies and provisioned Node. Tests do not
silently skip a missing frontend. Shipped CLI registration, trusted source capture/root approval
and accepted current-grant/result-record composition must be coordinated with their owners before
this private seam becomes a public server path. Streaming, outgoing HTTP and random providers
are not necessary for this minimum request/response interface.
