# Request and transport boundary

This module admits protocol-v4 requests and drives the worker's standard streams.
It owns validation and framing; syntax work is delegated to `../syntax/source.mjs`.
Imports do not consume stdin. `../../worker-v4.mjs` starts `runWorker()` once.

| File | Purpose and entrypoints | Dependencies | Focused v4 tests |
| --- | --- | --- | --- |
| `configuration.mjs` | Pin the provider version; initialize production limits and test-only lowered limits in their original order. | Pinned TypeScript API, `../../limits-v4.mjs`, process environment. | Handshake, exact/plus-one budgets, production-environment limit behavior. |
| `errors.mjs` | `AdapterError` and request/budget/invariant failures with stable codes. | None. | Malformed requests, unsupported syntax, bounded error recovery. |
| `request.mjs` | Exact keys and IDs, duplicate-key/JSON budgets, portable paths, sorted analyze file admission. | Configuration and errors. | Unknown/duplicate fields, malformed paths and IDs, shuffled file batches. |
| `dispatch.mjs` | `handle()` returns handshake or analyzes an admitted batch with fresh collector/counters. | Request validation, configuration, syntax source normalizer and diagnostics. | Syntax-only handshake, atomic analysis rejection, project aggregate budgets. |
| `transport.mjs` | `runWorker()` owns UTF-8 decoding, line framing, request/response byte caps, backpressure, error serialization and recovery. | Node streams/events/TextDecoder, dispatch, request validation, errors and diagnostic text compaction. | Invalid UTF-8/JSON, oversized lines/responses, EOF and next-request recovery. |

The request and response shapes, error text, property order, and newline framing
are part of the existing contract. Keep initialization outside the runner so an
invalid provider version or test-limit configuration still fails at startup.
Keep line state inside the runner and request state inside `processLine()`.

Run `pnpm --filter @zryna/adapter-typescript-6 test:v4` from the repository root.
Run `pnpm adapter:test` to include the unchanged v2/v3 boundaries. The byte-output
regression in `test/worker-v4-compatibility.test.mjs` is loaded by the v4 suite.
