# Zryna CLI reference

## Public M3 profile

Exact `--profile data-ownership-v1` on `build` and `run` selects the authenticated protocol-v4
DataOwnershipV1 driver and canonical `zryna-manifest-v3.json`. Both `--profile value` and
`--profile=value` spellings work. Omission still selects M1; `control-flow-v1` still selects M2.
Run arguments accept canonical `i32:<decimal>` and `bool:true`/`bool:false` values. Only scalar
entry exports are public. JSON keeps response version 1 and reports the manifest-v3 portable
path, authenticated warnings and ordered typed results. A complete typed trap is an `ok: true`
run observation with exit 0; inspect `results[].outcome.kind`, not just process exit status.
Invalid source and unsupported features fail without a bundle. Create-only publication never
replaces an existing destination. See [public boundaries](M3_PUBLIC_PROFILE.md),
[manifest v3](M3_CANDIDATE_DRIVER.md), and [executable beginner examples](M3_GETTING_STARTED.md).


Status: implemented for the default M1 `I32V1` slice and the explicit M2 `ControlFlowV1` slice.
The CLI is a thin request parser and renderer over `zryna-driver`; it does not own compiler
semantics, module resolution, backend behavior, or bundle publication.

The exact public M3 selector uses the shared conformant driver; its authenticated transaction
and manifest v3 are documented in [M3 driver](M3_CANDIDATE_DRIVER.md).

## Commands

```text
zryna architecture check [--root <PATH>] [--json]
zryna doctor             [--root <PATH>] [--json]
zryna package resolve <PACKAGE> --source-root <PATH> --mode <frozen|update> [--git-cache <PATH>] [--json]
zryna build <ENTRYPOINT> --target <javascript|webassembly|native|component|all> --node <PATH> [--profile <control-flow-v1|data-ownership-v1|browser-component-v1>] [--root <PATH>] [--name <STEM>] [--json]
zryna run   <ENTRYPOINT> --target <javascript|webassembly|native|all> --export <NAME> --node <PATH> [--profile control-flow-v1] [--arg=<i32|bool>:<VALUE> ...] [--root <PATH>] [--name <STEM>] [--json]
```

`architecture check` and `doctor` run the same mandatory fail-closed workspace gate. Every
`build` and `run` performs that gate before reading source, creating output, discovering a target
runtime, or doing target work. There is no bypass flag.
Architecture and doctor `--json` output retains the existing deterministic `ValidationReport`
shape containing its `diagnostics` array.

`package resolve` is the source-only package command documented in
[Package resolution](PACKAGE_RESOLUTION.md). It does not compile source or select a backend.

`ENTRYPOINT` is exactly one workspace-relative `.zry` path. Absolute paths, traversal,
normalized-away components, backslashes, links or Windows reparse points, and paths outside
`--root` are rejected. Without `--profile`, only that one file enters the unchanged M1 path.
`--profile control-flow-v1` instead selects the M2 path and makes `ENTRYPOINT` the root of one
bounded, deterministic graph of explicit relative `.zry` imports. The driver discovers that graph,
authenticates one final source map, and performs semantic lowering exactly once before target
dispatch. The frontend never resolves imports or reads the workspace.

`--profile` has no hidden default: omission means M1. Exact lowercase `control-flow-v1` selects
M2; `data-ownership-v1` selects M3; `browser-component-v1` selects the default M1 scalar compiler
with an explicit browser bundle.
`--target` is mandatory, exact, lowercase, and has no alias or default. `component` is build-only;
the browser profile requires it. Other explicit profiles with `component` reject before source or
target work. The repository-local browser route and component emission are outside the advertised
[v0.1.0 preview support matrix](DEVELOPER_PREVIEW.md).
`--root` defaults to the current directory; the driver requires its resolved
workspace root to be an absolute real directory. `--name` defaults to the entrypoint stem and must
be 1 to 128 ASCII letters, digits, underscores, or hyphens, begin with a letter or underscore, and
avoid Windows-reserved device names. `--node` is mandatory and supplies an absolute path to a real
regular executable, with no symbolic link or Windows reparse point, for the validated exact
Node.js 22.22.1 runtime used by the authenticated TypeScript frontend and JavaScript or WebAssembly
execution. A native-only build still needs Node for the frontend; it does not use Node as its
target runtime.

