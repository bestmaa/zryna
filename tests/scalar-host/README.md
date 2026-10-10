# Private scalar host conformance

Issue #387 uses one source-bound `ControlFlowV1` artifact, its retained scalar ABI, and explicit
empty host-interface requests. Node and browser seals share immutable artifact bytes while their
host-specific interface and binding hashes differ. This is the pure, non-package specialization
of the accepted #357 contract. Package-backed #380 composition, generated declarations/loaders,
non-scalars, WIT/components, public host activation, and complete #359 I1/C1 remain separate.

## Required execution lanes

All commands require the repository-pinned Node 22.22.1, pnpm 11.18.0 and Rust 1.97.1. Set
`ZRYNA_TEST_NODE` to the absolute authenticated executable. The browser resource test also needs
`ZRYNA_TEST_BROWSER_ROOT` pointing to the acquired receipt's `browser` directory.

Run the ordinary source/interface/consumer tests and the separately scheduled resource proof:

```text
cargo test --locked -p zryna-driver scalar_adapter_interface:: -- --nocapture
cargo test --locked -p zryna-driver scalar_adapter_interface::tests::host_consumers::pinned_real_browser_and_node_execute_the_same_sealed_scalar_corpus -- --ignored --exact --nocapture
cargo test --locked -p zryna-driver scalar_adapter_interface::tests::host_consumers::retained_esm_transport_accepts_exact_ceiling_and_rejects_first_extra -- --ignored --exact --nocapture
```

The second command must execute exactly one test, with 14 fixed positive calls (seven repeated)
and 39 malformed carrier cases in **each** real host. The typed result arrays must agree; the
same artifact hash and distinct expected host/interface/binding hashes must be retained.
An independent sentinel observes zero target entries for rejected inputs; the same cases also
exercise the unchanged generated artifact wrappers directly where an export exists. This does
not modify the artifact to instrument body calls. Console text and process exit codes are not
returned language values. The third command must execute exactly one independent transport test:
the complete 32 MiB ESM imports and returns typed i32 42 under the existing production deadline,
and the first extra input byte rejects before process launch. Its raw transport fixture does not
substitute for source verification or browser conformance.

The ordinary test command ignores the resource test and therefore supplies no browser proof.
The dedicated `browser-conformance.yml` workflow executes the two actual-browser tests and the
32 MiB transport test on Linux x64 and Windows x64. Its runner, `node
scripts/scalar-host/run-conformance.mjs <absolute-new-evidence-directory>`, verifies the frozen
archive hash, publisher receipt and complete inventory before execution, checks each cargo run
executed exactly one passing non-ignored test, and retains exact-commit receipts and bounded logs.
It keeps Chromium sandboxing enabled and uploads no browser binaries. A missing fixture,
acquisition or sandbox failure fails the lane rather than skipping it.
All commands above are required in the separately provisioned Linux x64 and Windows x64
conformance/CI lanes before #387 acceptance. A missing fixture, unreviewed hash, timeout or cleanup
failure fails that lane; it is never a successful skip. Keep the required frozen install,
structure, preflight, M0, full M2 and supported Linux/Windows hosted gates. In particular retain
`sealed_runtime_executes_public_m2_javascript_and_webassembly_exports` and existing runtime input,
frame, timeout and process-tree cleanup tests for the shared transport change.

## Test-only dependency and acquisition review

Only `playwright-core@1.63.0` is added to root development dependencies and the lockfile, using
the exact registry SHA-512 integrity in `browser-pin.json`. It has no runtime dependencies and
requires Node >=20. The runner is Apache-2.0, from upstream commit
`1b025d7e20a026371cd5f98ba0cdce48892737c8`; its exact browser manifest selects full Chrome for
Testing 153.0.8010.12, revision 1243. The browser is subject to its Chromium and bundled third-party
licenses. Retain the entire acquired archive and its license/notice/ABOUT inventory for review;
do not commit or redistribute browser binaries. Upstream sources:

