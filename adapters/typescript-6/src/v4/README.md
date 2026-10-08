# Protocol-v4 TypeScript syntax adapter

`../worker-v4.mjs` is the unchanged subprocess entrypoint and awaits the internal
`boundary/transport.mjs` runner. These modules implement the existing syntax-only
protocol; they do not resolve modules or names, assign types, lower IR, or emit code.
Protocol v2/v3 workers and the package's public v2 registration remain separate.

The [boundary](boundary/README.md) owns strict request admission, immutable startup
limits, request dispatch, and newline-delimited byte transport. The
[syntax](syntax/README.md) owns TypeScript parsing and provider-neutral source
normalization. Only source snapshots, located diagnostics, and protocol errors
cross stdout. TypeScript nodes, symbols, and numeric syntax kinds stay internal.

All per-request collectors and aggregate counters are allocated by dispatch;
normalizers retain the existing traversal, allocation, and validation order.
Construction and match helpers receive the expression normalizer explicitly to
support recursive operands without an import cycle. Source-offset maps are weakly
keyed by each TypeScript source file.

From the repository root, run `pnpm adapter:check`, `pnpm adapter:test`, and
`node --test tests/syntax-protocol-v4.test.mjs`. See each module README for the
entrypoints, dependencies, and focused cases in `test/worker-v4.test.mjs`.

## Integration requirement

The repository captures and stages exact provider files and authenticates their
bytes. Before integration, every `.mjs` file in this directory must be included in
the coordinated distribution and tooling closure registrations, preserving its
relative path below the worker. The installed/runtime closure must retain its
existing immutable byte checks and bounds. A checkout-only adapter test does not
prove packaged or captured execution.
