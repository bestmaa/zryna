# M2 deterministic module closure

Status: implemented as a driver-owned compiler boundary selected only by explicit
`--profile control-flow-v1`.

The M2 module-closure boundary turns one validated workspace-relative `.zry` entrypoint into one
immutable, source-map-authenticated module graph. The compiler driver owns filesystem discovery,
path resolution, graph completion, source hashes, cycle rejection, and graph identity. The
protocol-v3 provider receives only normalized portable paths and immutable source text; it never
receives a host path, directory handle, workspace capability, or authority to choose a resolved
dependency.

An internal native import-only entry now supplies verified v3 snapshots to this same driver
algorithm. It lexes each driver-captured source map, parses every nontrivia token as part of a
named import or rejects the whole batch, then invokes the existing v3 verifier. The only accepted
graphs have functionless modules containing named imports and trivia. The driver still owns all
source capabilities and graph decisions. This checkpoint adds no public frontend selection,
package import resolution, semantic execution, or target dispatch.

This boundary does not authorize a backend by itself. Its final authenticated snapshot enters the
[M2 straight-line semantic boundary](M2_STRAIGHT_LINE_SEMANTICS.md) and its
[control-flow extension](M2_CONTROL_FLOW_SEMANTICS.md), then the internal
[M2 JavaScript backend](M2_JAVASCRIPT_BACKEND.md),
[M2 core WebAssembly backend](M2_WEBASSEMBLY_BACKEND.md), or the independently resealed
[M2 Linux x86-64 native backend](M2_NATIVE_BACKEND.md) only through the explicit driver path.
[Manifest v2 and the atomic multi-file command](M2_MANIFEST_V2.md) and the independent
[fixed-oracle three-target gate](M2_CONFORMANCE.md) are implemented. Issue #57 records authenticated
website import, deployment, and live provenance separately from this compiler boundary.

## Fixed-point algorithm

`WorkspaceSourceRoot::capture` first retains one validated workspace-root capability.
`discover_module_closure` then performs these ordered steps:

1. Read the entrypoint through retained, component-relative, no-follow directory capabilities.
2. Build a temporary source map for exactly the new paths in normalized byte order and request one
   exact protocol-v3 snapshot for that batch.
3. Reject provider errors immediately. Fingerprint every verified import using its portable source
   path, source hash, byte ranges, identifier text, and specifier text; temporary `FileId` values
   are not retained.
4. Resolve verified specifiers in the driver, queue only unresolved paths, and read the next batch
   in normalized byte order.
5. Repeat until no unresolved path remains, or reject the first file, byte, edge, declaration,
   round, or call beyond a frozen limit.
6. Reject cycles and revalidate all retained directory and file identities. Build the complete
   immutable `SourceMap` exactly once and request exactly one final full-map protocol-v3 snapshot.
7. Revalidate every retained source again after that call. The final import fingerprints and edge
   set must equal discovery exactly before the driver seals ordered modules, ordered edges, source
   SHA-256 digests, and the graph SHA-256 identity.

An intermediate snapshot never enters semantics. Any provider error stops before another
specifier is resolved or source is read. A one-import-per-file chain performs one batch call per
file plus one final call, and provider source bytes are at most twice the final aggregate source
bytes.

## Path grammar and filesystem authority

Imports must be nonempty named imports whose specifier:

- begins with `./` or `../`;
- uses `/` separators and an explicit lowercase `.zry` extension;
- contains no absolute root, host prefix, backslash, empty component, URL scheme, query, fragment,
  or NUL;
- resolves `.` and `..` lexically without escaping the retained workspace root; and
- satisfies `NormalizedSourcePath` after resolution.

There is no implicit extension, implicit `index.zry`, package or bare resolution, alias mapping,
`node_modules`, URL import, default import, namespace import, re-export, wildcard import, or dynamic
import.

On Unix, root capture starts at a retained `/` capability. On Windows, it starts at a retained
local drive root; the canonical verbatim form of that same local disk root is also accepted. Every
named component is opened handle-relative without following links, and every Windows reparse
attribute is rejected. UNC, device, drive-relative, and all other verbatim namespaces are
unsupported and fail closed. Directory names are enumerated with a fixed bound to enforce
exact ASCII spelling and portable case-collision rejection. Non-ASCII directory entries still
consume that bound but cannot satisfy a requested portable source component.

