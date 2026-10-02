# Compiler documentation bundles

The compiler repository owns normative documentation and exports reviewed Markdown for the public
website without transferring semantic authority to the website renderer.

## Producer command

From a clean checkout of `main`, export the moving `next` channel with explicit authenticated
provenance:

```bash
pnpm docs:export -- \
  --channel next \
  --source-commit <40-character-lowercase-commit> \
  --source-ref refs/heads/main \
  --output .zryna/out/docs/next
```

The command verifies that the commit, ref, clean tracked worktree, and GitHub workflow environment
agree. A semantic-version channel must equal the compiler package version and use the matching
immutable `refs/tags/v<version>` ref.

The separate restricted-toolkit channel is exactly `playground-0.1.0`, with
`refs/tags/playground-v0.1.0`; its compiler package version remains `0.2.3`. Its dedicated internal
producer requires a separately selected integration commit/tree and the exact protected annotated
tag/workflow context validated by `captureProtectedSource`. It reads the package, registry, schema
and every documentation input directly from ordinary nonexecutable Git blobs at that commit.
A plain receipt or disabled Git verification cannot authorize this route.

The protected workflow entry point uses separate absolute, previously absent output and evidence
directories outside the clean source checkout:

```text
node scripts/docs/export-playground.mjs --source <absolute-source-checkout> --source-commit <selected-commit> --source-tree <selected-tree> --output <absolute-new-bundle> --evidence-output <absolute-new-evidence>
```

The two selected identities must come from independent protected admission/review, not from an
adjacent candidate policy. This command is an internal producer contract; no actual protected-tag
export or public toolkit publication is established here. Local Git fixtures simulate workflow
context and do not prove GitHub protection or publication.

This Linux-only output route retains no-follow source/output ancestors and created directory/file
handles, writes create-only document leaves, verifies named identity and exact bytes after writes,
then writes the manifest/checksum last. It preserves failed partials without rename, overwrite,
recursive cleanup or a pathname fallback. The separate source receipt retains the selected tree,
annotated tag object and exact workflow blob identity. Existing `next` and compiler-version output
semantics remain unchanged.

The official `next` artifact is published only by the dedicated `main`-push documentation job after
the aggregate required `m3` job (which retains M0–M2) succeeds. Its artifact name and job summary bind the exact compiler
commit and manifest SHA-256 as `zryna-docs-next-<commit>-<manifest-sha256>`; consumers authenticate
both values from that immutable workflow run before importing any bytes.

`docs/website-bundle-v1.json` is the explicit, ASCII-sorted source whitelist. Export never discovers
new documentation implicitly. Each source must be a bounded regular non-symlink UTF-8 Markdown
file. Output is staged privately, independently validated, and renamed into a previously absent
child of `.zryna/out/docs`; an existing bundle is never replaced.

## Bundle format

The `zryna.docs.bundle.v1` schema is
[`schemas/zryna-docs-bundle-v1.schema.json`](../schemas/zryna-docs-bundle-v1.schema.json). Every
bundle contains:

- canonical `manifest.json` with no generation timestamp;
- canonical `manifest.sha256` covering the exact manifest bytes;
- only the Markdown documents listed by the manifest.

The manifest binds the full compiler commit, full Git ref, package version, channel, stable document
IDs and paths, byte lengths, and per-document SHA-256 digests. Identical source and provenance
produce identical bytes.

The new fixed toolkit channel retains the same manifest fields, document IDs, paths and navigation
contract. Its dedicated channel/ref alternative is validated with compiler package version `0.2.3`.
The external website importer/schema and independent lock must explicitly accept this channel
before importing it; that integration remains pending and is not inferred from local consumer tests.

The bundled checksum detects corruption but is not its own trust root. Consumers must obtain the
expected manifest digest, channel, compiler commit, and ref from a reviewed workflow artifact,
signed release, or equivalent authenticated channel. Validate those expectations with:

```bash
node scripts/docs/check.mjs \
  --bundle .zryna/out/docs/next \
  --expected-manifest-sha256 <authenticated-digest> \
  --expected-channel next \
  --expected-source-commit <authenticated-commit> \
  --expected-source-ref refs/heads/main
```

The website vendors one reviewed bundle and a separate lock containing these expectations. It may
validate and present compiler-owned Markdown, but it must not infer support, rewrite normative
claims, or silently follow a moving branch.
