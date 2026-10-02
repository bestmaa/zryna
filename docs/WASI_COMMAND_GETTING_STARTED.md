# Bounded WASI command walkthrough

Status: implementation review candidate. The complete Linux and Windows acceptance gates and
independent final review remain required before this becomes a supported public profile.
The [H1 contract](WASI_COMMAND_H1_CONTRACT_DRAFT.md) specifies its source, grant, conversion,
denial and execution-record boundaries. The [activation proposal](WASI_COMMAND_ACTIVATION_PROPOSAL.md)
records the prerequisite decisions.

This route runs one source file from a compiler checkout. It uses the pinned Node.js 22.22.1
executable only for the authenticated protocol-v4 syntax provider, then a fresh pinned Wasmtime
48.0.1 engine for the independently audited command component. Installed package manifests
currently declare other profiles; an installed command or `--project-root` cannot select this
profile. Build the checkout CLI and use its executable for the commands below.

## Pure command and declared error

Set `NODE` to the absolute direct pinned Node executable. From the checkout root:

```sh
cargo build --locked -p zryna
./target/debug/zryna run examples/wasi-command/pure.zry --target wasi-command --profile command-h1-v1 --export main --node "$NODE" --name command-pure-1
```

On Windows, use `target\debug\zryna.exe` and PowerShell's `$NODE`. If `CARGO_TARGET_DIR` selects
another build directory, use that directory's executable. `main` takes no arguments and returns
`bool`: true produces the declared WIT `ok(())`; false produces `err(())`. The CLI prints those
typed results, never a raw source boolean. The pure example needs no grant file and makes no
host callback. `examples/wasi-command/error.zry` demonstrates the declared error; its complete
execution record is committed and the CLI exits with status 5.

Only the exact `run`, `wasi-command`, `command-h1-v1`, `main` combination selects this route.
`build`, scalar arguments, other exports, mixed targets/profiles and project roots reject.
Ordinary profiles reject `--grant-file` rather than silently accepting host authority.

## One explicit environment value

[The lookup example](../examples/wasi-command/lookup.zry) calls `environmentLookup("MODE")`
once, matches the closed `EnvLookupV1.Found` / `EnvLookupV1.Missing` result, and passes the found
owned String into a private helper that clones, concatenates and drops it. The key is a literal,
not an imported function or a value computed at runtime. The example returns true for Found
and false for Missing; it prints no input value.

An explicit owner-private request file approves exactly the source's key. The file contains:

```json
{"schema":"zryna.wasi-command-request.v1","world":"zryna:capability-profiles/command@0.1.0","grant":{"capability":"environment","key":"MODE"},"input":{"present":true,"value":"on"}}
```

The file is at most 4,096 bytes, the key 1–64 UTF-8 bytes, and a present value 0–1,024 UTF-8
bytes. Unknown or duplicate fields, extra grants, malformed Unicode, trailing records and
over-limit values reject before engine creation. Missing is exactly `"input":{"present":false}`;
it still requires the approved key. A present empty string is Found, not Missing.

On Linux, create a new caller-owned regular file with mode 0600. This example refuses to replace
an existing file:

```sh
REQUEST="$HOME/command-request.json"
(umask 077; set -C; printf '%s\n' '{"schema":"zryna.wasi-command-request.v1","world":"zryna:capability-profiles/command@0.1.0","grant":{"capability":"environment","key":"MODE"},"input":{"present":true,"value":"on"}}' > "$REQUEST")
./target/debug/zryna run examples/wasi-command/lookup.zry --target wasi-command --profile command-h1-v1 --export main --node "$NODE" --grant-file "$REQUEST" --name command-found-1
```

In Windows PowerShell 5.1, create a new file with a protected DACL that grants access only to
your current effective user and SYSTEM before writing the value. The runtime checks the actual
owner and DACL through its retained file handle; a private-looking parent directory is insufficient.

```powershell
$REQUEST = Join-Path $env:USERPROFILE 'command-request.json'
$bytes = [Text.Encoding]::UTF8.GetBytes('{"schema":"zryna.wasi-command-request.v1","world":"zryna:capability-profiles/command@0.1.0","grant":{"capability":"environment","key":"MODE"},"input":{"present":true,"value":"on"}}')
$user = [Security.Principal.WindowsIdentity]::GetCurrent().User
$system = [Security.Principal.SecurityIdentifier]::new('S-1-5-18')
$acl = [Security.AccessControl.FileSecurity]::new()
$acl.SetOwner($user)
$acl.SetAccessRuleProtection($true, $false)
foreach ($sid in @($user, $system)) {
  $acl.AddAccessRule([Security.AccessControl.FileSystemAccessRule]::new($sid, 'FullControl', 'Allow'))
}
$file = [IO.FileStream]::new($REQUEST, [IO.FileMode]::CreateNew, [Security.AccessControl.FileSystemRights]::Write, [IO.FileShare]::None, 4096, [IO.FileOptions]::None, $acl)
try { $file.Write($bytes, 0, $bytes.Length) } finally { $file.Dispose() }
target\debug\zryna.exe run examples/wasi-command/lookup.zry --target wasi-command --profile command-h1-v1 --export main --node "$NODE" --grant-file "$REQUEST" --name command-found-1
```