`run` requires one exact logical export. Each repeated `--arg` is a typed scalar. M1 accepts only
`i32:<VALUE>`. `control-flow-v1` accepts that grammar plus exact lowercase `bool:true` and
`bool:false`. Negative shell arguments are most robust as `--arg=i32:-1`. The decimal spelling is
canonical: whitespace, a plus sign, leading zeroes other than `0`, numeric prefixes, fractions,
exponents, negative zero, and values outside `-2147483648` through `2147483647` are rejected.
Arity and types are checked through the program's sealed scalar ABI before target execution.
Boolean source and invocation remain rejected when `--profile` is omitted.

## Targets

| Selection | `build` artifact | `run` artifact and execution | Supported host |
| --- | --- | --- | --- |
| `javascript` | deterministic `.mjs` | sealed module through Node.js | Linux, Windows |
| `webassembly` | validated import-free `.wasm` | direct standard WebAssembly API through Node.js | Linux, Windows |
| `native` | audited Linux x86-64 `.o` | invocation-specific audited `.elf` | Linux x86-64 for run |
| `component` | audited import-free Component Model `.wasm`; explicit browser profile also emits ESM loader and declarations | unsupported | Linux, Windows build; pinned Chrome for browser execution |
| `all` | `.mjs`, `.wasm`, `.o` | `.mjs`, `.wasm`, `.elf` | Linux x86-64 for run |

`build native` emits a relocatable object and does not invent `main`. `run native` generates the
sealed scalar-ABI harness for the requested invocation. `all` analyzes source exactly once,
constructs one `VerifiedProgram`, and dispatches that authority in the fixed order JavaScript,
WebAssembly, native. All run targets receive the same verified invocation and report ordered typed
observations. The repository's [M1 conformance suite](M1_CONFORMANCE.md) compares those public
observations with fixed expected values and the committed manifest without creating a second
runtime semantics authority. This composition rule applies independently to M1 and
`control-flow-v1`; it never converts one profile's verified authority into the other. The
[M2 conformance gate](M2_CONFORMANCE.md) independently compares the explicit profile against a
fixed oracle on every supported platform. Issue #57 records authenticated website import,
deployment, and live commit/digest evidence separately from the CLI contract.

`component` preserves the unchanged audited M1 core module and canonically lifts its verified
scalar exports. It binds the exact authenticated `zryna:capability-profiles/browser@0.1.0` source
identity, whose capability-world import and export sets are empty. Without an explicit profile it
remains artifact emission only. `browser-component-v1` adds a generated browser binding that
hashes the exact component and invokes its retained import-free core scalar functions. See the
[browser quickstart and interface contract](BROWSER_COMPONENT_V1.md). Neither path joins `all`.
Artifact emission does not claim Component Model support for the v0.1.0 preview release.

## Output bundles

The public output root is `<root>/.zryna/out`:

```text
.zryna/out/<stem>.build/
  zryna-manifest-v1.json
  javascript/<stem>.mjs
  webassembly/<stem>.wasm
  native/<stem>.o
  component/<stem>.wasm

.zryna/out/<stem>.run/
  zryna-manifest-v1.json
  javascript/<stem>.mjs
  webassembly/<stem>.wasm
  native/<stem>.elf
```

An explicit `--profile control-flow-v1` request uses the same bundle names and selected target
subdirectories, but contains `zryna-manifest-v2.json` instead. A bundle contains exactly one
manifest version. Consequently an existing `<stem>.build` or `<stem>.run` bundle collides
create-only regardless of profile; selecting M2 never replaces an M1 bundle.

An explicit `--profile browser-component-v1` build contains exactly
`component/<stem>.wasm`, `component/<stem>.mjs`, `component/<stem>.d.mts`, and
`zryna-browser-manifest-v1.json`. It is also create-only and collides with a same-stem M1/M2
build. The browser manifest retains the M1 source and artifact fields plus a `browser` record
with binding revision, WIT world/source digest, component/core/interface digests, and the exact
retained-core offset. Each of the three files has its own ordered artifact record and hash.

The tree above shows every possible M1 build subdirectory; only selected target paths exist. A
default `component` build therefore contains only `component/<stem>.wasm` and
`zryna-manifest-v1.json`.

Only selected target paths exist. Build and run bundles with the same stem may coexist. A second
command of the same kind is create-only and fails without changing the existing bundle. The Linux
native run artifact is mode `0755`; Windows native run is unsupported.

