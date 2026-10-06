# Three remaining frozen M2 BUILD roots

This private proof retains production BUILD parity for the frozen accepted entry roots
`invalid/bare-import/dep.zry`, `invalid/case-colliding-module/Dep.zry` and `valid/math.zry`
under `tests/m2-fixtures`. The invalid directory names describe rejected importing graphs;
these three standalone entry sources are accepted by the existing frozen corpus.

Each source runs the current source-bound feature CLI with `--native-frontend`, a real
create-only collision, then the current default CLI with explicit pinned bootstrap Node.
The producer retains all nine command records and stdout/stderr, before/after collision
inventories and both complete bundles. The independent reader binds committed source blobs,
the existing CLI-build receipt and nine-case smoke, exact argv/cwd/environment/typed exits,
three fixed source/graph identities, canonical manifest-v2 bytes and exact provider bytes.
There are six manifests, 18 target artifacts and 24 bundle files.

Runtime PATH is empty; Cargo/rustc and bootstrap Node are explicit admitted tools. A separate
metadata target is used; no existing task cache is reused. This proves private source-checkout
BUILD only. It does not establish installed/default no-Node use, native RUN, complete retained
IR observation, an independent backend/layout semantic oracle or public activation. Windows
uses the same JS/Wasm/Linux x86-64 ELF object BUILD targets without executing a native object.

The complete archive contains exactly 57 regular files and the original explicit
`empty-runtime-path/` entry. Admission rejects malformed ZIP types, paths, aliases, census,
flags and bytes before creating recovery output. Missing directory evidence is not rebuilt.
Synthetic Linux/Windows reader and archive controls execute no compiler or provider.

```sh
python -B tests/native-provider-qualification/production-m2-roots/verify_test.py
python -B tests/native-provider-qualification/production-m2-roots/archive_test.py
python -B tests/native-provider-qualification/production-m2-roots/run.py \
  --root /absolute/source --head FULL_COMMIT_SHA \
  --cli-proof /absolute/fresh-cli-proof --output /absolute/new-m2-roots-proof
python -B tests/native-provider-qualification/production-m2-roots/verify.py \
  --root /absolute/source --head FULL_COMMIT_SHA --platform linux \
  --output /absolute/new-m2-roots-proof --live
python -B tests/native-provider-qualification/production-m2-roots/archive.py pack \
  --root /absolute/source --head FULL_COMMIT_SHA --platform linux \
  --proof /absolute/new-m2-roots-proof --archive /absolute/new-m2-roots-proof.zip
```

Windows substitutes `--platform win32`. The owned private workflow executes both host lanes
and retains complete original archives. Earlier H4 Linux receipts and the original 220-case
corpus census, including all 30 blocked labels, remain unchanged. This proof completes only
these three BUILD roots; global #414 acceptance still requires exact context-manifest/receipt
integration, full retained IR, M2 native execution, installed no-Node use and applicable M4.
