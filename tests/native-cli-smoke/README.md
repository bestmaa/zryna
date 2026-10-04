# Private source-workspace native CLI proof

This preparatory route is compiled only with `native-provider-internal` (off by default).
The hidden `build --native-frontend` switch is explicit, build-only and source-checkout-only.
It does not activate issue #414, change installed distributions, select a frontend by default,
or remove ordinary builds' Node requirement. Project, installed, run and component selection
remain outside this route.

The driver retains reviewed #413 source handles through syntax/semantic verification, backend
preparation, artifact staging and atomic publication. M1 keeps one entry source. M2 uses the
reviewed sealed v3 closure. M3 retains its reviewed v4 snapshot, then re-parses and re-seals an
owned closure against the same immutable source-map identity and graph records so existing
ownership preparation/publication can consume it without modifying the reviewed source API.
There is no fallback, raw IR authority or copied source session. Private M2 capture selects the
canonical M2 bare-import and cycle rejection presentation over bound tokens and its retained graph.
The existing manifests v1/v2/v3, artifact bytes, source hashes and create-only transactions remain
in use. Architecture validation still requires the pinned Cargo toolchain.

Build and retain both CLI binaries from the same clean committed checkout (Cargo targets must
belong to that checkout; never copy a cache whose `CARGO_MANIFEST_DIR` names another checkout):

```sh
cargo build --locked -p zryna
cp "$CARGO_TARGET_DIR/debug/zryna" /tmp/zryna-default
cargo build --locked -p zryna --features native-provider-internal
cp "$CARGO_TARGET_DIR/debug/zryna" /tmp/zryna-native-feature
python3 -B scripts/run-native-cli-smoke.py \
  --root "$PWD" --default-cli /tmp/zryna-default --feature-cli /tmp/zryna-native-feature \
  --node /absolute/pinned/node --cargo /absolute/pinned/cargo --rustc /absolute/pinned/rustc \
  --output /tmp/new-native-cli-evidence
```

On Windows retain `zryna.exe`; supply absolute tool paths. The runner uses a new external directory
and a PATH containing only an empty directory for every CLI invocation. Node, npm and pnpm are
absent from that PATH; bootstrap receives its pinned Node executable explicitly. Native receives
no `--node`. Cargo and rustc are explicit so architecture validation remains intact. This proves
private source-checkout build behavior; it does **not** prove a clean ordinary installed CLI can
compile without Node, pnpm or Cargo. A separate control removes Cargo and requires the unchanged
architecture failure `ZRYNA-A1101`.

Nine real positive builds cover M1, the full registered M2 closure and seven M3 ownership sources, including three imported closures.
For those closures, the runner copies the registered wrapper and dependency bytes into new
task-owned `.zryna/cache` fixture directories as `main.zry` and `math.zry`; it binds original
registry SHA-256 values and rechecks every generated source before and after the proof.
For each source, the default bootstrap and opt-in native CLI must emit byte-identical complete
bundles, including the manifest, and identical complete success JSON. A repeated native build must
reject replacement without changing any published byte. Evidence retains both complete bundles
and every stdout/stderr. Six negative entrypoints must reject with their frozen code and publish no
final bundle. Additional controls check the feature-disabled binary, ordinary Node requirement,
project/component rejection and retained Cargo requirement. The receipt binds the exact Git head,
tree, tracked source digests and binary digests before/after; any proof failure makes the runner
exit nonzero. Existing output paths are never removed or replaced. Generated bundles move only to
new evidence paths after their exact hashes are recorded and recovered.

This is a bounded CLI smoke. Exhaustive downstream evidence is the independent
[`native-provider-corpus`](../native-provider-corpus/README.md) harness. The five M2 admission
failures from the earlier baseline are retained in historical evidence and covered by private
M2 diagnostic selection and three added CLI entrypoints. Fault-injection, ordinary installed
no-Node proof and public-activation gaps remain open. There is no acceptance waiver or new skip.

New private unsupported component selection uses `ZRYNA-C1013` (request rejection, status 2).
Existing codes, serializers, default Node issuers, process/grant boundaries and release materials
are unchanged. Pinned base: merged #413/#414 preparation at
`0635c19f922f7af61fa1e05b1a632b1013e908e7`; minimum reviewed #413 API ancestor
`af415a682330b1015818e9c0a8d61555fdd8d18c`.
