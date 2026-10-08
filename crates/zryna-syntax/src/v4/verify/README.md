# Source and arena authentication

The private entrypoints consumed by `v4::verify_snapshot` are `check_budgets`, `verify_file`
and `verify_provider_diagnostics`. `budgets.rs` preflights checked project totals and per-node
limits before allocating verification state. `files.rs` authenticates dense IDs, normalized
paths, complete source identity and top-level ordering. `source.rs` authenticates UTF-8 spans,
exact tokens and the portable identifier profile; `imports.rs` checks bindings and specifiers.

`types.rs` checks flat type ownership, postorder edges, depth and fixed-array spelling/limits.
`declarations.rs` checks nominal members and top-level names. `body.rs` checks signatures and
orchestrates body verification; `statements.rs` checks statement tokens and root/block owners.
`graph.rs` supplies child/root enumeration, bounded place checks and iterative arena order.
`expressions.rs` retains the expression-edge closure and deferred error reporting;
`constructions.rs` and `matches.rs` authenticate their respective token/member forms using
that same closure. `diagnostics.rs` bounds provider text and resolves/sorts its locations.

Dependencies are raw DTOs, limits, source-map authority, diagnostics, the private structure
checks and existing lexical binding checker. All helpers are restricted to v4. Shared, orphan,
forward, cyclic, unreachable or over-depth claims fail closed; syntactic place acceptance
provides no semantic ownership authority. Diagnostic order, codes and bounded truncation remain
owned by the facade's private `Errors` accumulator.

Exact tests:

```sh
cargo test --locked -p zryna-syntax v4::tests::resources
cargo test --locked -p zryna-syntax v4::tests::arenas
cargo test --locked -p zryna-syntax v4::tests::source
cargo test --locked -p zryna-syntax v4::tests::diagnostics
```

These cover raw-vector exact/first-extra ceilings, type ownership/depth, non-recursive places,
source identity/token rejection, canonical integer syntax and deterministic sensitive-name and
provider-diagnostic rejection. Parser producer coverage remains in `cargo test --locked -p
zryna-frontend` and `pnpm m3:syntax:quick`.
