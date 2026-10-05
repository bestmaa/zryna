# Restricted playground compiler transport

This thin one-request stdio transport composes the driver's existing scalar source compiler,
mandatory verifier, browser component audit and generated bindings. It returns source-bound
structured diagnostics and in-memory authenticated component/loader/declaration bytes. It does
not write source or publish artifacts, select a target, run user programs or define language rules.

Trusted startup configuration supplies one fixed material root with the authenticated Node
22.22.1 and captured TypeScript 6.0.3 provider closure. Requests contain only a protocol version,
monotonic browser revision and at most 4096 UTF-8 source bytes under logical `src/main.zry`.
The response is a bounded binary frame: four-byte little-endian metadata length, exact UTF-8 JSON,
then its three ordered artifacts. Rejected source carries actual compiler diagnostics and no
executable payload. A malformed request rejects before provider discovery or launch.

The executable is not a sandbox. An independently authenticated external supervisor must place
its complete process tree in a private namespace/delegated bounded cgroup before supplying
untrusted input. The separate language-server and diagnostic-query contracts remain unchanged.
Public toolkit authentication/publication and full issue 410 acceptance remain pending.

The dedicated Linux capture path requires inherited soft and hard limits of CORE 0, FSIZE
256 MiB and NOFILE 512 after namespace material-copy setup. It lowers FSIZE to 12 MiB while
capturing and staging the authentic nine-file provider closure, with a 16 MiB aggregate bound.
It retains and revalidates that stage, then lowers FSIZE to 1,277,956 bytes and NOFILE to 128
before authenticating and probing Node. Compilation uses the existing verifier and audited
component path. Consuming completion checks owned-stage cleanup before returning response
bytes; a cleanup failure retains the primary failure and produces no successful frame.
Generic library capture and language-server discovery do not apply these irreversible limits.

The actual parent setup envelope remains a prerequisite: a child of a process with a 1 MiB
hard FSIZE limit cannot raise it for material copying or staging. The parent's effective
ancestor memory bounds must also permit the separately required compiler envelope. These
source changes do not establish that topology, authenticate a new binary or grant execution.

Focused executable tests live in `tests/lifecycle.rs`. The separately ignored authentic-provider
regression requires an independently authenticated installed material root selected through
`ZRYNA_PLAYGROUND_TEST_MATERIALS` and an externally owned native process-tree envelope. It must
be executed explicitly for lifecycle acceptance; the default test run does not establish that
proof. Genuine corpus parity, hostile fatal-stage cleanup and Linux containment remain pending.
