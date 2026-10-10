# Finite local status service

This source-checkout example computes status 200 or 599 with an empty body. Guest grants are empty;
root approval permits only the exact finite loopback configuration. See the
[profile contract](../../docs/WASI_SERVER_STATUS_V1.md) for limits and candidate status.

On Linux, create fresh caller-private configuration and approval:

```sh
export ZRYNA_SERVER_INPUT_DIR="$(mktemp -d)"
python3 - <<'PY'
import hashlib, json, os
from pathlib import Path
root = Path(os.environ['ZRYNA_SERVER_INPUT_DIR'])
config = json.dumps({'listen': '127.0.0.1:0', 'attempts': 1, 'header_bytes': 1024,
                     'body_bytes': 64, 'request_ms': 2000, 'service_ms': 30000}).encode()
approval = json.dumps({'schema': 'zryna.wasi-server-listener-approval.v1',
                       'configuration_sha256': hashlib.sha256(config).hexdigest(),
                       'allow_listen': True}).encode()
for name, data in [('config.json', config), ('approval.json', approval)]:
    descriptor = os.open(root / name, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600)
    with os.fdopen(descriptor, 'wb') as output:
        output.write(data)
PY
```

From the compiler checkout, use its exact absolute pinned Node executable and a fresh stem:

```sh
cargo run --locked -p zryna -- serve examples/wasi-server/status-200.zry \
  --profile server-status-v1 --export status --node "$NODE" \
  --server-config "$ZRYNA_SERVER_INPUT_DIR/config.json" \
  --listener-approval "$ZRYNA_SERVER_INPUT_DIR/approval.json" \
  --name local-status --json
```

Once readiness supplies the port, send one request before the 30-second deadline:

```sh
curl --request POST --header 'Content-Length: 0' --header 'Connection: close' \
  'http://127.0.0.1:<port>/local'
```

The service closes its connection/listener and publishes the separate server bundle. Use a fresh
stem for another invocation. Removing or changing a retained input stops admission and prevents
the final record. Missing, false or mismatched root approval never starts a listener.

Windows uses the same JSON with owner/SYSTEM-only protected ACLs, as required by the contract.
The cross-platform fixture `apps/zryna/tests/fixtures/private_file.rs` creates those exact ACLs
for real CLI tests. Installed beta packages do not advertise this candidate selector.

The separate [clock/status example](CLOCK_EXAMPLE.md) shows denied and granted guest clock authority using the same status source and additional private guest files. Status-only guest grants remain empty.
