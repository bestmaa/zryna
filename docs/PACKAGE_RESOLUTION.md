# Source-only package resolution

Status: implemented source-only package graph and lockfile support for Issue #403. This is not a
package registry, package-import syntax, build engine, release publisher, or native dependency
manager.

The internal [native source snapshot route](NATIVE_SOURCE_SNAPSHOTS.md) may consume one exact
instance's immutable source inventory from this frozen graph. It retains the resolver's source
capabilities and original bytes through syntax verification and dispatch. It does not add alias
imports, a module-export table, source acquisition or lock updates.

## Files and command

Each package directory contains canonical `zryna.package.json` bytes using the closed
`zryna.package.v1` definition in
[`zryna-package-release-v1.schema.json`](../schemas/zryna-package-release-v1.schema.json). The root
package additionally receives canonical `zryna.lock.json` bytes using `zryna.lock.v1`.

```text
zryna package resolve <PACKAGE> --source-root <PATH> --mode update [--git-cache <PATH>] [--json]
zryna package resolve <PACKAGE> --source-root <PATH> --mode frozen [--git-cache <PATH>] [--json]
```

`<PACKAGE>` is the root package's lowercase portable locator relative to the absolute resolved
source root. `update` resolves and authenticates the complete graph, revalidates every retained
input, then publishes the root lock with one atomic file replacement. A missing destination is
created without exposing staged bytes. A failure preserves the prior lock and removes the known
private staging name. `frozen` requires the existing lock to be byte-canonical and exactly equal
to the newly authenticated graph; it never writes or repairs the lock.

Revalidation immediately before publication reopens each package and descendant directory from
the retained source or cache root, compares directory identities and complete bounded entry sets,
then rechecks retained file identity and state without reopening or rereading file contents.
Captured bytes are authenticated against their declared inventory before becoming immutable
compiler inputs. This detects persistent replacement
or entry mutation observed at that boundary; it is not an atomic filesystem snapshot against an
arbitrary hostile writer that continues mutating paths after validation.

For a local-only quickstart, create this layout:

```text
example/
  packages/app/zryna.package.json
  packages/app/src/main.zry
  packages/library/zryna.package.json
  packages/library/src/main.zry
```

Both manifests list every source file with its exact byte size and SHA-256. The app manifest uses
source `{ "kind":"local", "locator":"packages/app", "revision":"" }` and declares the library
with its exact name, version, alias, and local source tuple. Run:

```text
zryna package resolve packages/app --source-root /absolute/path/to/example --mode update
zryna package resolve packages/app --source-root /absolute/path/to/example --mode frozen
```

Moving the complete source root preserves identity; changing a package's relative locator changes
its manifest and package identity. The command emits no compiler artifact and does not reinterpret
dependency aliases as source imports.

## Exact Git cache

Git sources must use canonical lowercase HTTPS `.git` locators and full lowercase 40-character
commits. Resolution never runs Git or contacts the network. The caller may provide a prepopulated
material cache whose entry directory is lowercase SHA-256 of:

```text
UTF8("ZRYNA-PACKAGE-GIT-CACHE-V1\0" + locator + "\0" + commit)
```

The entry contains `zryna.package.json` and its declared regular source files. Its manifest must
repeat the exact locator and commit. Missing entries, branches, tags, abbreviated commits,
redirects, alternate transports, source substitution, undeclared files, links, reparse points,
special files, byte drift, and checksum drift reject without fallback. Cache placement is not
identity or trust; all bytes are independently checked against the manifest.

## Bounds, identity, and unsupported forms

The resolver preserves the accepted #168/#360 package identities. A package instance is exactly the pair
of the domain-separated canonical manifest digest and the domain-separated complete source-file
inventory digest. Dependencies are selected by exact name, version, and source tuple. Aliases order
edges but do not change instance identity. The graph is target/runtime only, so its opaque package
semantic-domain input retains the exact profile, role, and instance. Same-name packages from
different versions or sources remain distinct. The resolver does not accept module paths or
declaration ordinals as nominal authority: only semantics may seal final nominal identity after
receiving an authenticated module and real source-ordered declaration ordinal.

One graph admits at most 16 packages, eight dependencies per package, 16 files per package,
256 total directory entries inspected per package, 1,024 bytes per source file, 96 bytes per
portable path, six wire-container levels, and 65,536 bytes per manifest or lock record. The entry
budget includes the manifest, an existing lock, source files, and directories including empty
directories. Enumeration rejects the first extra entry before name conversion or retention and
never silently truncates it. Collections are canonical and unique. The first extra item,
cycle, orphan, absent exact selection, incompatible compiler/profile/target coverage, path escape,
case collision, file/directory collision, duplicate incompatible instance, stale lock, or unsafe
filesystem object rejects atomically.

Unsupported forms are semver ranges, pre-releases, feature selection, implicit defaults, branch or
tag resolution, registry or mirror lookup, transparent fetch, submodules, Git LFS, hooks, scripts,
host/build dependencies, native recipes or libraries, arbitrary tools, deep/package imports,
package type imports, source acquisition, compilation, cache execution, and package publication.

The manifest and lock still carry one exact compiler identity. Standalone compilation normally
requires that identity to equal the running compiler. The only reviewed exceptions are the
explicit `0.2.1` project to `0.2.3` compiler and `0.2.2` project to `0.2.3` compiler transitions.
This is a closed table, not a SemVer range; `0.2.0`, future or unrelated versions, profile changes,
target omissions, manifest drift, stale locks, and source inventory changes continue to fail
closed.

## Diagnostics

| Code | Meaning |
| --- | --- |
| `ZRYNA-P4001` | Wire or graph resource bound exceeded. |
| `ZRYNA-P4002` | Invalid UTF-8/JSON or noncanonical wire bytes. |
| `ZRYNA-P4003` | Closed schema, value vocabulary, or collection ordering rejected. |
| `ZRYNA-P4004` | Source material is absent, substituted, unsafe, unstable, or mismatched. |
| `ZRYNA-P4005` | Portable path or locator is unsafe. |
| `ZRYNA-P4006` | Exact dependency selection is absent or ambiguous. |
| `ZRYNA-P4007` | Package-instance or package semantic-domain identity is invalid. |
| `ZRYNA-P4008` | Dependency edge, cycle, or reachability validation failed. |
| `ZRYNA-P4009` | Compiler, profile, or target coverage is incompatible. |
| `ZRYNA-P4010` | Frozen lock is stale or differs from the authenticated graph. |
| `ZRYNA-P4011` | Atomic lockfile staging or publication failed. |

Diagnostics contain stable logical categories and never print credentials or absolute source/cache
paths. JSON success reports the lock digest, package count, and whether update publication occurred.

The exact root-package `.zryna` directory is reserved for project-owned generated state and is not
source material. Resolution retains and revalidates that directory entry but does not traverse its
contents. A non-directory at that name rejects, and every other undeclared file remains part of the
authenticated inventory check. See [standalone projects](STANDALONE_PROJECTS.md).
