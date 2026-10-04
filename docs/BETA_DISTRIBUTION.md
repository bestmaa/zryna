# Beta distribution definition

Status: distribution contract implemented through [#422](https://github.com/zryna/zryna/issues/422),
[#407](https://github.com/zryna/zryna/issues/407) and
[#406](https://github.com/zryna/zryna/issues/406). The immutable
[`v0.2.3` Developer Preview](https://github.com/zryna/zryna/releases/tag/v0.2.3) has downloadable
Linux and Windows archives; [#423's completion evidence](https://github.com/zryna/zryna/issues/423#issuecomment-5663028888)
records clean-host installation and public upgrade verification. See
[installation and removal](BETA_INSTALLATION.md) for the exact observed release boundary.
The requirements below remain binding for each new candidate; this document does not independently
authorize publication or extend support. The source-only [v0.1.0 preview policy](DEVELOPER_PREVIEW.md)
remains unchanged. The external pilot and final beta record in
[#424](https://github.com/zryna/zryna/issues/424) remain open, and no stable support is activated here.

## Products and ownership

The beta packaging route bundles the pinned bootstrap provider instead of depending on native
frontend completion. The two products are `zryna-<version>-x86_64-unknown-linux-gnu.tar.gz` and
`zryna-<version>-x86_64-pc-windows-msvc.zip`, each with one correspondingly named root directory.
The selected release version must agree with the binary, manifests, tag and release subject.
It must not repurpose the source-only `v0.1.0` identity.

The intended baseline is existing admitted M1, M2 and M3 JavaScript and core-WebAssembly
build/run on Linux x86-64 and Windows x64. Existing Linux native-object semantics remain
unchanged. Linux native linking/execution additionally requires the documented, explicitly
provisioned GNU toolchain. Windows native execution remains rejected. Shipping a compiler
does not activate new syntax, profiles, browser/WASI execution or stable compatibility.

| Owner | Required responsibility |
| --- | --- |
| #422 | Finite archive inventory, deterministic assembly, installation admission, runtime/provider authentication, install verification and distribution documentation. |
| #407 | Explicit project admission, source inventory and project-owned output; create-only scaffolding; source-checkout build/run integration. |
| #406 | Protected source/tag admission, clean build environment, production release envelope, SBOM/provenance, independent reproduction, signing/attestation, immutable publication and rollback/revocation. |
| Existing compiler components | Unchanged source architecture engine, provider-neutral syntax verification, semantic and IR authorities, backend audits and driver orchestration. |

The [package/release v1 foundation](../spec/package/PACKAGE_RELEASE_V1.md) validates small
synthetic textual records. Its resource profile and signature declarations are not a binary
release format or executed signature verification. #406 must define its production envelope
explicitly; neither larger limits nor a successful fixture validation silently grants that authority.

## Installation and project authority

Source-checkout invocation retains a complete architecture-validated compiler root. The
separate project root selects only authenticated package sources and project-owned state/output.
The #407 integration carries distinct `compiler_root` and `project_root` requests; it does not
make an external project emulate `zryna.workspace.json` or supply compiler dependencies.

The later installed route replaces only compiler-root admission and frontend/runtime selection
with a verified installation capability. It reuses project/source/output admission. A project
manifest, current directory, environment variable, provider response or caller-supplied Boolean
cannot assert that the installation or source architecture has been verified.

The installed CLI locates its installation through its directly invoked executable and retains
validated filesystem identities. Every ancestor and selected file must satisfy bounded no-follow,
regular-file, containment and stable-read checks on both platforms, including Windows reparse
rejection. Installation, project and temporary execution roots remain separate authorities.
No project path may select a runtime, worker, loader, receipt or alternative trust root.

The existing architecture engine continues to run unchanged on the exact clean compiler source
before build/packaging. Its complete Cargo graph and filesystem proof cannot be reconstructed by
pretending the binary archive is a source workspace. The installed route instead requires the
authenticated source-build receipt described below and independent installed-inventory admission
before compiler phases. This explicit distinction creates no skip flag and does not remove any
gate from source builds, release builds or source-checkout commands.

## Trust chain and execution order

1. An independently configured verifier authenticates the release subject against an approved
   signing identity or attestation identity/policy. The policy binds repository, protected workflow,
   exact source commit/tree, tag/version, target, subject digest and required successful gates.
   Keys, identities and policy supplied by the downloaded archive are not trusted merely because
   that archive names them. A checksum fetched beside an archive is insufficient authentication.
2. Before the first CLI execution, that verifier checks the complete archive digest and exact
   release asset inventory. Safe extraction rejects undeclared entries and unsafe topology. The
   authenticated CLI is the root of subsequent process-local installation admission; executing an
   unverified CLI to ask whether it is trustworthy does not establish initial trust.
3. The source-build receipt binds the exact source/tree, workspace-contract and lockfile digests,
   tool identities, architecture command/result and evidence digest to the protected build. The
   release subject authenticates both receipt and produced CLI/archive. A bare `passed` field,
   source SHA alone or copied receipt from another build is insufficient. #406 owns this binding.
4. The CLI contains a build-bound expected identity for the exact provider/runtime material
   closure. It independently checks installed regular-file bytes, lengths, roles and complete
   topology against that identity. A mutable manifest next to the CLI cannot replace the expected
   digest. This check precedes any Node launch, including `--version`, and any module load.
5. After byte authentication, the driver validates the exact Node version and provider protocol
   handshake. Retained file/directory identities and bytes are revalidated through execution;
   bounded private staging protects captured provider bytes from subsequent source replacement.
   Node receives a cleared environment, explicit executable/worker, controlled working directory,
   fixed arguments and bounded process cleanup. Inherited `NODE_OPTIONS`, `NODE_PATH`, PATH
   lookups or project-local modules cannot contribute executable inputs.
6. Compiler requests compose authenticated installation, project/source and output capabilities.
   A failed admission creates no successful output bundle and never falls back to ambient tools,
   a download, a different provider or a development checkout.

The initial trusted installation and owner-private staging permissions/ACLs remain assumptions.
The contract does not claim protection from a hostile operating system or arbitrary same-user
replacement of the trusted CLI. System loader dependencies must be enumerated and audited on the
qualified host; private DLL/shared-library lookup cannot consult writable project/search paths.

## Finite archive inventory

All paths below are relative to the single archive root. Directories are precisely the ancestors
of admitted files. The final platform material lock enumerates every individual file; a glob,
directory copy, installer inventory or package-manager dependency traversal is not an allowlist.

| Role | Exact path or finite expansion rule |
| --- | --- |
| CLI | `bin/zryna` on Linux; `bin/zryna.exe` on Windows. |
| Node 22.22.1 | `runtime/node/bin/node` on Linux; `runtime/node/node.exe` on Windows. |
| Provider source | The nine files listed below under `lib/zryna/bootstrap/`. |
| Project notices | `LICENSE`, `NOTICE`, copied from the exact source revision. |
| Runtime notices | `licenses/node-LICENSE`, `licenses/typescript6-LICENSE.txt`, `licenses/typescript-LICENSE.txt`, `licenses/typescript-ThirdPartyNoticeText.txt`. |
| Linked dependency notices | Individually enumerated `licenses/rust/<package>-<version>/<filename>` entries selected from the exact target build dependency/license audit. No optional or wildcard entries. |
| User information | `VERSION`, `README.md`, `SUPPORT.md`, each bound to the release. |
| Metadata | Exact envelope paths and their non-circular coverage are defined at the assembly boundary below. |

Only the two native binaries have executable roles. A discovered additional non-system runtime
binary dependency blocks assembly until its exact inventory and license change is reviewed; it
is never added by a copier. No npm, pnpm, Cargo, compiler checkout, download helper, LSP binary,
editor package, credentials, host paths or privileged installer is included.

The provider closure consists of these nine ordinary files:

```text
worker.mjs
worker-v3.mjs
worker-v4.mjs
limits-v3.mjs
limits-v4.mjs
node_modules/@typescript/typescript6/package.json
node_modules/@typescript/typescript6/lib/typescript.js
node_modules/@typescript/old/package.json
node_modules/@typescript/old/lib/typescript.js
```

The five worker/limit modules are exact source-revision bytes. The wrapper is
`@typescript/typescript6` 6.0.2; its sole compatibility dependency resolves to TypeScript 6.0.3
materialized under `@typescript/old`. Preserve their original package manifests and loader bytes,
even where the original declaration contains a range: the distribution accepts only the locked
implementation and file digests. No dependency resolution occurs at installation or runtime.

The existing #408 tooling closure captures only protocol v2 plus the four package files. It does
not prove the complete v3/v4 CLI closure. Their separately imported limit modules must also be
captured, hashed and independently exercised. Distribution admission must not weaken the
development installation's existing pnpm-link authentication to accommodate ordinary archive files.

Every inventory tuple records portable path, byte length, SHA-256, role, executable mode, origin
material and license reference. Reject missing/extra entries, duplicate keys/paths, case-folded
collisions, file/directory prefix collisions, absolute/traversing names, Windows reserved names,
trailing dots/spaces, links/reparse points, archive hard links, special files and unstable reads.
The reviewed production schema must set inclusive bounds for each entry, total expansion, count,
depth and parsing resources before assembly implementation. Until those bounds and the complete
target-specific license list are frozen, no inventory is publication-ready.

## Deterministic identity and assembly boundary

The logical assembler consumes one approved release version, exact source commit/tree, target,
source epoch, clean compiled CLI, authenticated material set, exact architecture/build receipt
and required gate evidence. Inputs are explicit; no mutable tag resolution, network acquisition,
environment-selected command or credentials are available during assembly. #406 authenticates
and provisions inputs; #422 checks their exact identities before constructing the archive.

The output is one archive, complete ordered inventory and input/output digest receipt. #406
independently verifies those outputs, binds them into its production release subject, and alone
owns signing/attestation and publication. The agreed logical input is
`zryna.distribution-build-input.v1`, with these required groups:

| Group | Binding |
| --- | --- |
| `version`, `tag` | One release version and its exact tag; no tag creation or mutable resolution by the assembler. |
| `source` | Repository, full commit/tree, ref and `sourceDateEpoch`. |
| `target` | Triple, archive format and qualified `platformBaseline`. |
| `toolchains`, `materials` | Ordered records with name, version, origin, SHA-256 and signature-evidence SHA-256. The exact toolchain set is Cargo 1.97.1, Node 22.22.1 and rustc 1.97.1. Evidence identifies the applicable authenticated build/upstream chain; a digest alone is not verification. |
| `compiledCli` | Logical input path, SHA-256 and byte size. |
| `architectureReceipt` | Logical path, digest, format and exact source commit/tree. |
| `gateReceipt` | Logical path/digest, workflow, run ID/attempt/URL, source commit and exact required pre-assembly job results. |
| `recipe` | Explicit format and digest of the deterministic recipe. |

Logical paths resolve only within separately admitted input capabilities; no serialized private
host paths enter release identities. Production serialization, primitive types and inclusive bounds
remain #406's envelope/schema work and must be frozen before assembly implementation. Required
post-assembly install/reproduction evidence belongs to the outer release admission, not an input
receipt pretending those future checks already ran.

The protected architecture producer runs against an isolated source checkout while its tooling
dependencies remain outside that checkout. It rejects tracked, untracked and ignored workspace
differences. Rustup must select the supplied absolute Cargo and rustc paths from the exact 1.97.1
toolchain; link, reparse and proxy aliases are rejected. The producer binds those executables by
digest before and after execution, requires exact full version output, removes ambient Cargo/Rust
overrides and uses a controlled Cargo home without configuration files. The protected gate
producer requires identity encoding and reads GitHub response bodies through a one-MiB streaming
limit with a declared-length check and fixed request deadline. These controls belong to the
producer; parsing a correctly shaped receipt does not reproduce their observations.

The embedded metadata inventory is exactly `metadata/distribution.json`,
`metadata/inventory.json`, `metadata/materials.json`, `metadata/architecture-receipt.json` and
`metadata/checksums.sha256`. SBOM, provenance and signatures are detached release assets.
The sidecar `zryna-<version>-<target>.build-receipt.json` is not embedded: it binds the input
digest, archive filename/size/SHA-256, embedded record digests, inventory root, recipe and target.
#406's `zryna-release-envelope-v1.json` binds both platform subjects and the explicit detached
asset set, reproduction result, workflow identity and revocation-policy digest. A local packer
result never authorizes publication.

The hash graph must be acyclic:

- The build-bound execution-material identity covers Node, nine provider files and any explicitly
  admitted non-system loader material. It excludes the CLI that embeds its expected identity.
- `metadata/distribution.json` binds source/target and execution-material expectations; it cannot
  contain an inventory or archive digest that would introduce a cycle. `metadata/inventory.json`
  indexes every embedded file, including distribution/materials/architecture metadata, except
  exactly itself and `metadata/checksums.sha256`.
- `metadata/checksums.sha256` covers every other embedded file, including the inventory, and
  excludes only itself. Its exact bytes are covered by the outer archive digest.
- The complete archive contains payload and the closed envelope inventory. The external release
  subject hashes the archive and every detached SBOM, provenance, checksum, notes and signature
  subject attachment according to #406's explicit coverage rules. No file hashes itself; no
  executable bytes may hide in an unindexed metadata exception.
- An envelope signature is detached from the bytes it signs and is not hashed into that same
  envelope. Its expected publication path is explicitly admitted by release policy; verification
  validates its cryptographic relation to the envelope. Merely listing a signature file is not
  signature verification.

Canonical records must use an explicitly versioned domain, fixed UTF-8 serialization, ordered
unique path tuples and lowercase SHA-256. Identity binds version, commit/tree, target, lockfiles,
source epoch, material pins and build recipe/environment/tool identities. Host paths, incidental
mtime, locale, insertion order and random values cannot affect deterministic identity.

Archive entry order, normalized modes, uid/gid, timestamps, compression version/options, ZIP
headers, debug-path remapping and executable timestamp policy must be fixed in the release recipe.
Two clean builds of the same revision/target must match exact archive bytes by default. Any
platform-qualified equivalence requires prior review, an exact normalization algorithm and both
raw and normalized identities. A broad exclusion of binary differences is not reproducibility.
Detached signatures/attestations do not enter deterministic payload production. Embedded binary
signing, if selected later, requires a separately reviewed identity/reproduction rule.

### Archive admission limits

The implementation candidate uses these inclusive limits for both targets. The file ceiling
counts every regular file, including all Rust notices and the five metadata files. Directories
are derived only from admitted file ancestors; they cannot introduce independent contents.

| Resource | Inclusive ceiling |
| --- | --- |
| Regular files | 512 |
| Each CLI or Node executable | 256 MiB |
| Each provider file | 16 MiB |
| Each notice, document or metadata file | 2 MiB |
| Each canonical JSON record | 256 KiB, additionally subject to its file limit |
| Sum of regular-file bytes | 512 MiB |
| Complete archive bytes | 513 MiB |
| Portable relative path | 200 ASCII bytes and 12 segments |
| Canonical JSON nesting | 12 levels |

Canonical JSON uses recursively sorted object keys, compact UTF-8 encoding and one final LF.
Wire shapes are defined by the [prepared distribution](../schemas/zryna-distribution-v1.schema.json),
[materials](../schemas/zryna-distribution-materials-v1.schema.json) and
[inventory](../schemas/zryna-distribution-inventory-v1.schema.json) schemas. Shape validation alone
does not establish source, material, gate or release authority.
Numbers must be safe integers. Duplicate keys, noncanonical bytes, unknown fields, unsorted or
colliding paths, dangling license references and undeclared payload files fail admission.
Case collisions, traversal, Windows reserved names, links, reparse points, hard links and special
files are rejected. Only the CLI and Node files have mode `0755`; other files have mode `0644`.

Linux archives use ustar headers with uid/gid zero, empty owner names, the source epoch, zero
padding and two final zero blocks. Gzip uses level 9 and OS byte 255. Windows ZIP archives use
stored members, Unix creator attributes, midnight on 1980-01-01, and no extra fields or comments.
Both formats have one matching archive root and deterministic entry order. Admission decodes
within the budgets and compares a canonical re-encoding, including trailers, before extraction.

Production gzip bytes require the authenticated upstream Node 22.22.1 runtime with its exact
zlib build. The Linux material reports `1.3.1-e00f703`; a system Node with zlib `1.3.1` is not
interchangeable. The Windows runtime check, full compiler/linker recipe and two-build archive
comparison remain required acceptance evidence; these limits do not establish release readiness.

## Materials, host qualification and remaining acceptance

[Bootstrap material evidence](BETA_BOOTSTRAP_MATERIALS.md) records the current pins, extracted
package/license hashes, upstream platform floor and remaining provenance checks. It distinguishes
upstream requirements from Zryna clean-host evidence. The release version and supported baseline
must be frozen only after exact-build loader inspection and qualifying clean-host execution.

The observed user-local workflow, exact host prerequisites, PATH setup, first external project,
owned-file removal, recovery behavior, and current upgrade blocker are recorded in
[Downloadable beta installation and removal](BETA_INSTALLATION.md).

Required evidence includes independent same-tag builds; protected-tag dry runs; clean offline
archive installation without checkout or development tools; relocated version/project creation,
JavaScript/WebAssembly build/run; deterministic inventory and license/SBOM/provenance validation;
pre-execution tamper rejection; runtime/provider replacement and hostile loader tests; safe
extraction/collision/removal tests; and rollback/revocation rehearsal. Preserve conditional Linux
native evidence and expected Windows native rejection separately from portable install success.

The exact candidate must pass the contribution-required frozen install, preflight, complete
Linux/Windows M0 and all affected profile/provider/security gates, plus required hosted checks.
Receipts identify source/tree, platform, commands, exits and executed/ignored counts. Contract
review, a test listing or another revision's result is not executed acceptance evidence.

Each new release still requires approved protected-tag/environment configuration, least-privilege
signing/attestation authority and an independent verification policy under #406. The separate
#407 project, #422 distribution and #406 publication boundaries are implemented for the observed
v0.2.3 release; the [release run](https://github.com/zryna/zryna/actions/runs/34834478150) completed
all 15 jobs, including installed acceptance, reproduction and four public upgrade paths. Those
results do not qualify a later revision, the outer portable setup candidate, or #424's external
pilot. Publication and pilot completion require their own exact-candidate evidence.