- [Runner release and license](https://github.com/microsoft/playwright/tree/1b025d7e20a026371cd5f98ba0cdce48892737c8)
- [Exact browser manifest](https://github.com/microsoft/playwright/blob/1b025d7e20a026371cd5f98ba0cdce48892737c8/packages/playwright-core/browsers.json)
- [Registry metadata and integrity](https://registry.npmjs.org/playwright-core/1.63.0)
- [Chromium license](https://chromium.googlesource.com/chromium/src/+/main/LICENSE)

Only after the separate acquisition grant, run `python scripts/scalar-host/acquire-browser.py`.
It accepts no URL/path/platform overrides and uses the two fixed upstream HTTPS URLs. Acquisition
is create-only under this tree's `.zryna/cache`, bounded to 256 MiB compressed, 1 GiB expanded,
30,000 entries and depth 32. A shared 120-second elapsed deadline starts before preparation and
is checked throughout transfer, extraction, inventory traversal, every file/archive hash and
receipt generation/write. These are cooperative checks: blocking I/O may return after the
deadline, but late completion fails rather than being reported as successful acquisition.
Unsafe paths, links, special entries, encryption,
duplicate identities and expansion overruns reject. No browser, dependency installer, package
script or system setup command executes. A failed acquisition retains its bounded private
directory for inspection; remove only that verified task-local directory before retrying.

The initial trust is the official HTTPS source, **not an upstream browser digest**: upstream
publishes a version/revision, but no browser cryptographic hash in the runner manifest. Review the
receipt's configured and final response URLs, resulting archive SHA-256, full file inventory
SHA-256, executable and notices before copying the
two approved hashes into the platform pin. A platform remains launch-disabled while either hash
is null.
No channel selection, system-browser fallback, headless-shell replacement or shared cache is used.
Redirects are restricted before following them to the exact initial URL and the corresponding
Google `chrome-for-testing-public` URL recorded as `publisherUrl` in the pin; the final response
must match that publisher URL exactly. Both destinations match the
[Google version manifest](https://googlechromelabs.github.io/chrome-for-testing/153.0.8010.12.json).
Any new redirect destination requires review, even if it uses HTTPS. URL provenance does not
replace the reviewed archive/inventory integrity pins.

The runner checks every inventoried file before and after execution, exact runner/browser versions,
and the artifact hash in both the parent Node process and the real browser page. It uses one
browser, context and page; serves the blank page through an in-memory route; imports the retained
ESM Blob; blocks other page requests and service workers; and records unexpected workers/requests.
It attempts every page/context/browser close even after failures. The Rust parent applies a
30-second deadline and the existing five-second cleanup reserve, with actual process-group
cleanup on Linux and Job containment on Windows. Failure to confirm cleanup is a failure. Routing
is test observation, not an operating-system sandbox or ambient-browser isolation claim.

## Transport and emission accounting

Production invocation sends raw retained source on stdin, with no JSON or base64 on that channel.
The exact existing 33,554,432-byte artifact ceiling therefore remains representable. The inline
script is separately checked against 16 KiB, including the selected sealed signature and at most
256 arguments. Output is exactly one canonical bool byte or four signed-i32 bytes, normalized
through the scalar ABI. Production timeout remains five seconds, plus existing cleanup reserve.

Node receives into one exact-sized buffer and checks its length and SHA-256 before import. The
child's data URL adds at most 44,739,244 base64 characters plus the 28-character prefix. Buffer,
encoded string, URL representation and engine decoding/parse storage are distinct costs; this
does not claim unchanged peak process memory versus file import or a hard RSS bound. The buffer
reference is released before import; engine garbage-collection timing is not guaranteed. Rust
retains the source and the existing bounded stdin-writer copy. No emitter pass is added: #381's
two emissions still perform four render passes and can retain two 32 MiB artifacts at sealing.
Explicit test-only host-view verification shares the final artifact allocation through `Arc`.

Browser conformance serializes a test-only packet, separately limited to 1 MiB source and the
same 32 MiB stdin limit including JSON/helpers/metadata. That fixture bound is not a production
artifact ceiling or evidence for full-ceiling transport. Required full-ceiling execution remains
separate from the small cross-host scalar fixture.

## Candidate evidence

The implementation is an unverified local candidate. Linux x64 and Windows x64 browser
archive/inventory hashes are pinned from reviewed publisher-HTTPS acquisition receipts. Frozen
installation and metadata-only fixture preparation do not establish host execution. Rust source
tests, Node target execution, real browser execution, resource limits, preflight, M0, M2 and hosted
conformance are **UNRUN** until an exact candidate receipt records observed commands, nonzero counts
and platforms. This file specifies required evidence; it records no passing execution.
