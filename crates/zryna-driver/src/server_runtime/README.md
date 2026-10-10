# Private server execution acceptance slice

This dedicated test target connects the merged lifecycle lease to an authenticated executable
`zryna:capability-profiles/server@0.1.0` component. The component retains all eight exact resolved
WASI 0.2.12 interfaces, computes a status through compiler-produced verified scalar IR, constructs
real HTTP response/body resources and finishes an empty body. This runtime owns the in-memory
execution and bounded publication seam; its dedicated target also loads the private loopback
transport described in `../server_transport/README.md`. The source-checkout `server-status-v1` composition is documented in
`../../../../docs/WASI_SERVER_STATUS_V1.md`; its guest grants remain empty. Existing
runtime probes retain their verified private test provider; the transport exercises the normal
authenticated production worker before the same driver lowering.

Preparation binds immutable source bytes, the verified program, complete component bytes,
authenticated WIT closure, fixed arrangement revision, admitted grants and request/store limits.
The backend independently validates final bytes and exact world and replays the fixed arrangement;
startup repeats source/arrangement/seal validation. Caller-supplied raw components cannot enter the
normal route. All HTTP resource identities are fresh within this runtime family and store-local;
there is no shared command/server identity allocator or cross-profile resource transfer.

Grant requests are strict bounded JSON with the exact server world. Host approval is a separate
trusted token. Empty requests never acquire ambient authority, even if the host token allows
clock reads. Private runtime probes admit 1–16 explicitly approved monotonic reads. The public
clock/status composition admits exactly one read per Store. Timer/subscription requests and network/random grants remain unsupported and fail before
engine construction. Environment/filesystem/command fields fail closed. Unused exact-world
capability imports trap before any provider access. Legitimate owned HTTP resource destructors
release objects under empty grants; response construction itself does not grant outgoing network.
`ClockRead` is a fixed arrangement used by the private probes and public clock/status
composition. It does not implement source-level requirement analysis or #400's final reviewed
grant interface.

Each admitted request receives one fresh Wasmtime Store with fuel, epoch interruption, a 64 KiB
stack, one bounded 64 KiB memory, zero tables, three core instances, at most eight owned resource
objects and at most sixteen host callbacks. Request/response bodies and metadata are reserved by
the existing lifecycle. Its aggregate buffer cap is narrowed to 4 MiB here, reserving the other
4 MiB for the execution-owned input copy. Linear-memory limits do not bound compiler/JIT memory
or all process heap; the component-byte audit and finite request/resource quotas bound the specific
objects admitted by this slice.

The execution thread exclusively owns the Store. The lifecycle retains a revoker, never a locked
Store. Guest callbacks check independent revocation/deadline state and cannot synchronously retire
their own lease. Cancellation, autonomous expiry and shutdown increment the engine epoch, wait
until the actual Store and its resource registry are destroyed, and join the execution thread.
Lifecycle quota stays reserved during that cleanup. Epoch wakeups check each request's own token
so another request's cancellation does not grant permission to cancel its sibling. A provisional
response cannot escape until Store destruction and the lifecycle's final activity/deadline check.

Tests execute scalar-derived statuses and a real clock callback; reject malformed, expired and
oversized requests before Store creation; exercise resource/fuel/memory/callback failures through
actual guest execution; reject invalid source results and grant/envelope substitution; and pause a
real callback to prove cancellation/deadline/shutdown wait for destruction and suppress responses.
The three existing lifecycle termination regressions are also loaded unchanged in this target.

Every runtime cancellation caller explicitly awaits the control's teardown and join, including a
duplicate caller that observes an already-detached lifecycle entry. A two-caller callback test
rejects early cleanup certification. Actual CPU probes also require an active sibling's epoch
callback to continue after another request's cancellation, with its own Store still executing.
An already-computed and fully cleaned guest response is separately cancelled before publication;
the final lifecycle check must suppress that candidate. Test-only raw CPU/growth fixtures and
excessive fuel bypass the fixed arrangement solely to isolate runtime interruption evidence;
neither override exists in the normal preparation interface.

Execution-owned input copies are constructed only after revoker retention and while the startup
gate is held. Revocation waits for that gate and for the staged copy to be destroyed; an expired
startup cannot create a guest Store or release its reservation while those bytes remain owned.
The existing expiry owner may finish detached-entry bookkeeping just after guest/input teardown;
quota remains reserved through that final bookkeeping, and shutdown waits for it as well.

Whole #401 acceptance remains open: reviewed #400 interface composition, source/interface capability
admission, shipped build/run selectors, reviewed granted/denied public local example, current
supported-host evidence and public support documentation remain dependencies. The private loopback implementation
and fixed pure-source process corpus are independently implemented in the same dedicated target.
Streaming bodies, outgoing HTTP and random providers are not requirements for the minimum interface.
The source-checkout status and clock/status compositions remain review candidates; whole-issue
completion and installed public support are separate gates.

Production composition also selects the distinct one-read clock/status arrangement after exact caller-private guest request/root approval admission. Private multi-read/subscription/timer probes do not become public source requirements. The same Store-local accounting records actual clock reads in the separate public v2 record; empty-grant status behavior is unchanged.
