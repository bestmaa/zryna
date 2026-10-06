# Private installed native capability preparation

This #414 stage adds a feature-gated installation issuer and a standalone proof consumer.
It depends on reviewed #413 source/package issuance already present in the stacked base.
It does not change ordinary installed provider selection, the production distribution schema,
CLI dispatch, source issuers, graph serialization, backend authority or target execution.
The enclosing #414 acceptance remains open.

`NativeInstallation::capture_current` accepts no caller root or digest. It admits exactly the
executing `bin/native-installation-proof` image (Windows: `.exe`),
`metadata/native-provider.json` and `LICENSE`. The canonical closed descriptor is bound to
the image at compile time with the exact source commit/tree, host and protocols 2/3/4.
Runtime environment variables cannot issue or replace that binding. An independent reader
must authenticate the initial executable against its retained actual source build before
execution; the purpose marker alone is not initial executable authentication.

The capability retains the existing installation tree/file handles and executing-image
identity. Package capture uses the existing frozen exact-package issuer. Verified native
syntax retains both source ownership and an installation borrow. Public observations
revalidate both; callbacks receive only the opaque installed capability, and both success
and error outcomes receive a postcheck. No raw syntax, source owner, closure or backend
authority can be detached through this interface. The existing native verifier consumes
its source owner on error, so no additional source checkpoint after that error is claimed.

The proof builds two actual images from clean committed source: one with the private
compile-time binding and a negative build without it. Dependencies must match the committed
Cargo lock. Every hostile mutation starts with a real positive under the authenticated
original image. Complete command, source, image, byte/state and typed exit receipts are
retained. Small fixture bytes are included in journals; executable copies reference the
retained original build bytes. Only stopped disposable fixture copies are removed afterward.
Malformed synthetic executable bytes are inspected externally and never executed.

Run on the pinned Linux toolchain layout used by this saved environment:

```sh
python3 -B tests/native-provider-qualification/installed-native-capability/run.py \
  --source /absolute/clean/source \
  --output /absolute/new/evidence-directory \
  --target /absolute/task-specific/cargo-target \
  --cargo /absolute/toolchain-root/cargo/bin/cargo
```

The installed consumer runs with an empty PATH and without Node, npm, pnpm, Cargo or Rust
runtime hooks. Building the probe requires the pinned Rust toolchain and source; consuming
the resulting private installation does not require a compiler checkout. This proves only
the bounded private preparation route. Ordinary installed-default no-Node acceptance,
complete provider parity, clean-machine installation and public activation remain separate.

Boundaries and outstanding evidence:

- The descriptor is limited to 4096 bytes, license to 65536 bytes and image to 128 MiB.
  Existing package source limits remain unchanged, including the 1024-byte file limit.
- The original full M2 main fixture is 1997 bytes and is expected to be rejected by that
  package limit. A separately frozen 293-byte import fixture and its 47-byte dependency
  exercise the bounded v3 route; this is not full M2 corpus discharge.
- Private files use initial creation modes 0600/0700 under the environment's existing umask.
  The harness changes no permission settings, umask, credentials or deployed settings.
  Windows retains the existing declared-mode policy; no Windows ACL proof is inferred.
- Linux uses the existing kernel executing-image handle. Windows uses the unchanged
  current-executable-path helper; no independent kernel-image identity is claimed there.
  Only actual execution on a host credits that host; Linux execution does not credit Windows.
- OS-denied mutations are reported as blocked effective-mutation obligations with observed
  prevention. Setup failures are neither rejection passes nor ignored controls.
- Extra unrelated source files follow the existing package issuer's owning contract and are
  recorded as an observation. This stage does not extend protected source census semantics.

The new source paths are private helpers in the registered driver component. Only the
feature-gated module registration changes an existing source file. Existing #413 source
APIs, downstream parity suites and shared CLI/pipeline glue are unchanged. This stage grants
no #400 host capability, consumes no restricted #400 proposal and changes no #531 snapshot
path. Its stacked draft dependency must be reviewed separately before integration.
