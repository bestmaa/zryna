# Zryna Developer Preview

Thin local editor integration for Zryna's verified scalar, M2 and M3 diagnostics and formatter.
Lexical syntax highlighting and explicit **Zryna: Run Saved File** are included. The default
profile is scalar `i32-v1`; select `control-flow-v1` for local `let`/`const`, direct calls,
`if`/`else` and `while`; select `data-ownership-v1` for admitted M3 source and saved imports.
Unsupported or incomplete input receives no edits. Explicit Run remains limited to scalar and M2.

## Installation

Zryna Developer Preview **0.5.0** is available on the
[VS Code Marketplace](https://marketplace.visualstudio.com/items?itemName=zryna.zryna) and
[Open VSX](https://open-vsx.org/extension/zryna/zryna). In VS Code, install the exact version:

```text
code --install-extension zryna.zryna@0.5.0
code --list-extensions --show-versions
```

The list must include `zryna.zryna@0.5.0`. Open VSX-compatible clients can install from the
linked listing or use **Install from VSIX** with its exact 0.5.0 download. The extension includes
no compiler or language server: configure the matching 0.5.0 server and runtime using the user
settings below, or use an independently verified complete portable setup candidate.

The portable **0.1.0-candidate.3** setup combines the unchanged compiler 0.2.3 with server/editor
0.5.0 and a pinned runtime. Follow the [portable setup guide](https://github.com/zryna/zryna/blob/main/docs/PORTABLE_SETUP.md).
Verify the reviewer-delivered archive identity before execution. This setup remains a review
candidate without a public beta/stable release. Its isolated installer creates a new profile with user
settings `zryna.installationPath` and `zryna.installationDigest`. The entire installation is checked
against that independently supplied digest before source transmission. Existing profiles and
projects remain untouched. No checkout, Rust or separately installed Node is needed.

For development only, explicit source configuration remains available:

Build the matching reviewed compiler source with Node.js 22.22.1, pnpm 11.18.0 and Rust 1.97.1:

```text
pnpm install --frozen-lockfile
pnpm preflight
pnpm m0:check
cargo build --locked -p zryna-language-server
pnpm editor:check
pnpm editor:package
code --install-extension /absolute/compiler/checkout/.zryna/out/zryna-0.5.0.vsix
```

Set these **user settings** to absolute paths:

- `zryna.serverPath`: the built `target/debug/zryna-language-server` executable (`.exe` on Windows).
- `zryna.compilerRoot`: the trusted matching compiler checkout with its pinned adapter installed.
- `zryna.nodePath`: the exact Node.js 22.22.1 executable.
- `zryna.compilerPath`: the separately installed official v0.2.3 `zryna` executable for Run.
  Run uses the installation's runtime and does not pass source-checkout or Node overrides.

Open the edited project's folder and a `.zry` file. Use **Format Document**, **Format Selection** or
**Go to Definition**. The extension serves one active document at a time; changing files creates a
fresh connection. Range formatting accepts complete functions only and never expands a selection.
Formatting has one canonical two-space/LF style. Comments and token spellings remain unchanged.

## Real extension-host acceptance

The opt-in Windows desktop acceptance harness launches VS Code 1.138.0 through
`@vscode/test-electron` with a dedicated workspace, user-data directory, extension directory,
and global storage outside normal VS Code profiles. It uses a separately verified complete portable
setup, not a source rebuild. After independently checking the candidate archive and `setup.json`
digest, establish trust once:

```powershell
pnpm editor:host-acceptance -- '<verified setup directory>' '<setup.json SHA-256>' --prepare-trust
```

In the first VS Code window, use **Manage Workspace Trust** to trust only the printed test
workspace. A small test extension runs the API checks in the real extension host, records the
result, and closes this isolated VS Code instance. Later runs use the same command without
`--prepare-trust`; an absent trust decision fails immediately with an instruction to prepare it.
The state path is stable for this checkout, under the system temp
directory as `zryna-vscode-host-state-<checkout hash>`. Its isolated VS Code profile and empty
workspace directory remain so VS Code can remember that exact trust decision. The test source,
compiler run projects, and per-run result files are removed after every run. Remove the dedicated
state directory to revoke this harness's saved trust and data. The pinned VS Code download has a
separate system-temp cache. Neither command disables Workspace Trust or edits a normal VS Code
profile. A local Code executable may be passed before `--prepare-trust` when testing an installed
version; this does not pin that version.

Only one run may use this retained state at a time. Another invocation fails immediately while
`run.lock` exists and leaves the active run untouched. If VS Code cannot be confirmed closed after
a timeout, the launcher retains the lock, source, and run files for inspection. Clear a stale lock
only after verifying that no acceptance VS Code process still uses the printed state directory.

```powershell
pnpm editor:host-acceptance -- '<verified setup directory>' '<setup.json SHA-256>'
```

The host checks activation, scalar, M2 and M3 profile selection, scalar definition, M2 diagnostics and
recovery, M3 saved-import formatting, format idempotence, real JavaScript and WebAssembly return values, and opening generated
JavaScript. It uses a validated command argument for Run so the interactive picker remains available
to users. The API tests do not prove status-bar appearance, rendered Problems layout, notification
presentation, or OS file explorer behavior; those require a separate visual review. The harness is
not part of `editor:check` or default CI.

On a local Windows run against the earlier 0.4.0 `cb60922` candidate, the one-time trusted setup completed in
23.0 seconds; two consecutive cached, unattended acceptance runs completed in 24.0 and 24.0
seconds. The first VS Code download in an earlier probe took 102 seconds including a failed trust
check, so a complete cold acceptance duration has not been measured. These are observations,
not duration guarantees or 0.5.0 host acceptance evidence. Separately, on 2 October 2026, clean
installations from the VS Code gallery and verified Open VSX 0.5.0 download each passed all nine
checks in the real Windows VS Code 1.138.0 host, with the installed extension path asserted and
only the acceptance runner loaded as a development extension. Real results were scalar
JavaScript 9, M2 JavaScript 13 and M2 WebAssembly -13. Both runs also passed diagnostic recovery
and M3 saved-import formatting/idempotence. See the [exact publication evidence](https://github.com/zryna/zryna/blob/main/docs/LANGUAGE_SERVER.md#publication-and-installed-host-evidence)
for source/setup identities and the separate prior Linux/Windows CI evidence. Linux graphical
extension-host acceptance remains unverified.

## Compatibility

| Extension | Editor | Server |
| --- | --- | --- |
| 0.5.0 | VS Code-compatible API >=1.82.0 | Server 0.5.0 with exact `scalar-format-v1`, `control-flow-format-v1` or `data-ownership-format-v1` capability for the selected profile, plus `portable-setup-v1`; installed mode checks exact source revision |
| 0.5.0 | Same | Public released v0.2.3 server and older 0.4.0 server are incompatible; the unchanged installed compiler 0.2.3 supports scalar/M2 Run |

The extension verifies the exact capability before sending source. A package-version match alone
is insufficient. The VSIX alone contains no compiler; the portable candidate supplies the matched
setup. The published package is available from both registries. Marketplace installation does
not authenticate or promote the outer portable candidate into a public beta or production release.
The immutable 0.5.0 VSIX contains the earlier pre-publication documentation; this current guide
and [release notes](https://github.com/zryna/zryna/blob/main/editors/vscode-zryna/CHANGELOG.md)
record completed publication.

## Boundaries

The extension runs only in trusted local-file workspaces. Executable paths come exclusively from
user settings; workspace overrides are ignored. It downloads nothing,
provides no debugging commands, transmits no source over a network, and has no telemetry or
runtime dependencies. Compiler diagnostics remain inert plain text. Global errors without an exact
source range appear in the status bar and **Zryna Diagnostics** output. Editor APIs apply explicitly
requested edits; the extension provides no general filesystem write service.

Default scalar formatting supports the protocol-v2 i32 function/return/reference/literal/addition
slice, and Go to Definition. Select **Zryna: Select Editor Profile** to use explicit
`control-flow-v1` diagnostics and formatting for the bounded local M2 syntax. M2 has no definition
index; Go to Definition is scalar-only. M2 formatting requires a semantically accepted single
file and caps the formatted result at 131,072 bytes. Select `data-ownership-v1` for M3 formatting
of the active buffer and saved imports below the trusted workspace folder. Unopened imports are
authenticated from disk; changes to admitted sources before edit publication reject edits.
Malformed/unavailable revisions, partial ranges, cancellation and stale documents produce no edits. See the
[language server and format contract](https://github.com/zryna/zryna/blob/main/docs/LANGUAGE_SERVER.md).

## Run and inspect output

Save the active `.zry` file, then choose **Zryna: Run Saved File** from the command palette or editor
title. Choose a profile, an exported function, JavaScript or WebAssembly, and each required typed
argument. The default scalar profile accepts i32; explicit M2 accepts i32 and bool.
There is no default export or argument. Cancel any picker to stop without starting the compiler.
The progress notification supports cancellation. Unsaved/closed/virtual files and untrusted
workspaces are rejected. Opening, editing, formatting and saving never execute project code.

Default Run supports the installed compiler's one-file `i32-v1` profile: explicit i32
parameters/result, integer literals, references and addition, with a **1024 UTF-8-byte** package
source limit. Explicit `control-flow-v1` Run supports the bounded one-file local M2 syntax above
and i32/bool parameters/results, with a **2 MiB** source limit. i32 arguments are canonical
decimal integers from -2147483648 through 2147483647; bool arguments are exact `true` or `false`.
The picker is lexical discovery,
not semantic validation; the compiler remains authoritative and may reject an offered function
or program. Imports, M3 ownership, classes and native Windows executables are unsupported.

Each invocation uses a fresh compiler-created project in extension global storage under `runs`.
It copies saved bytes into `src/main.zry`, updates the generated manifest size/SHA-256, and invokes
the compiler package resolver to refresh the authenticated lock. It never adds helper programs to
the package. Original source and existing projects remain untouched. Each direct compiler process
has a 120-second timeout and 1 MiB captured-output limit.

**Zryna Run** output shows the saved path/hash, invocation, compiler results/diagnostics and actual
artifact location. Results refer to that snapshot even if the editor changes during execution.
**Zryna: Open Generated JavaScript** opens the last successful JavaScript artifact.
**Zryna: Reveal Run Output** selects the actual `.mjs` or `.wasm` file in the OS file explorer.
Output is under `<run-project>/.zryna/out/result.run/javascript/result.mjs` or
`webassembly/result.wasm`; manifest invocation, source identity and artifact hashes are checked.
Run state is retained for inspection, including failed/cancelled scaffolds; the output channel
identifies the project. Remove old run directories manually when no run is active.

This explicit execution scope follows #457 and supersedes #409's earlier no-execution boundary
only for this command. The published extension's Run surface remains limited to scalar and M2.