The driver writes and synchronizes every selected artifact, result, and manifest inside one new
sibling transaction directory. On Unix it sets the transaction and target directories to mode
`0700`. On Windows those directories inherit ACLs from the validated compiler-owned output root,
so the workspace and `.zryna/out` must already be private to the invoking principal. The driver
revalidates containment and commits the complete bundle with one create-only, same-filesystem
directory rename to the absent final path. A failure leaves no advertised bundle. It never replaces
or modifies a pre-existing path.

The successful rename is the commit point; directory-entry crash durability after that point is
not claimed.

`zryna-manifest-v1.json` is deterministic UTF-8 JSON with `version` equal to `1` and `profile`
equal to `zryna-m1-cli-v1`. Its top-level fields are `version`, `profile`, `command`, `entrypoint`,
`source_sha256`, `stem`, `targets`, `artifacts`, `invocation`, `results`, and `diagnostics`.
`source_sha256` is the lowercase SHA-256 digest of the exact source bytes. Targets, artifacts,
results, and diagnostics are in stable order. Each artifact records its `target`, `kind`, bundle-
relative `/`-separated `path`, `bytes`, and lowercase `sha256`. Artifact kinds are
`ecmascript-module`, `core-webassembly-module`, `linux-x86-64-relocatable-object`, and
`linux-x86-64-invocation-executable` as applicable. A component build uses target `component` and
artifact kind `webassembly-component`. Its public function labels are
`zryna-export-<lowercase-hex-of-logical-name-bytes>`; this collision-free mapping accommodates
every scalar ABI logical name without changing existing target names. `invocation` is `null` for `build`; for `run`
it records `export` plus ordered arguments as `{ "type": "i32", "value": n }`. `results` is empty
for build and records each run target and its typed `outcome` in target order. The manifest contains
no absolute or temporary path, timestamp, process id, inherited environment value, credential, or
raw external-tool output.

`zryna-manifest-v2.json` is a separate deterministic UTF-8 JSON contract. Its `version` is `2` and
its `profile` is `zryna-control-flow-v1`. It records the canonical entrypoint; the module graph
digest; path-ordered source records with dense module IDs, portable paths, and lowercase SHA-256
hashes; canonical named-binding module edges; the stem; ordered targets and artifact records; the
optional typed invocation; ordered typed outcomes; and stable diagnostics. The graph digest is the
driver-owned `ZRYNA-M2-GRAPH\0` version-1 identity authenticated during module closure discovery;
the manifest does not ask a backend or renderer to recompute graph authority. Boolean arguments
and returned values use explicit typed records, never truthiness or an untyped numeric lane.

Manifest v2 uses only portable `/`-separated bundle and source paths and inherits the manifest-v1
ban on host-specific paths, temporary names, timestamps, process IDs, environment values,
credentials, and raw tool output. Sources, edges, targets, artifacts, results, and diagnostics have
one specified stable order. Selected artifact bytes and the manifest are create-new, synchronized,
and checked as one exact staged inventory before the single directory-rename commit point. Any
discovery, semantic, backend, execution, staging, manifest, cleanup, or commit failure reports no
final manifest path and exposes no partial final bundle. Ordinary failure cleanup is bounded;
process termination or machine failure may leave an unadvertised private transaction directory.

## Output and exit status

Without `--json`, a successful build prints the committed manifest path. A successful run prints
one ordered `<target>: i32 <value>` observation per selected target. Non-fatal diagnostics go to
standard error and use their stable Zryna codes. With `--json`, every parsed build or run command
emits exactly one versioned JSON document containing `version`, `ok`, `command`, the portable
workspace-relative committed `manifest` path (or `null` on failure), ordered `results`, and
`diagnostics`. Machine
consumers must use its structured fields rather than parse text output.

| Status | Meaning |
| ---: | --- |
| `0` | complete success and bundle commit |
| `1` | workspace or mandatory architecture failure |
| `2` | CLI syntax or request validation failure |
| `3` | frontend, source, semantic, scalar ABI, or IR rejection |
| `4` | target preparation, tool, audit, transaction, or commit failure |
| `5` | execution or result-frame failure |
| `6` | cleanup could not be confirmed |

Stable diagnostic codes are the precise failure identity. Clap parse errors retain status `2`.
No failure status advertises a partial bundle.

## Runtime and process limits

The JavaScript and WebAssembly target host validates that `node --version` exits successfully,
writes exactly `v22.22.1\n` to standard output, and writes nothing to standard error. The runtime
path is inspected before and after discovery and again before execution. Zryna passes literal
argument vectors and never invokes a shell.

