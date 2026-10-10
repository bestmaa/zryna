# Bounded server clock/status arrangement

`server-clock-status-v1` is the distinct source-checkout granted capability example for #401.
It is a review candidate. It does not expand installed beta support or close #401/#402.

```text
zryna serve <ENTRYPOINT> --profile server-clock-status-v1 --export <NAME> --node <ABSOLUTE_PATH> --server-config <ABSOLUTE_PATH> --listener-approval <ABSOLUTE_PATH> --guest-request <ABSOLUTE_PATH> --guest-approval <ABSOLUTE_PATH> --name <FRESH_STEM> [--root <PATH>] [--json]
```

The authenticated source, original root/Node binding, single no-argument i32 export, status
200–599, empty response body, literal 127.0.0.1 listener and finite budgets are exactly the
[status profile](WASI_SERVER_STATUS_V1.md) constraints. The audited fixed component arrangement
additionally imports and consumes one monotonic clock read before executing the status export.
It discards the timestamp. This adds no source-language clock intrinsic or HTTP timestamp output.
The status-only profile retains empty guest grants and rejects guest input flags.

## Separate guest request and root approval

In addition to the existing configuration-bound listener approval, provide a caller-private
guest request with exactly these fields:

```json
{"world":"zryna:capability-profiles/server@0.1.0","requests":["clock"],"clock":{"monotonic_reads":1,"subscriptions":0,"timers":0}}
```

A different caller-private root guest approval binds the SHA-256 of those exact bytes:

```json
{"schema":"zryna.wasi-server-guest-approval.v1","request_sha256":"<64 lowercase hex characters>","monotonic_reads":1}
```

Listener permission never grants guest clock access. Missing/mismatched approval, zero or extra
reads, duplicate/unknown fields, unknown or repeated capabilities, subscriptions and timers reject
before compiler preparation or binding. Guest requests are at most 2048 bytes and root guest
approvals at most 1024 bytes. Exact limits are inclusive; the first extra byte rejects. Both retain
the existing caller-private no-follow same-handle identity checks: Unix ownership/mode 0600/one
link; Windows protected owner/SYSTEM ACLs and no-delete sharing. Original guest inputs are
revalidated alongside source, Node and listener inputs at admission, response and final commit.
Unix replacement/removal stops service and suppresses uncommitted records; Windows retained
handles prevent the tested writes/replacements.

The one-read budget is per request Store. No subscriptions, timers, wall clock, randomness,
environment, filesystem, outgoing networking or process permission is granted. Existing memory,
fuel, resource, callback, concurrency, framing, attempt and absolute deadline limits are unchanged.
Each new Store receives its own exact one-read grant only after explicit admission.

## Actual observations and result identity

The separate create-only bundle contains `zryna-wasi-server-manifest-v2.json`, schema
`zryna.wasi-server-manifest.v2`, profile `server-clock-status-v1`. The v1 status record is unchanged.
The v2 record retains the common source/component/WIT/configuration/listener and teardown
identities, adds operation `clock_read_then_status`, exact requested/effective `clock` grant,
one-read limits and original guest request/approval identities, and records observed clock reads
and denied callbacks. No raw timestamp, request body or private input path enters the result.

Approved authority is distinct from execution: cancellation or service expiry with no admitted
requests records zero clock reads and zero Stores. Malformed transport requests consume attempts
without guest execution. A rejected guest request may have consumed its read before rejection;
read counts are actual observations and need not equal served responses. Every Store consumes at
most one read. Observed teardown and create-only publication retain the status profile rules and
exit categories. Forced termination releases OS handles but cannot certify graceful cleanup.

The [local denied/granted example](../examples/wasi-server/CLOCK_EXAMPLE.md) requires all four
private input files. Retain both original server test targets, command/browser regressions,
required local conformance and exact-candidate supported-host evidence. Independent review and
whole #400/#401/#402 acceptance still govern integration and public support.
