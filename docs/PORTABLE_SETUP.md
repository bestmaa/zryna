# Portable setup candidate

This is **setup 0.1.0-candidate.3**, an internal review and user-acceptance candidate, not a
published beta or stable release. It combines the unchanged, signed compiler **0.2.3 Developer
Preview**, language server **0.5.0**, editor **0.5.0**, Node **22.22.1** and TypeScript **6.0.3**.
Those component versions intentionally differ. `setup.json` binds the exact component bytes and
source revision. The editor requires `scalar-format-v1` for its default scalar profile or
`control-flow-format-v1` for explicit M2, plus `portable-setup-v1` and the exact server source
revision. An older released server with the same compiler version is incompatible.

Editor extension 0.5.0 is separately published on the
[VS Code Marketplace](https://marketplace.visualstudio.com/items?itemName=zryna.zryna) and
[Open VSX](https://open-vsx.org/extension/zryna/zryna). Registry installation does not supply
or authenticate this outer setup. Its review-candidate status and forbidden production admission
remain unchanged; see the [editor publication evidence](LANGUAGE_SERVER.md#publication-and-installed-host-evidence).

The explicit M3 editor profile requires the exact data-ownership formatting capability. It
authenticates saved imports beneath the trusted workspace folder and never runs project code
during formatting. Explicit Run remains limited to scalar and M2.

The supported packaging targets are Windows x64 (Windows Server 2022 baseline) and Ubuntu 24.04
x64. Windows desktop and WSL observations are additional local smoke evidence, not independent
clean-machine proof. VS Code-compatible desktop software with API 1.82 or newer is a separate
prerequisite. Linux graphical editor use also needs a desktop session. Other architectures,
macOS, remote/virtual workspaces and native Windows program output are unsupported.

## Verify before running anything

Obtain the candidate archive, its SHA-256, and the SHA-256 of `setup.json` through the reviewed
candidate handoff. Check the exact archive hash **before extraction or execution**:

```powershell
Get-FileHash -Algorithm SHA256 -LiteralPath .\zryna-setup-0.1.0-candidate.3-x86_64-pc-windows-msvc.zip
```

```sh
sha256sum zryna-setup-0.1.0-candidate.3-x86_64-unknown-linux-gnu.tar.gz
```

A checksum found only beside an untrusted download is not publisher authentication. This
candidate has no public publisher signature: use only the exact reviewer-delivered archive
identity. The builder independently verifies the nested compiler's signed release and records its
archive identity and provenance. That signature does **not** authenticate the new outer setup,
server or VSIX. Public distribution of the outer setup needs separately reviewed protected publication
and signed outer provenance; no existing tag or release is replaced.

Extract into a new user-owned directory, keeping all files together. Do not merge files into an
old installation. Projects and editor profiles must live outside this directory. Neither Rust,
pnpm, a source checkout nor a separately installed Node is needed for the packaged JavaScript,
core WebAssembly, scalar or bounded M2 editor capabilities.

Use full absolute paths with ordinary directories, without symlinks, junctions or Windows 8.3
short-name aliases. The verifier rejects noncanonical paths rather than following redirects.

## Windows installation

In PowerShell, select the extracted root and the reviewed manifest digest:

```powershell
$setup = 'C:\Zryna\zryna-setup-0.1.0-candidate.3-x86_64-pc-windows-msvc'
$digest = '<reviewed setup.json SHA256>'
& "$setup\install.cmd" --digest $digest
& "$setup\install.cmd" --digest $digest --editor "$env:LOCALAPPDATA\Programs\Microsoft VS Code\Code.exe" --profile "$env:USERPROFILE\ZrynaCandidateProfile"
```

The profile must be a new directory with an existing parent. The installer never edits an existing
profile, workspace settings or source. To open a project later, start Code with both paths:

```powershell
& "$env:LOCALAPPDATA\Programs\Microsoft VS Code\Code.exe" --user-data-dir "$env:USERPROFILE\ZrynaCandidateProfile\data" --extensions-dir "$env:USERPROFILE\ZrynaCandidateProfile\extensions" 'C:\Projects\ZrynaPractice'
```

## Linux installation

```sh
setup="$HOME/zryna-setup-0.1.0-candidate.3-x86_64-unknown-linux-gnu"
digest='<reviewed setup.json SHA256>'
"$setup/install.sh" --digest "$digest"
"$setup/install.sh" --digest "$digest" --editor /usr/bin/code --profile "$HOME/ZrynaCandidateProfile"
/usr/bin/code --user-data-dir "$HOME/ZrynaCandidateProfile/data" --extensions-dir "$HOME/ZrynaCandidateProfile/extensions" "$HOME/projects/ZrynaPractice"
```

Keep normal workspace trust enabled. Grant trust only to the specific project you intend to use.
The extension does not download tools, run on save, auto-save, or execute arbitrary shell commands.

## Editor exercise

1. Create an empty practice folder outside the setup and copy `examples/main.zry` into it.
2. Open that folder in the isolated profile. Open `main.zry`, run **Format Document**, then save.
   The two functions acquire two-space indentation; running Format Document again makes no edit.
3. Select **Zryna: Run Saved File**, choose `add`, JavaScript, and arguments `13`, `-4`.
   The Zryna Run output should contain `javascript: i32 9`.
4. Run `double`, WebAssembly, argument `13`; expect `webassembly: i32 26`.
   Use **Zryna: Reveal Run Output**. After a JavaScript run, **Zryna: Open Generated JavaScript**
   opens the emitted module. Outputs live in the isolated editor profile's extension storage.
5. Temporarily replace `x+y` in `add` with `x+missing`. Expect a source diagnostic. Formatting
   invalid source makes no edit. Undo that edit; unsaved source is never executed by Run.
6. Copy `examples/control-flow.zry` into the same practice folder. Open it and select
   **Zryna: Select Editor Profile**, then **M2 control flow**. Format the complete file twice;
   the second request should make no edit. Run `accumulate` with JavaScript, then enter `true`
   and `5`; expect `javascript: i32 10`. Run again with WebAssembly, enter `false` and `3`;
   expect `webassembly: i32 -6`. These runs require explicit profile, target, export and inputs.
7. Copy `examples/m3/main.zry` and `examples/m3/math.zry` into a new `m3` directory beneath
   that trusted folder. Open `main.zry`, select **M3 data ownership**, then Format Document.
   The import is read from the saved `math.zry` file; a second format request makes no edit.
   This profile provides formatting and diagnostics, not M3 Run.

The default editor profile retains the one-file `i32-v1` Run subset and its 1,024 UTF-8-byte
package limit. Explicit `control-flow-v1` enables bounded local `let`/`const`, direct calls,
`if`/`else`, `while`, and exact i32/bool inputs for saved-source Run, capped at 2 MiB of source.
The explicit `data-ownership-v1` editor profile formats admitted M3 source with saved imports
below the trusted workspace folder. The edited entry may be unsaved; the server authenticates
unopened imports from disk and rejects edits if they change before publication. The server serves
one active document per connection; M2 formatted output is capped at 131,072 bytes.
Go to Definition applies only to the scalar profile. The compiler remains the authority for
accepted source, invocation and resource limits. Opening, editing, formatting and saving never
execute project code.
This candidate does not promise debugging, completion, general application frameworks, browser
bindings, WASI, self-hosting or native Windows executables.

For the compiler-only starter, use `compiler/bin/zryna` (`zryna.exe` on Windows) from any directory
outside the installation:

```text
<compiler> --version
<compiler> new hello
<compiler> run src/main.zry --project-root hello --target javascript --export main --name first-js
<compiler> run src/main.zry --project-root hello --target webassembly --export main --name first-wasm
```

Both runs return 42. Choose fresh `--name` values to repeat; existing output bundles are never
overwritten. The existing compiler M2/M3 profiles retain their published limits and require
matching project manifests/locks. The editor's M2 Run creates a fresh authenticated M2 package.
Linux native execution additionally requires the supported external GNU toolchain and is outside
this portable no-development-tools exercise.

## Relocation, rollback and removal

Close this candidate's editor window normally after saving your work. Move the complete setup
directory, then update only `zryna.installationPath` in this profile's user settings. Keep the same
reviewed `zryna.installationDigest`; verify the moved setup again. No workspace setting can
override executable selection. A changed/missing file or wrong digest rejects the setup.

To roll back, reopen the previous editor profile with its previous installation. To uninstall,
close the candidate window normally and remove only the candidate installation and its dedicated
profile after retaining any outputs you want. Never remove your separate project folder. No PATH,
system installation, security policy or existing editor profile was changed.

## Building and reproduction (developers only)

Building requires pinned Rust 1.97.1, Node 22.22.1, pnpm 11.18.0, the platform linker, Git and
Cosign. Run the repository's required checks, commit the candidate, and build the language server
with `ZRYNA_TOOLING_SOURCE_COMMIT` set to that exact commit. Use release mode, one codegen unit,
stripped symbols, path remapping and deterministic linker flags for the supported host. Build a
second replica in a different absolute directory and compare its bytes before claiming binary
reproducibility. A repeatable command alone is not observed byte reproducibility.

`pnpm editor:package` produces a VSIX with normalized ZIP timestamps. Download all immutable
v0.2.3 release assets, then run with the pinned build Node:

```text
node scripts/portable-setup/build.mjs --release <assets-directory> --cosign <absolute-cosign> --server <built-server> --vsix <packaged-vsix> --output <new-output-directory>
```

The builder verifies signatures, exact source/VSIX correspondence, embedded server revision and
the complete compiler archive before assembling a deterministic outer archive and candidate
receipt. Local unsigned receipts establish only the identities observed in that build. Required
hosted checks, independent binary replicas and user acceptance are separate evidence. The editor's
registry publication and installed-host verification are recorded in the editor guide; protected
outer-setup publication and external pilot recruitment remain separately authorized work.
