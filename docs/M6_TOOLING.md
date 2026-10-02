# M6 tooling support and evidence

Status: final integration verification is pending. The restricted source playground and its
separate toolkit are under verification; neither has a supported launcher, published toolkit
or completed M6 acceptance. This page records the bounded tooling surface and the evidence
required before those claims can change. It does not activate debugging or additional query forms.

The [M6 conformance contract v1](../spec/tooling/M6_CONFORMANCE_V1.md) defines the common evidence
identity and complete acceptance matrix. Existing [architecture](ARCHITECTURE.md),
[standalone projects](STANDALONE_PROJECTS.md), [language-server and editor contracts](LANGUAGE_SERVER.md),
and [portable setup](PORTABLE_SETUP.md) retain their own compatibility and release status.

## Version and support matrix

Component versions identify different products; they are not interchangeable release numbers.

| Surface | Exact version or profile | Documented behavior | Status and evidence boundary |
| --- | --- | --- | --- |
| Standalone compiler/project | Compiler 0.2.3; generated `i32-v1` project | `new`, frozen source authentication, create-only JavaScript/core WebAssembly build and run on Linux/Windows | Existing project contract; final M6 integration must rerun its applicable project and portability gates. |
| Language server | Server 0.5.0; scalar protocol v2, explicit `control-flow-v1` v3 or `data-ownership-v1` v4 | Bounded stdio diagnostics, full-text revisions, negotiated coordinates and formatting; scalar definition only | Existing LSP contract; no build or execution method. Final integration evidence is pending. |
| Formatter | `scalar-format-v1`, `control-flow-format-v1`, `data-ownership-format-v1` | Compiler-admitted document/range edits, token/comment preservation, idempotence and revision checks | Delivered through the matching LSP/editor. There is no `zryna fmt` CLI command. Final integration evidence is pending. |
| Editor extension | VSIX 0.5.0; VS Code-compatible API >=1.82.0 | Diagnostics, scalar definition, M1–M3 formatting; explicit saved-file Run for scalar/M2 JavaScript or core WebAssembly | Separately published extension with historical installed-host evidence. Publication does not authenticate an outer setup or close the new integration. |
| Portable setup | 0.1.0-candidate.3; compiler 0.2.3, server/VSIX 0.5.0 | Reviewed complete installation, isolated editor profile, explicit compiler/editor exercises and relocation | Internal review/user-acceptance candidate. No public outer publisher signature; production admission remains forbidden. |
| Fixed-component example | Existing `browser-component-v1` bundle | Local execution of fixed `examples/universal/add.zry` with editable scalar inputs | Independently documented [example](../examples/playground/README.md). It does not compile edited source or establish source-playground acceptance. |
| Restricted source playground | `restricted-browser-v1` transport; `browser-component-v1` output | Candidate one-file source compilation, compiler-owned diagnostics and admitted scalar component evaluation | Under verification. Actual positive compiler/browser/isolation acceptance and a supported launcher remain pending. |
| Restricted toolkit | Proposed 0.1.0, proposed `playground-v0.1.0` immutable prerelease | Separate authenticated compiler/material/binding/WIT/browser/policy composition | Not published. Outer publisher authentication, durable artifacts and final integration receipts are pending. |

