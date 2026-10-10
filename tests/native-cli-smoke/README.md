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

The normal pull-request activation workflow also has a distinct **private native CLI** job on
Linux and Windows. It checks out the current PR head, fetches locked dependencies, and uses
`run_ci.py` to build both CLIs in one fresh external target directory. Real Cargo/Rustc paths
come from `rustup which`; the Windows system environment remains available while every CLI
invocation retains the empty PATH used by the smoke. `verify_ci_receipts.py` independently
requires all 21 unique outcomes, rehashes tracked/generated sources and binaries, and compares
the retained complete bundles and rejection responses. Failed or partial receipts reject.
Receipt hostile-case tests use synthetic files and establish no compiler/platform result.

Current-head CLI job/artifact identities include the current head, run and attempt. They are
separate from the same workflow's unchanged immutable-consumer preparation at `6c0f3f64…`.
Small proof artifacts retain all bundle bytes, stdout/stderr and admitted receipts; separate
executable artifacts preserve both compiled binaries. Windows qualification requires the actual
current-head Windows job and admitted receipt to pass; a Linux result is never substituted.
This remains build-only JS/Wasm/fixed-target object proof, with Cargo explicit and required.
No native execution, ordinary installed no-Node acceptance or public activation follows from it.

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

The separate `m2_semantic_oracle.py` reconstructs all thirteen ordered fields and canonical
wire bytes for one retained M2 successful all-target build. Its artifact lengths and hashes
come from the independently frozen M2 conformance registry, not candidate files or provider
agreement. It rehashes the two original source files, recomputes the graph, separately checks
the resolved edge target, and admits both complete provider bundles and success responses
against the sealed `3f23fb7` local/Linux/Windows proof. The new oracle revision is separate
from that old producer revision; this executes no compiler and does not re-label receipts.
Synthetic hostile controls demonstrate rejection of jointly rehashed provider payloads that
the earlier bounded header/inventory reader accepts. The existing hostile suite runs these
controls with its normal command on both hosted CLI platforms.

```sh
python3 -B tests/native-cli-smoke/m2_semantic_oracle.py \
  --source-repo /absolute/clean/3f23fb7-source-recovery \
  --checkpoint /absolute/sealed/3f23fb7-evidence \
  --output /absolute/new-one-m2-semantic-admission.json
```

M2 manifest v2 has no layout or runtime-ABI fields; those belong to M3 v3. Native object bytes
here target Linux x86-64 on both host platforms. This adds no Windows-native execution
requirement. One successful build content case does not close exhaustive production-manifest
parity, run-result fields, publication/fault history, verified-IR reconstruction, M3 identities,
the thirty corpus blockers, or ordinary installed no-Node/no-pnpm/no-Cargo acceptance.

New private unsupported component selection uses `ZRYNA-C1013` (request rejection, status 2).
Existing codes, serializers, default Node issuers, process/grant boundaries and release materials
are unchanged. Pinned base: merged #413/#414 preparation at
`0635c19f922f7af61fa1e05b1a632b1013e908e7`; minimum reviewed #413 API ancestor
`af415a682330b1015818e9c0a8d61555fdd8d18c`.
