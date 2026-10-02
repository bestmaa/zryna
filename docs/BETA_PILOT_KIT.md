# Beta pilot kit (preparation)

This is a fixed, local exercise for Issue #424, not evidence that an external pilot or
beta publication occurred. Use only a candidate delivered through the reviewed handoff.
First verify its archive SHA-256 against the independently supplied digest, then follow
[portable setup](PORTABLE_SETUP.md) for installation and the isolated editor profile.
Record the exact archive digest and `setup.json` digest in the
[local report template](BETA_PILOT_REPORT.md). Never execute an unverified archive.

Setup **0.1.0-candidate.3** combines compiler **0.2.3**, language server **0.5.0** and
editor **0.5.0**. It remains an internal review candidate with production admission forbidden.
It advertises the installed default M1 CLI and scalar/M2 editor exercise.
Repository-local M2/M3 CLI commands below are a **comparison baseline** for
the source-checkout Developer Preview; they do not count as candidate distribution
passes. The retained Windows candidate.2 invocation, `run src/main.zry --project-root
<generated-M1-project> --profile control-flow-v1 --target javascript --export
main`, rejected with `ZRYNA-P4009` because that generated project's frozen package
names the default M1 profile. This historical observation is limited to that invocation;
it does not supply candidate.3 acceptance evidence.
See [standalone projects](STANDALONE_PROJECTS.md) and
[M3 public profile](M3_PUBLIC_PROFILE.md) for the respective boundaries. Do not infer
class, global, native Windows, or production support from any result here.

The nested compiler's immutable v0.2.3 release has its own completed
[installation and upgrade evidence](BETA_INSTALLATION.md). That evidence authenticates
the compiler release, not the outer setup, server or editor composition. Record the exact
candidate.3 archive and setup digests from the reviewed handoff before using this exercise.

## Fixed candidate exercise

Use a new, user-owned practice directory outside the installation. In these commands,
`<compiler>` is the installed `compiler/bin/zryna` (`zryna.exe` on Windows) and
`<practice-parent>` is an absolute path to an existing directory. Run from the
parent directory; replace the placeholders with real paths. The `new` destination
must not exist. Each `--name` must be unused in that project.

```text
<compiler> --version
<compiler> new <practice-parent>/hello
<compiler> run src/main.zry --project-root <practice-parent>/hello --target javascript --export main --name pilot-m1-js
<compiler> run src/main.zry --project-root <practice-parent>/hello --target webassembly --export main --name pilot-m1-wasm
```

Expect `zryna 0.2.3` for this candidate and `javascript: i32 42` and
`webassembly: i32 42`. The generated `hello/src/main.zry` is the existing M1 example.
Outputs are create-only under `hello/.zryna/out`; use fresh names on retries.
Record both observed results and any diagnostic, rather than treating a missing run
as a pass. For Linux native execution, the documented GNU toolchain and Linux x86-64
host are required; this fixed portable exercise does not require native.

For the editor, follow the exact [editor exercise](PORTABLE_SETUP.md#editor-exercise):
default scalar `add(13, -4)` on JavaScript yields `i32 9`, and explicit **M2 control
flow** `accumulate(true, 5)` on JavaScript yields `i32 10`. Repeat the guide's
WebAssembly cases and diagnostic/format checks. Candidate.3 also provides explicit M3
formatting and diagnostics with authenticated saved imports, as described in the guide;
M3 Run remains unavailable. Record editor M3 format/diagnostic observations separately
from scalar/M2 Run and from CLI rows.

## Repository-local CLI comparison

This lane needs a separate compiler checkout with its pinned toolchain and installed
dependencies, following [getting started](GETTING_STARTED.md#prepare-the-checkout).
Run from that checkout root. Set `<checkout-compiler>` to its already-built `zryna`
executable and `<node>` to the absolute direct Node.js 22.22.1 executable. These
commands intentionally omit `--project-root`; they do not test the installed setup.
Use new output names when repeating them.

| Workflow and existing source | Command | Expected observation |
| --- | --- | --- |
| M1 scalar, [`examples/universal/add.zry`](../examples/universal/add.zry) | `<checkout-compiler> run examples/universal/add.zry --target javascript --name pilot-checkout-m1 --export add --arg=i32:20 --arg=i32:22 --node <node>` | `javascript: i32 42`; manifest v1 |
| M2 import and control flow, [`examples/control-flow/main.zry`](../examples/control-flow/main.zry) | `<checkout-compiler> run examples/control-flow/main.zry --profile control-flow-v1 --target webassembly --name pilot-checkout-m2 --export choose --arg=bool:true --arg=i32:21 --node <node>` | `webassembly: i32 42`; manifest v2 |

For M3, save the existing [Pair example](M3_GETTING_STARTED.md#pair-struct-fields-and-arithmetic)
as `.zryna/cache/pilot/owned-pair.zry` in the checkout (create the parent directory
first). Its `score(2, 3)` returns 65. Run:

```text
<checkout-compiler> run .zryna/cache/pilot/owned-pair.zry --profile data-ownership-v1 --target javascript --name pilot-checkout-m3-js --export score --arg=i32:2 --arg=i32:3 --node <node>
<checkout-compiler> run .zryna/cache/pilot/owned-pair.zry --profile data-ownership-v1 --target webassembly --name pilot-checkout-m3-wasm --export score --arg=i32:2 --arg=i32:3 --node <node>
```

Expect `javascript: i32 65` and `webassembly: i32 65`, with manifest v3 in
each run bundle. Owned values stay inside Zryna; the public observation is scalar.
On supported Linux x86-64 with the documented GNU toolchain, a separately named
`--target native` run can be reported as an additional observation. Do not substitute
a Windows native run or claim three-target parity from these fixed commands.

## Report and triage

Keep one local copy of the [report template](BETA_PILOT_REPORT.md) per host and
candidate digest. Record a row for every planned workflow, including skipped and
failed attempts. Note installation, compatibility, diagnostic, elapsed-time and
security observations without raw project source, secrets or private absolute paths.
Use a minimal independently written reproducer only with the owner's consent. Route
suspected vulnerabilities through the [private security channel](../SECURITY.md).
Track blockers in focused issues; a passing local exercise does not satisfy Issue
#424's external participant, clean-host, artifact identity or publication gates.
