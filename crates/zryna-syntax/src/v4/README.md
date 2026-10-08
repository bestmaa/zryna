# Protocol v4 syntax boundary

The public entrypoints remain `zryna_syntax::v4::{decode_snapshot, verify_snapshot}`.
`decode_snapshot` returns untrusted wire values. Only `verify_snapshot` returns an opaque
`ProjectSyntaxSnapshot` bound to one exact `SourceMap`; verification is all-or-nothing.
The verified fields, read-only accessors, diagnostic accumulator and final seal stay in
[`../v4.rs`](../v4.rs). No internal helper constructs an independently trusted snapshot.

| Responsibility | Files and entrypoints |
| --- | --- |
| Wire models | [`raw/`](raw/README.md): project/import, type/data, body, expression and diagnostic DTOs |
| JSON admission | [`decode/`](decode/README.md): exact grammar, duplicate keys and bounded vectors |
| Source authentication | [`verify/`](verify/README.md): file identity, budgets, tokens and flat arenas |
| Source structure | [`structure/`](structure/README.md): child containment and canonical lexical order |
| Resource contract | `limits.rs`: unchanged protocol and per-collection/project ceilings |
| Boundary regressions | [`tests/`](tests/README.md): relocated original unit tests and fixture inputs |

All implementation modules are private. Helper visibility is restricted to `crate::v4`;
raw values retain `UntrustedSpan`, and verified views retain the issuing source-map identity.
Dependencies remain `serde`/`serde_json`, `zryna-source` and `zryna-diagnostics`.
The existing private lexical binding checker remains at `../v4_lexical_bindings.rs`.
This layer authenticates syntax; name resolution, semantic typing and IR construction belong
above it. Protocols v2/v3 and frontend provider selection are separate responsibilities.

From the repository root, run the complete local boundary tests with:

```sh
cargo test --locked -p zryna-syntax
cargo test --locked -p zryna-frontend
pnpm m3:syntax:quick
```

The frontend suite includes native parser candidate, frozen corpus, worker and malformed-provider
coverage. Its declared ignored provider/resource cases retain their original prerequisites;
listing tests is not execution. Canonical repository gates are `pnpm preflight` and `pnpm m0:check`.
