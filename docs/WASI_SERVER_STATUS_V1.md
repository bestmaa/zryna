# Bounded source-checkout server status profile

`server-status-v1` implements the first bounded public server slice for #401. It is a review
candidate and does not expand the immutable beta distribution's support matrix or close M4.
Nonempty granted guest execution and the whole #401/#402 evidence remain required.

```text
zryna serve <ENTRYPOINT> --profile server-status-v1 --export <NAME> --node <ABSOLUTE_PATH> --server-config <ABSOLUTE_PATH> --listener-approval <ABSOLUTE_PATH> --name <FRESH_STEM> [--root <PATH>] [--json]
```

The source-checkout route validates architecture, retains one workspace-relative source through
`WorkspaceSourceRoot`, authenticates pinned Node.js 22.22.1 and the protocol-v2 provider, verifies
I32V1 semantics and IR, then emits and independently audits the pinned server component. One source
with one no-argument i32 function is admitted. Dependencies, extra functions/interfaces, non-i32
results, raw snapshots/IR/components, arguments and guest grant flags are rejected. Responses have
an empty body and status 200–599. Invalid computed statuses reject the request without a response.
Installed distributions reject this route. Linux x64 and Windows x64 need exact-candidate evidence
before support is claimed.

## Configuration and explicit root permission

Configuration is a caller-owned private regular file with only these JSON fields:

```json
{"listen":"127.0.0.1:0","attempts":1,"header_bytes":1024,"body_bytes":64,"request_ms":2000,"service_ms":30000}
```

A separate root listener approval binds the SHA-256 of the exact configuration bytes:

```json
{"schema":"zryna.wasi-server-listener-approval.v1","configuration_sha256":"<64 lowercase hex characters>","allow_listen":true}
```

Both files retain no-follow directory/file capabilities, bounded same-handle reads, privacy and
immutable state. Unix requires caller ownership, mode 0600 and one link; Windows requires the
existing owner/SYSTEM private ACL and no-delete sharing. Ancestor links/reparse points, duplicate
or unknown fields, false/mismatched approval and files over 1024 bytes reject before compilation
or binding. Identical replacement bytes cannot replace the retained file identity.

Root listener permission is separate from guest capabilities. Requested and effective guest grants
are empty. Clock, randomness, environment, filesystem, outgoing networking and process authority
stay denied. The pinned HTTP resource operations remain narrowly bounded. Private clock probes
do not become public source permission.

| Limit | Accepted range |
| --- | --- |
| Listener | Literal IPv4 127.0.0.1; port 0 selects an ephemeral port |
| Attempts | 1–64; malformed clients count |
| Header / body bytes | 64–8192 / 1–65536 |
| Absolute request deadline | 1–5000ms; no greater than service lifetime |
| Service lifetime | 1–30000ms |
| Concurrent requests | 1 |
| Guest memory / fuel | 65536 bytes / 100000 units |
| Guest resources / callbacks | 5 / 8 |

HTTP/1.1 GET/POST requires a bounded path, loopback Host, canonical Content-Length and
Connection: close. Transfer encoding, upgrades, persistent connections and extra buffered bytes
reject. Every response closes its connection. Stores and reservations are reused only after actual
owned state is destroyed.

## Observations and publication

JSON mode emits separate newline-delimited readiness and final result records. Readiness has
version 1, command `serve`, kind `ready`, profile `server-status-v1` and actual endpoint; it is
emitted only after authenticated preparation and binding. The final record has kind `result`,
execution counters and a portable manifest path.

The distinct create-only `<stem>.wasi-server-run` bundle contains `component/<stem>.wasm` and
`zryna-wasi-server-manifest-v1.json`, schema `zryna.wasi-server-manifest.v1`. It binds source,
component, WIT and preparation identities, exact empty guest grants, original root approval,
configuration digest/limits, actual endpoint, attempt/served/rejected counts, terminal outcome and
actual Store/resource/listener teardown. It contains no request bodies, private input paths or
arbitrary host error text. Existing bundles are never replaced.

Original source, Node, configuration and approval are rechecked before bind, during idle admission,
before each Store, before response publication and before final commit. Changes stop admission and
suppress uncommitted responses and final records. A framed request keeps its original absolute
deadline. The driver readiness value can request cancellation; serving joins teardown before
publication. Forced process termination closes OS handles and produces no graceful certificate;
an uncommitted private transaction directory may require later cleanup.

Outcomes are attempts_exhausted, service_deadline, cancelled and host_failure. Attempts exhausted
means finite service completion; malformed individual requests may still be rejected. CLI exit 0
reports that completion; exit 5 reports deadline/cancellation/host failure. Input rejection is exit
2, authority/preparation failure exit 4, unconfirmed cleanup/record failure exit 6; architecture
retains its existing category. Diagnostics C4201–C4205 respectively identify selection/input,
retained identity, preparation, service/teardown and record failures.

Run `cargo test --locked -p zryna --test wasi_server`, retain both private server targets, existing
command/browser suites, complete local gates and exact-head Linux/Windows checks. This slice
cannot waive granted guest execution or authorize M4 closure. See the
[local example](../examples/wasi-server/README.md).
