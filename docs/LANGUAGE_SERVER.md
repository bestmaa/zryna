# Language server protocol v1

Status: bounded stdio transport for protocol-v2 scalar diagnostics, definition, and formatting,
plus explicitly selected M2 `control-flow-v1` and M3 `data-ownership-v1` diagnostics and
formatting. The M2/M3 selections add no definition index or compiler execution method.
Zryna Developer Preview 0.5.0 is published on the
[VS Code Marketplace](https://marketplace.visualstudio.com/items?itemName=zryna.zryna) and
[Open VSX](https://open-vsx.org/extension/zryna/zryna). See the installation and publication
evidence below; extension publication does not promote the portable setup candidate.

## Start and initialize

Run `zryna-language-server --compiler-root <absolute-path> --node <absolute-path>`. The compiler
root must contain the registered TypeScript 6 adapter and `--node` must identify the exact pinned
Node.js 22.22.1 runtime. Both paths are configuration established before protocol input; LSP
messages cannot select a worker, process, provider, build target, network endpoint, or executable
action. For M3 saved imports, also pass `--workspace-root <absolute-path>` from the trusted local
workspace folder. The installed mode accepts the same option alongside `--installed-root`.
The server captures this no-follow root before reading protocol input; the M3 `rootUri` must match
it exactly. Protocol messages cannot select or expand that filesystem root.
Pre-protocol root/adapter configuration failures use `ZRYNA-D3001`; existing pinned-runtime
failures retain their `ZRYNA-R3xxx` codes. They are written as inert standard error text before
the server accepts protocol input.

The server uses LSP/JSON-RPC 2.0 over standard input/output. Each message has one required ASCII
`Content-Length` header, an optional exact UTF-8 `Content-Type` header, `\r\n` separators, and a
complete payload of at most 16 MiB. Duplicate, unknown, malformed, oversized, or truncated headers
terminate the connection. JSON-RPC objects reject malformed versions, IDs, method names, duplicate
fields, trailing values, and method-specific shape errors without publishing compiler results.

`initialize` requires one `file:` `rootUri` and client capabilities. The server selects the first
client-listed `utf-8`, `utf-16`, or `utf-32` position encoding and otherwise uses the LSP default
`utf-16`. Document URIs must be exact descendants of that root. Percent decoding is strict;
traversal, non-portable paths, case aliases, invalid UTF-8, queries/fragments, NUL, backslashes, and
source-limit violations fail closed through URI or `zryna-source` validation.

Without `initializationOptions`, the connection retains protocol-v2 scalar admission and
`scalar-format-v1`. Exact `initializationOptions: {"zrynaProfile":"control-flow-v1"}` selects
protocol-v3 M2 admission and `control-flow-format-v1`. Exact
`initializationOptions: {"zrynaProfile":"data-ownership-v1"}` selects protocol-v4 M3 admission
and `data-ownership-format-v1` under the separately captured workspace root. Unknown profile values or additional option
fields reject initialization before any source is admitted. The initialize response echoes the
selected `experimental.zrynaAnalysisProfile` and `experimental.zrynaFormattingProfile`. It also
advertises `portable-setup-v1` and, in installed mode, the exact embedded source commit. The
The matching editor extension checks these fields, position encoding, method capabilities, and installed source
identity before `didOpen` sends text.

## Supported methods

| Method | Contract |
| --- | --- |
| `initialize`, `initialized` | Negotiate one connection and its exact position encoding. |
| `textDocument/didOpen` | Admit one `zryna` full-text overlay with a nonnegative version; M3 accepts bounded additional open overlays. |
| `textDocument/didChange` | Require exactly one full-text replacement and a strictly increasing version. |
| `textDocument/didClose` | Remove the overlay, invalidate its revision, and clear published diagnostics. |
| `textDocument/definition` | Resolve scalar function/parameter identifiers through the semantics-owned index. Unavailable in M2 connections. |
| `textDocument/formatting` | Format a verified document under the selected profile. |
| `textDocument/rangeFormatting` | Format only complete verified top-level units inside the selection (functions for scalar/M2; imports, data declarations, and functions for M3). |
| `$/cancelRequest` | Cancel one admitted definition or formatting query by its exact JSON-RPC ID. |
| `shutdown`, `exit` | End the connection in order without executing project code. |

Scalar and M2 admit at most one open document per connection. M3 accepts a bounded open-document
set below the trusted root, with the first open document as its entrypoint until it closes. Full-text sync is
deliberate; incremental range edits are not advertised or accepted. Hover,
references, rename, completion, code actions, symbols/indexing, debugging, builds,
execution, M2/M3 definition, data-ownership queries, and workspace mutation are not
implemented. Unknown requests receive `Method not found`; unknown notifications have no effect.

## Revisions, diagnostics, and definitions

Every accepted open/change/close source set builds a new immutable `SourceMap` and driver-owned
session revision. Versions must advance even for same-length edits, edit-and-undo, or identical
replacement bytes. In-flight work retains the exact snapshot/revision/source authority. The driver
rechecks that authority immediately before publication; replacement, close, foreign URI, eviction,
deadline, cancellation, malformed input, or resource exhaustion cannot publish a partial or old
result. A stale definition receives LSP `ContentModified`; cancellation receives
`RequestCancelled`. Absent supported symbols return `null` only after the active semantic authority
reports `absent`.

Each ready revision emits two notifications:

- `zryna/publishDiagnostics` contains the opaque `snapshot`, monotonic `revision`, exact document
  URI/version list, and the complete structured-diagnostics-v2 report unchanged. This is the
  revision-bearing authority, including global/workspace locations and terminal `ZRYNA-D2001`.
- `textDocument/publishDiagnostics` contains only source-located diagnostics for each open document.
  It preserves code/severity/message, derives the exact negotiated range from the retained text,
  and retains original guidance/location in inert `data`. It never invents a document range for a
  global or workspace label.

M3 resolves imports through the driver's bounded fixed-point module closure. An open buffer shadows
its saved file at the exact normalized path; unopened imports are read under the retained no-follow
workspace root and revalidated with the final verified v4 source map. Before returning M3 edits,
the server rediscovers the graph and requires every reachable path and byte to match the admitted
revision. Changed or unsafe dependencies return `ZRYNA-D4002` with no edits. The server does not
execute workspace code or write source files.

Scalar definition requests use the active document and negotiated position to derive one exact UTF-8 byte
offset. Token ends, whitespace, comments, and EOF return `null`; invalid lines/columns, UTF-8 scalar
splits, UTF-16 surrogate splits, and non-round-trippable CRLF interiors reject. A successful result
contains the declaration URI and exact converted range issued by the retained semantic index.

## Limits and compatibility

The transport adds a 16 MiB frame limit and otherwise preserves the query-session ceilings:
65,536-byte logical requests, 1,048,576-byte responses, depth 64, 100,000 work units, 10,000 results,
two revisions, 64 MiB retained cache, 32 in-flight requests, 128-byte IDs, and a 30-second deadline.
Source/provider limits remain independently enforced. Cache hits do not reduce logical charges.

The component depends only on the driver orchestrator and source foundation. Driver-owned fixture
support authenticates a fixed in-memory syntax snapshot to prove nonzero cancellation and
stale-revision behavior at valid offsets; it does not move frontend execution into the transport.
Provider identity stays behind the driver boundary and is absent from LSP messages. Future field,
method, sync-mode, coordinate, diagnostic, or limit changes require a separately reviewed compatible
extension or protocol revision; they do not silently activate query forms specified but not yet
implemented in the semantic-query v1 design.

Focused verification is `cargo test --locked -p zryna-language-server`. It covers framing,
initialization, lifecycle, same-length revision races, cancellation/recovery, URI and coordinate
hostiles, unsupported methods, exact diagnostics, definition positions, and the real stdio process
on the supported CI operating systems. Complete repository gates remain required before merge.

## Scalar format v1

The scalar formatter supports only the existing one-file protocol-v2 scalar profile: exported
functions with explicit i32 parameters/results and the admitted return, reference, integer and
addition expressions. It consumes the exact verified snapshot only after successful scalar semantic
admission. Parenthesized expressions, classes, imports,
incomplete syntax and semantic errors receive no edits. The separately selected M2 formatter
handles its reviewed one-file control-flow surface. M3 uses the separate protocol-v4 connection.
The matching 0.5.0 editor provides these formatting profiles; its publication and installed-host
acceptance are recorded below for #409.

The canonical style uses two spaces inside function bodies, one space between words and around
addition, no space before commas/colons/semicolons or inside parameter parentheses, a space after
commas/colons, and LF after opening/closing braces and semicolons. Nonempty documents end with LF.
Token spellings/order, comment bytes (including newlines inside block comments), integer spelling
and declaration identity remain unchanged. The formatter never inserts/removes tokens, sorts
imports, evaluates code, or writes files. Line-comment terminators become LF; comments remain in
token order, although a trailing comment after a semicolon occupies the following line.
Formatting options are parsed for LSP compatibility; tabSize must be positive, but the canonical
style does not depend on editor indentation preferences.

Document formatting returns either one whole-document edit or an empty list for canonical text.
Range formatting requires exact negotiated coordinates and accepts complete function spans plus
surrounding trivia. It does not expand the selection: intersecting a partial function rejects the
whole request, and trivia-only/empty selections return no edits. Each returned edit covers only
one complete function, preserves text before/after it byte-for-byte, and omits a new trailing LF
outside that function. All edits derive from the retained active revision; cancellation, close,
replacement and expiry reject pending work. Applying the edits belongs to the editor.

The prepared document is capped at 131,072 UTF-8 bytes; unavailable/over-limit preparation cannot
produce edits. Its exact text, path and function-boundary bytes count against the existing session
cache. JSON results remain capped at 1 MiB and 10,000 edits; definition and formatting share the
32-request queue. No text is sent over a network.

| Stable code | Meaning |
| --- | --- |
| ZRYNA-D4001 | No admitted formatting state: invalid/unsupported syntax or semantics, unavailable diagnostics, or unsupported whitespace. |
| ZRYNA-D4002 | Requested revision is no longer active (LSP ContentModified). |
| ZRYNA-D4003 | Invalid coordinate, reversed selection or partial function. |
| ZRYNA-D4004 | Request/result limit exceeded. |

Failures carry the code in LSP error.data.code and never contain result edits. Existing compiler
diagnostics remain authoritative. Some incomplete provider reports cannot be represented by the
existing diagnostic-v2 contract; their analysis remains unavailable and formatting still fails
closed with D4001. Cancelled requests use LSP RequestCancelled; expired requests fail without edits.

## Control-flow format v1

Selecting `control-flow-v1` at initialization admits one authenticated protocol-v3 source file
through the compiler-owned M2 analysis path. The one-file local editor surface supports i32/bool
functions, `let`/`const`, assignment, `if`/`else`, `while`, direct calls, and admitted arithmetic
and comparisons. Imports, M3 ownership/data syntax, classes, and global variables are outside
this connection. The server does not infer M2 from source text or fall back to scalar analysis.
The selected M2 connection advertises `definitionProvider: false`; definition requests return
`Method not found` until a separately verified M2 definition index exists.

`control-flow-format-v1` consumes only a verified, semantically accepted M2 snapshot. It keeps
token spellings and comments in order, uses two-space nesting and LF, and returns one whole-document
edit or no edit when already canonical. Range formatting accepts complete function spans only;
each edit stays inside its selected function and preserves every byte outside. Partial function
selections reject with `ZRYNA-D4003`; empty or trivia-only selections return no edits. Invalid,
unsupported, or semantically rejected input returns `ZRYNA-D4001` with no edits. Stale revisions
return `ZRYNA-D4002`. Semantically accepted input whose canonical document exceeds 131,072
UTF-8 bytes returns `ZRYNA-D4004` with no edits; the exact cap is accepted. Cancellation and
source replacement cannot publish old edits.

## Data-ownership format v1

Selecting `data-ownership-v1` requires a trusted startup `--workspace-root` matching the
initialization `rootUri`. The driver captures the fixed protocol-v4 worker, discovers reachable
imports through the retained workspace source session, verifies the final syntax/source map, and
runs the M3 semantic lowerer before retaining formatting plans. Open documents shadow their saved
paths with exact versioned UTF-8 text. Missing, changed, unsafe, syntactically rejected, or
semantically rejected modules yield diagnostics and no edits. The client does no import parsing.

`data-ownership-format-v1` preserves every token and comment byte in source order and changes
whitespace to two-space nesting and LF. Document formatting returns one replacement for the
requested open file or no edit when canonical. Range formatting accepts complete imports, data
declarations, and functions; partial intersections reject the whole selection with D4003, while
bytes outside selected units remain unchanged. Each canonical document is bounded to 131,072
UTF-8 bytes. Formatting never sorts imports, executes code, or writes a file. A saved import that
changes after admission makes the formatting revision stale and returns D4002 without edits.

## Editor installation and compatibility

The VS Code/Open VSX package lives in editors/vscode-zryna. Zryna Developer Preview 0.5.0 is
available from both registries as `zryna.zryna`. It provides diagnostics, scalar definition,
document and range formatting for one active local file at a time. Switching files starts a fresh bounded
connection. Explicit M3 formatting resolves saved imports through the trusted project root;
the client does not parse imports. It has no runtime package dependencies, telemetry,
download/update behavior, debugging or general filesystem write service. A separate explicit
editor Run command is described below; it does not add execution to the language-server protocol.
Diagnostic messages render as plain text, and edits/definitions are validated against the same
document and version before returning them to VS Code.

| Extension | Editor engine | Required compiler | Source profile |
| --- | --- | --- | --- |
| 0.5.0 | VS Code-compatible API >=1.82.0 | Server 0.5.0 advertising scalar-v2, scalar-format-v1, and portable-setup-v1 | One-file scalar; definition available |
| 0.5.0 | Same | Server 0.5.0 advertising control-flow-v1, control-flow-format-v1, and portable-setup-v1 | One-file M2; definition unavailable |
| 0.5.0 | Same | Server 0.5.0 advertising data-ownership-v1, data-ownership-format-v1, and portable-setup-v1 | M3 saved imports under the trusted workspace root; definition unavailable |
| 0.5.0 | Same | Server 0.4.0 or public immutable v0.2.3 server | Incompatible |

The package version alone is insufficient: the extension verifies server name/version, UTF-16
positions, both formatting methods, the exact selected analysis/formatting capability and installed
source revision before sending document contents. The editor profile defaults to `i32-v1` for
scalar definition compatibility. **Zryna: Select Editor Profile** offers `i32-v1`,
`control-flow-v1`, and `data-ownership-v1`; it stores the choice per workspace and reconnects
before admitting the active document. The explicit Run picker accepts scalar and M2 only. Neither
selection nor formatting executes source. No new compiler release or tag is created by this work.

Install the published extension in VS Code:

~~~text
code --install-extension zryna.zryna@0.5.0
code --list-extensions --show-versions
~~~

The extension list must include `zryna.zryna@0.5.0`. In an Open VSX-compatible client, select
the [Zryna listing](https://open-vsx.org/extension/zryna/zryna), or obtain the exact 0.5.0 VSIX
from that listing and use **Install from VSIX**. The extension contains no compiler or server.
Configure the matching server and runtime using the user settings below, or use an independently
verified [portable setup candidate](PORTABLE_SETUP.md). Registry installation alone does not
establish a working compiler connection.

For source development, use the matching reviewed checkout and pinned toolchains:

~~~text
pnpm install --frozen-lockfile
pnpm preflight
pnpm m0:check
cargo build --locked -p zryna-language-server
pnpm editor:check
pnpm editor:package
code --install-extension /absolute/compiler/checkout/.zryna/out/zryna-0.5.0.vsix
~~~

Set zryna.serverPath, zryna.compilerRoot and zryna.nodePath in USER settings to absolute paths.
The first names the built target/debug/zryna-language-server executable (.exe on Windows); the
second names that trusted compiler checkout with its pinned adapter dependencies, and the third
names the exact Node.js 22.22.1 executable. Workspace-provided overrides are ignored. The extension
is disabled in untrusted or virtual workspaces. Open the edited project's folder and a .zry file,
then select an editor profile and use Format Document/Format Selection. Go to Definition is
available in scalar mode. This does
not add a zryna fmt CLI command or any compiler execution flag.

Packaging uses pinned @vscode/vsce 4.0.0 without dependencies or signing. Its optional signing
executable installer is explicitly disabled; the VSIX contains only its manifest, client/Run
modules, lexical grammar, README, changelog and license. Publication requires reviewed exact-package provenance,
a configured marketplace publisher/namespace and its credentials. None are provisioned or embedded
by this package. See the [package changelog](../editors/vscode-zryna/CHANGELOG.md) for release notes.

### Publication and installed-host evidence

On 2 October 2026, the reviewed 0.5.0 VSIX was published under the existing `zryna` publisher
and verified Open VSX namespace. The
[exact Open VSX metadata](https://open-vsx.org/api/zryna/zryna/0.5.0) reports version 0.5.0,
publication by `bestmaa` in the verified namespace, and a downloadable package. Its downloaded
VSIX has SHA-256
`cc7ba5e53083adfd35e1398ce9ab009587ff4e90c4b8c479f02f34cae5245254`, equal to the reviewed package.

Two new isolated Windows profiles installed 0.5.0 successfully: one directly from the VS Code
gallery, and one from the verified Open VSX download. Both installation commands exited 0 and
listed `zryna.zryna@0.5.0`. The gallery's 12 installed extension payload files match the reviewed
VSIX; its package manifest differs only by registry installation metadata.

Each installed extension separately passed all nine real-host checks in VS Code 1.138.0:
explicit workspace trust, activation and verified installation, scalar definition, scalar
JavaScript result 9, M2 formatting/idempotence, M2 JavaScript result 13 and output opening, M2
WebAssembly result -13, invalid-to-valid/stale diagnostic recovery, and M3 saved-import
formatting/idempotence. Only the acceptance runner was loaded as a development extension; each
Zryna extension's installed path was asserted. The tests reused the previously trusted isolated
workspace and did not change a normal editor profile.

The acceptance tests came from `2f2f64e723924cd329a987e7ac9833149fd0811b`. The matching reviewed
setup retained source `bf8706ab4a8a606ec99d816e8df90d1c4c1f0f31` and independently checked
`setup.json` SHA-256 `b0713306edc989c83c72c8bb5044fc192f88ee8649db168b7ff6e30038701f0a`.
Earlier #500 checks on exact source `3fadcbd3d0112dd8cd79bfaa10bd334a18594acc` passed all 33
hosted checks, including Linux/Windows reproducible setup and fresh portable acceptance. Those
checks are separate from the Windows installed-marketplace host observations; no Linux graphical
extension-host acceptance is claimed.

No compiler/server rebuild, new compiler tag, or portable beta/stable release accompanied this
publication. Setup `0.1.0-candidate.3` remains a review candidate with production admission
forbidden. The immutable 0.5.0 VSIX retains its pre-publication README/changelog; this current
guide and repository release notes supersede their publication-pending statements.

## Explicit editor Run

The [portable setup candidate](PORTABLE_SETUP.md) adds a standalone installation path without
changing Run's source/target limits. User settings select an installation and independently
supplied manifest digest; complete file verification precedes server startup. The server's exact
source revision must match the manifest before the client transmits document contents. Installed
mode uses fixed authenticated Node/provider bytes from the unchanged compiler 0.2.3 distribution.
The outer candidate is not authenticated by that compiler's signature and is not a public release.

Issue #457 adds lexical highlighting and an explicit Zryna: Run Saved File command. This newly
authorized command supersedes #409's original no-project-execution scope only for a user-triggered
run in a trusted local workspace. Opening/saving, diagnostics, definition and formatting never
execute project code. The language-server protocol and its execution exclusions remain unchanged.

Set zryna.compilerPath in user settings to the absolute official installed v0.2.3 compiler
executable, separately from the source-built formatting server. Workspace overrides are ignored.
Run first prompts for `i32-v1` or `control-flow-v1`, then one exported function, JavaScript or
WebAssembly, and each required input. M1 accepts canonical signed i32 values; M2 also accepts
exact `true`/`false` bool inputs and results. There are no hardcoded invocation values. After a
complete selection, Run switches the active editor connection to the selected profile and checks
the saved source again before compiling. Cancellation during selection starts no process. Lexical
discovery is advisory; compiler admission remains authoritative. The one-file source limit is 1024
UTF-8 bytes for `i32-v1` and 2 MiB for `control-flow-v1`. Imported modules, M3, classes, global
variables, and native Windows executables are outside this command's scope.

Unsaved input is rejected, and source changes during selection require a fresh invocation. Each
saved snapshot goes to a new compiler-created project in extension global storage. The extension
updates only its generated source inventory size/hash, invokes package resolve in update mode,
then invokes the installed compiler directly with an argument array and no shell or root/runtime
override. It does not bypass package authentication or insert executable helpers. Picker
cancellation starts no processes; progress cancellation and bounded subprocess deadlines stop
waiting. Run directories are retained for inspection and manual cleanup. No source files are
rewritten. The output channel identifies the snapshot and reports actual compiler results/errors.

Open Generated JavaScript and Reveal Run Output use the latest successful invocation's actual
artifact after checking the manifest source/invocation and emitted bytes/hash. JavaScript opens
as source; Wasm is revealed in the OS file explorer. This is an editor package update, with no
new compiler release or tag. Extension 0.5.0 publication and installed-host acceptance are
recorded above; explicit Run remains limited to scalar and M2.