Each Node probe or target execution has a five-second hard deadline followed by a separate bounded
five-second cleanup deadline. Standard input is closed. A run result is exactly four
standard-output bytes and standard error must be empty; version output is capped at 64 bytes and
target standard error at 16 KiB. Unix runs in a new process group with a cleared environment
containing only `LANG=C`, `LC_ALL=C`, and `TZ=UTC`. Windows uses a kill-on-close job with a cleared
environment retaining only the required `SystemRoot` and `WINDIR` values. Windows requires the
job-wide termination request to succeed and the leader to be reaped before the deadline. Timeout,
overflow, abnormal exit, or malformed result framing fails without a bundle; inability to complete
the documented cleanup steps uses exit status `6`.

Native discovery, linking, and execution use the separate hard caps and controlled environment in
the [native executable contract](../spec/native-semantics/EXECUTABLE.md). Generated artifacts are
not a security sandbox, and the dynamically linked native executable requires the validated host's
CRT, libc, and loader.

M2 JavaScript and core WebAssembly build and run remain portable across the required Linux and
Windows checks. Native object emission targets Linux x86-64. Native run, and therefore `all` run,
requires a Linux x86-64 host; Windows rejects it before native staging or bundle publication with
`ZRYNA-N4002`. There is no fallback to another target and no partial JavaScript/WebAssembly bundle.

## Examples

From the workspace root, using an exact Node.js 22.22.1 executable:

```bash
cargo run --locked -p zryna -- build examples/universal/add.zry --target javascript --name add-js --node /absolute/path/to/node
cargo run --locked -p zryna -- build examples/universal/add.zry --target webassembly --name add-wasm --node /absolute/path/to/node
cargo run --locked -p zryna -- build examples/universal/add.zry --target native --name add-native --node /absolute/path/to/node
cargo run --locked -p zryna -- build examples/universal/add.zry --target component --name add-component --node /absolute/path/to/node
cargo run --locked -p zryna -- build examples/universal/add.zry --profile browser-component-v1 --target component --name add-browser --node /absolute/path/to/node
cargo run --locked -p zryna -- build examples/universal/add.zry --target all --name add-all --node /absolute/path/to/node

cargo run --locked -p zryna -- run examples/universal/add.zry --target javascript --name add-js --export add --arg=i32:20 --arg=i32:22 --node /absolute/path/to/node
cargo run --locked -p zryna -- run examples/universal/add.zry --target webassembly --name add-wasm --export add --arg=i32:20 --arg=i32:22 --node /absolute/path/to/node
cargo run --locked -p zryna -- run examples/universal/add.zry --target native --name add-native --export add --arg=i32:20 --arg=i32:22 --node /absolute/path/to/node
cargo run --locked -p zryna -- run examples/universal/add.zry --target all --name add-all --export add --arg=i32:2147483647 --arg=i32:1 --node /absolute/path/to/node --json

# For a workspace module graph rooted at src/main.zry:
cargo run --locked -p zryna -- build src/main.zry --profile control-flow-v1 --target all --name app-m2 --node /absolute/path/to/node
cargo run --locked -p zryna -- run src/main.zry --profile control-flow-v1 --target javascript --name app-m2-js --export choose --arg=bool:true --arg=i32:42 --node /absolute/path/to/node
```

On Windows, pass the absolute direct executable path, for example
`--node C:\Tools\node-v22.22.1\node.exe`; a PATH shim, symbolic link, or reparse point is rejected.

The M1 `all` invocation reports three ordered `i32` observations with value `-2147483648`; the checked
M1 differential suite requires those observations and the manifest to agree. Package resolution,
non-relative/package imports, watch mode, incremental or remote builds, general browser APIs, WASI,
Windows or macOS native execution, static native executables, overwrite behavior, and
runtime-enforced comparison inside an ordinary end-user command remain outside the current slice.
The repository-owned [M2 conformance gate](M2_CONFORMANCE.md) performs fixed-oracle three-target
comparison. M2 closure evidence includes authenticated compiler documentation plus website and live
provenance recorded by Issue #57; those publication checks do not change command semantics.
M1 closure evidence includes website publication of versioned status and reference data from the
authenticated compiler documentation bundle tracked in Issue #21.

## Editor formatting

Formatting is exposed through the separately built language server.
See [the scalar formatter and editor installation contract](LANGUAGE_SERVER.md#scalar-format-v1)
for exact source-build compatibility, supported syntax, range behavior and commands.
