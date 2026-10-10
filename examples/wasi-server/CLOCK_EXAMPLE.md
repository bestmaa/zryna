# Denied and granted local clock/status example

This example uses the existing `status-200.zry` source with the distinct audited clock/status
arrangement. The source itself computes only the status; the generated guest arrangement reads
and discards one monotonic timestamp per request. See the
[contract](../../docs/WASI_SERVER_CLOCK_STATUS_V1.md) for exact limits and candidate status.

First create the private configuration and listener approval from [the status example](README.md).
On Linux, add two fresh private guest files in that same caller-owned input directory:

```sh
python3 - <<'PY'
import hashlib, json, os
from pathlib import Path
root = Path(os.environ['ZRYNA_SERVER_INPUT_DIR'])
request = json.dumps({'world': 'zryna:capability-profiles/server@0.1.0',
                      'requests': ['clock'], 'clock': {'monotonic_reads': 1,
                      'subscriptions': 0, 'timers': 0}}).encode()
approval = json.dumps({'schema': 'zryna.wasi-server-guest-approval.v1',
                       'request_sha256': hashlib.sha256(request).hexdigest(),
                       'monotonic_reads': 1}).encode()
denied = json.dumps({'schema': 'zryna.wasi-server-guest-approval.v1',
                     'request_sha256': hashlib.sha256(request).hexdigest(),
                     'monotonic_reads': 0}).encode()
for name, data in [('guest-request.json', request), ('guest-approval.json', approval),
                   ('guest-denied.json', denied)]:
    descriptor = os.open(root / name, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600)
    with os.fdopen(descriptor, 'wb') as output:
        output.write(data)
PY
```

The denied invocation uses the correctly bound request but approves zero reads:

```sh
cargo run --locked -p zryna -- serve examples/wasi-server/status-200.zry \
  --profile server-clock-status-v1 --export status --node "$NODE" \
  --server-config "$ZRYNA_SERVER_INPUT_DIR/config.json" \
  --listener-approval "$ZRYNA_SERVER_INPUT_DIR/approval.json" \
  --guest-request "$ZRYNA_SERVER_INPUT_DIR/guest-request.json" \
  --guest-approval "$ZRYNA_SERVER_INPUT_DIR/guest-denied.json" \
  --name local-clock-denied --json
```

It exits 2 with C4201, emits no readiness and publishes no bundle. For the granted invocation,
use `guest-approval.json` and a fresh name `local-clock-granted`. Send the same bounded POST
from the status example to the actual readiness endpoint before the service deadline.
The response is status 200 with an empty body. After one valid request, the v2 manifest records
one requested/effective clock grant, one actual clock read, one created/destroyed Store and
confirmed teardown. Use fresh names/files when repeating the example.

Windows uses the same JSON with the contract's protected owner/SYSTEM private ACLs. Its actual
CLI tests verify retained input handles prevent writes/replacement and forced host termination
releases those handles. Installed beta packages do not advertise this candidate profile.