The extension requires the exact selected analysis/formatting capability and matching server
source identity before transmitting source. An immutable compiler release's older server is not
made compatible by the compiler version number. M3 formatting uses the trusted saved-import root;
M3 editor Run, M2/M3 definition, completion, rename and debugging remain unavailable. See the
[editor compatibility table](LANGUAGE_SERVER.md#editor-installation-and-compatibility).

Standalone `--project-root` admission retains its documented default `i32-v1` restrictions;
the playground does not add component profiles to generated standalone packages. Existing
compiler M2/M3 capabilities retain their separate profiles and gates. No milestone or profile
is promoted by success of this restricted tooling slice.

Portable packaging targets remain Windows x64 with the Windows Server 2022 baseline and
Ubuntu 24.04 x64. macOS, other architectures, remote/virtual workspaces and native Windows
program output are outside this portable workflow. Packaging/transport support alone does not
establish a native graphical editor or restricted execution-host acceptance result.

## Preserve historical evidence

The [publication and installed-host evidence](LANGUAGE_SERVER.md#publication-and-installed-host-evidence)
records the reviewed 0.5.0 VSIX and its exact hash, registry installations, and nine real Windows
extension-host checks for each installed package. Those tests used test source
`2f2f64e723924cd329a987e7ac9833149fd0811b` and setup source
`bf8706ab4a8a606ec99d816e8df90d1c4c1f0f31`.

The same record identifies 33 historical hosted checks at
`3fadcbd3d0112dd8cd79bfaa10bd334a18594acc`. Those checks establish the recorded setup revision's
Linux/Windows reproduction and portable acceptance. They do not establish Linux graphical
extension-host acceptance, current playground isolation or a pass at the final M6 integration.

The [portable candidate verification rules](PORTABLE_SETUP.md#verify-before-running-anything)
remain in force. Authentication of its nested compiler does not authenticate the new outer
setup, server or VSIX. Preserve original artifact versions, source revisions, hashes and status.
If the final integration intentionally reuses an immutable artifact, identify those exact bytes
and rerun the applicable integration exercise; never replace a published version with different bytes.

## Existing runnable project/editor workflow

For a reviewer-verified portable candidate, select the absolute installed compiler executable
as described in [portable setup](PORTABLE_SETUP.md). From a separate writable practice directory:

```text
<compiler> --version
<compiler> new hello
<compiler> run src/main.zry --project-root hello --target javascript --export main --name m6-first-js
<compiler> run src/main.zry --project-root hello --target webassembly --export main --name m6-first-wasm
```

`<compiler>` is the verified `compiler/bin/zryna` executable, or `zryna.exe` on Windows. The
documented generated source returns 42; this is the expected observation, not a new test result.
Choose fresh output names when repeating commands because publication is create-only.
Source-checkout users use the explicit compiler/project roots and pinned Node path in
[standalone projects](STANDALONE_PROJECTS.md#build-and-run-from-a-source-checkout).

For an editor exercise, install the exact published 0.5.0 package or the reviewed VSIX, configure
the matching server/installation in user settings, and open a trusted local practice folder.
Select the explicit profile, request Format Document twice, and exercise the documented scalar/M2
Run pickers after saving. Invalid source must yield actual compiler diagnostics and no formatting
edits. Follow [portable setup's editor exercise](PORTABLE_SETUP.md#editor-exercise) for concrete
sources, exports and expected values. Opening, editing, formatting and saving never execute code.

## Restricted playground workflow and limits

The intended complete workflow is: verify the independently authenticated toolkit and host
receipts; start a dedicated supervised browser session; edit admitted example source; compile;
inspect actual source diagnostics; choose a returned export; enter canonical signed i32 values;
run; inspect the observed value and authenticated component hash. Editing or Cancel must invalidate
pending observations and retain one-operation admission until containment-wide cleanup is confirmed.

This workflow is not presently an installation command. No launcher command, download URL,
public service endpoint or successful end-to-end session is advertised here. A supported launcher,
its exact invocation, immutable signed artifact locations and reproduced user exercise must be
added to this page only after the corresponding conformance receipts are accepted.

The candidate compiles exact editable UTF-8 source at fixed logical path `src/main.zry` through
the existing scalar compiler, mandatory IR verifier, component audit and deterministic bindings.
It does not derive executable results from preset answers or a source catalogue. Only returned
exports with scalar i32 parameters/results may be evaluated. Inputs span -2147483648 through
2147483647; negative zero, spaces, leading zeros, plus signs, fractions and exponents reject.

| Candidate boundary | Required finite restriction | Acceptance status |
| --- | --- | --- |
| Source/request | 4096 UTF-8 source bytes inclusive; 32768 request bytes; one fixed source/revision carrier | Implementation under verification; exact/first-extra and malformed-carrier evidence required. |
| Compiler output | 1277956 frame bytes; 4096 stderr bytes; component <=1048576 bytes; each binding <=65536 bytes | Actual compiler output and hostile output-boundary evidence pending. |
| Diagnostics | Compiler schema 1; <=65536 report bytes and 256 diagnostics; messages/guidance <=4096 bytes each | Actual CLI/report parity pending; LSP diagnostics v2 is unchanged. |
| Compilation | 10 seconds plus 2 seconds for teardown; 512 MiB memory, zero swap, 32 tasks, two-CPU quota and effective cpuset | Actual containment/resource/deadline acceptance pending. Threads count as tasks. |
| Evaluation | 5 seconds plus 2 seconds for teardown; independent host watchdog and one operation, no queue | Whole browser process-tree cancellation and deadline proof pending. |
| Dedicated browser | Proposed 1 GiB memory, 256 tasks and two CPUs | Proposed numeric baseline is not accepted; independent finite-policy and positive resource proof required. |
| Toolkit inventory | At most 128 files/materials; each file <=256 MiB; expanded files <=512 MiB; bounded archive and metadata | Publication schema checks alone do not authenticate any executable. |

The dedicated compiler startup now separates material staging from runtime admission. After
authenticated namespace copies, it requires inherited CORE 0, FSIZE 256 MiB and NOFILE 512
soft/hard readbacks. Authentic provider staging lowers FSIZE to 12 MiB and bounds the nine-file
closure to 16 MiB. The same retained stage is revalidated before final FSIZE 1277956 and
NOFILE 128 limits are read back, ahead of the first authenticated Node probe. Consuming compile
completion verifies owned-stage cleanup before returning a response; cleanup failures reject
without a successful frame. This lifecycle source and its prospective hostile/process tests
remain under verification, including the explicitly selected authentic-provider resource test.

Actual parent launch topology remains separate. A direct child of a helper with hard FSIZE
1 MiB and an effective 128 MiB ancestor memory cap cannot supply the larger copying and
compiler envelope. Those helper guards remain required. A separately reviewed parent setup
envelope, inherited limit trace, effective ancestor controls and whole-tree cleanup proof are
required before genuine positive compilation is accepted.

The candidate host surface is non-root Ubuntu 24.04 x86-64 with an operator-provided empty,
already delegated cgroup v2 domain containing cpu, cpuset, memory and pids controllers. Actual
kernel, namespace, seccomp and helper capabilities must be authenticated and demonstrated.
The official fixed bubblewrap 0.12 acquisition route remains under review; no distribution-package
safety claim is implied. Chrome 153.0.8010.12 and playwright-core 1.63.0 are the selected browser
test pins, not evidence of containment. Windows transport tests do not establish Windows host admission.

Packages, imports, user-selected files/commands/targets, native execution, ambient host capabilities
and arbitrary dependency fetching are excluded. Host or compiler availability errors remain separate
from source diagnostics. Diagnostic/source/error text is inert; keyboard use, labels, error focus,
live announcements and Unicode locations require actual browser accessibility evidence.

## Final acceptance record

All advertised surfaces must be exercised against one authenticated integration commit and tree,
with exact per-artifact source/version/hash identities. The [v1 matrix](../spec/tooling/M6_CONFORMANCE_V1.md)
requires complete commands, platforms/profiles, nonzero counts, durable logs/documents/artifacts,
signature/provenance receipts and required hosted results. Earlier binaries or passing unit fixtures
cannot supply a missing final integration row.

Required remaining proof includes actual CLI source-diagnostic/scalar parity across the editable
corpus; exact resource and malformed/expired/unsupported rejection boundaries; revision, cancellation,
isolation and authority substitution; real browser accessibility; runnable exported documentation;
project, LSP, formatter and installed-extension integration; and all applicable Linux/Windows
hosted gates. No row is marked passed by this page. Missing launcher/publication, browser baseline
or final receipts keeps restricted playground and M6 completion under verification.