Final files are opened no-follow as regular files. Unix uses nonblocking opens so a FIFO cannot
stall discovery. Windows permits only read sharing, preventing rename/delete replacement while the
retained source handle exists. Each file is bounded and revalidated immediately after reading;
all retained ancestor and file bindings, metadata, bytes, and hashes are revalidated before and
after final provider authentication.

## Frozen discovery limits

| Quantity | Limit |
| --- | ---: |
| source files / modules | 4,096 |
| aggregate source bytes | 8 MiB |
| import-discovery rounds | 4,096 |
| provider calls including the final full-map call | 4,097 |
| cumulative provider input source bytes | 16 MiB |
| named-import binding edges | 65,536 |
| conservative canonical edge-manifest bytes | 32 MiB |
| import declarations across the closure | 65,536 |
| entries inspected in any retained source directory | 65,536 |
| aggregate discovery wall time | 2 minutes |

Protocol v3 independently enforces the per-module, per-declaration, request, response, diagnostic,
and syntax-arena limits frozen by the M2 language contract. Every counter uses checked arithmetic.
The first item beyond a discovery-owned limit produces `ZRYNA-D3201` and prevents semantic or
artifact phases from running. Each external worker receives only the remaining aggregate wall-time
budget, so one late provider call cannot extend the two-minute discovery deadline.

## Canonical graph identity

Modules are assigned dense IDs after closure in normalized portable path byte order. Each module
records the raw 32-byte SHA-256 of its exact UTF-8 source. Named-binding edges are sorted by
importer path bytes, specifier bytes, imported name, then local alias.

The graph identity is SHA-256 over this exact byte document:

1. ASCII `ZRYNA-M2-GRAPH\0`;
2. little-endian `u32` version `1`;
3. the entrypoint as a little-endian `u32` byte length followed by UTF-8 bytes;
4. a little-endian `u32` file count, then each sorted file path in the same length-prefixed form
   followed by its raw 32-byte source digest; and
5. a little-endian `u32` edge count, then each sorted edge's importer, specifier, imported name,
   and local alias as four length-prefixed UTF-8 fields.

The document contains no host absolute path, locale result, filesystem enumeration order, JSON,
timestamp, target name, or hexadecimal digest spelling. The same accepted source set therefore
produces byte-identical ordered graph identity on Linux and Windows.

## Stable discovery diagnostics

| Code | Meaning |
| --- | --- |
| `ZRYNA-D3001` | invalid entrypoint or module specifier, including root escape |
| `ZRYNA-D3002` | unsafe workspace root or linked/reparsed directory traversal |
| `ZRYNA-D3003` | missing, unreadable, nonregular, or invalid UTF-8 source |
| `ZRYNA-D3004` | retained directory or source identity, state, bytes, or hash changed |
| `ZRYNA-D3005` | wrong-case path or portable ASCII case collision |
| `ZRYNA-D3006` | duplicate named-import binding edge |
| `ZRYNA-D3007` | cyclic module graph |
| `ZRYNA-D3101` | provider reported an error for a discovery batch or final map |
| `ZRYNA-D3102` | provider/final-map authentication or closure invariant mismatch |
| `ZRYNA-D3201` | deterministic discovery resource exhaustion |

Frontend transport failures remain `ZRYNA-F1xxx`. Provider syntax diagnostics are retained by the
verified protocol snapshot, but they cannot substitute for driver-owned path, graph, race, cycle,
or budget decisions.

## Verification boundary

The focused driver suite covers deterministic diamond discovery, repeated-root graph identity,
missing and wrong-case paths, every rejected specifier class, duplicates, cycles, provider errors
and final drift, linked ancestors, source mutation, FIFO rejection, root and final replacement,
and exact/first-extra discovery budgets. The complete repository gates still run on Linux and
Windows before this boundary can merge. No success from this API authorizes semantics or artifact
publication by itself.