Use a supported local absolute path without links, junctions, reparse points, traversal or hard
links. The driver retains the original file and ancestors, validates one bounded immutable
snapshot and rechecks that original authority before the mediated lookup. It holds the capture
through the execution-record commit, then closes it. It never modifies or deletes your file,
creates a temporary disk copy, or reads values from the process environment, arguments or cwd.

| Request/source combination | Observation |
| --- | --- |
| Pure source, no file | Empty grants, zero host quotas, source determines `ok` or `err` |
| Lookup source, matching key, present value | Found with one owned String |
| Lookup source, matching key, present empty value | Found with an owned empty String |
| Lookup source, matching key, missing value | Missing; the example returns WIT `err` |
| Lookup source, no file or wrong key | Rejected before engine creation |
| Pure source, any grant file | Rejected before engine creation |
| Approved policy revoked at the callback | Fatal host denial; no WIT return and no value read |

Other command-world interfaces remain denied. Arguments are always an empty list and initial
cwd is absent. Filesystem, clocks, random, sockets/network and process access receive no grant.
The first source gate accepts one file and one literal H1 call site, with ordinary M3 owned and
control-flow operations; it rejects dependencies and additional host operations.

The exhaustive initial grant table is narrower than the specified registry's eligibility:

| Category | Registry ceiling | Initial effective authority |
| --- | --- | --- |
| Clock | 64 subscriptions, 64 timers | None; all callbacks denied |
| Environment | 128 entries, 65,536 key/value bytes | Exactly one literal key with an explicit private file; otherwise none |
| Filesystem | 16 preopens, 256 open descriptors | None; no inherited or requested descriptor |
| Network | 64 allowed endpoints, 128 concurrent operations | None; no sockets, DNS or endpoints |
| Randomness | 65,536 bytes/call, 8,388,608 bytes/instance | None; no entropy callback |
| Process, HTTP and other interfaces outside the command world | No command grant | Rejected by source/component admission; private host probes remain denied |

The one environment reservation is static: one entry and at most 1,088 key/value bytes (64 +
1,024), with zero quotas for every other category. It is independent of present/missing and
the actual value length; repeated reads do not consume another grant. All other capability
spellings, arrays of grants, duplicates, conflicts and extra JSON fields reject before an engine
is created. Registry eligibility alone confers no authority.

## Bundle and execution record

Each run creates `.zryna/out/<name>.wasi-command-run/` with exactly:

```text
wasi-command/<name>.wasm
zryna-wasi-command-manifest-v1.json
```

Use a fresh name for another run. An existing destination is never replaced, and a failed
publication removes only its owned private transaction. Ordinary source errors or request
rejections create no final bundle. A consumed execution with `err`, host denial or trap still
produces a complete record when publication succeeds.

The distinct manifest records source/program/composition identities, the exact pinned world and
34-file WIT closure, core/component/layout digests, requested and effective grants, fixed limits,
input presence and byte count, actual execution outcome, and confirmed or unconfirmed teardown.
It excludes the private value, request path and unkeyed value hash. The caller retains the value
file; the manifest is an execution observation rather than durable secret-value attestation.
Parsing an editable manifest cannot construct execution authority.
See the [complete manifest field reference](WASI_COMMAND_MANIFEST_V1.md).

| Execution | `runReturn` | Additional observation |
| --- | --- | --- |
| `run-returned` | `ok` or `err` | No trap or denial fields |
| `host-denial` | `absent` | First authenticated denied interface/operation, permission-denied, host policy revision |
| `runtime-trap` | `absent` | Controlled-language, interface-violation or host-process-failure category |

Only exact audited runtime component/role/function/instruction coordinates confer a controlled
language or canonical-interface identity. Fuel/deadline exhaustion, unrelated traps and host
exceptions have no such identity. Denial takes precedence over its deliberate host trap.

The envelope is fixed at 100,000 fuel, a five-second epoch deadline, 64 KiB guest stack, two core
instances, one non-growing 256-page memory and zero tables. Its disjoint static, language and
canonical-transfer regions and 16-live / 4,096-total transfer-allocation limits are described in
the H1 contract. A fatal canonical failure consumes the store; there is no retry or guest cleanup
after that failure. Teardown requires store consumption and joining the owned deadline worker.

Add `--json` for the versioned CLI response. Status 0 requires WIT `ok(())` and confirmed
teardown. A declared `err`, denied callback or trap exits 5; unconfirmed teardown exits 6.
Command flags and invocation-mode validation exit `2`. Private grant-file capture, decoding
and source/grant admission exit `3`; this includes an omitted lookup grant, a wrong key and
a grant supplied to pure source. Other preparation failures keep their phase-owned statuses.
