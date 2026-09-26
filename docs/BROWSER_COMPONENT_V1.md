# Browser scalar component profile v1

```text
cargo run --locked -p zryna -- build examples/universal/add.zry --profile browser-component-v1 --target component --name add-browser --node <absolute Node 22.22.1 path>
```

This produces one create-only bundle below
`.zryna/out/add-browser.build`. Use the repository-local CLI; standalone installed package
compatibility does not yet admit components. An existing bundle of that name is never replaced.

Run `node examples/browser-component/serve.mjs` from the repository root, then open
`http://127.0.0.1:8000/examples/browser-component/`. Select
`.zryna/out/add-browser.build/component/add-browser.wasm` in the page. The example imports the
generated loader from that exact build path and displays `add(20, 22) === 42`, plus typed
rejections for a string argument, non-integral number, negative zero and wrong arity. The page
does not supply a guest import or capability. The loopback-only static server is a host setup step, not a
network permission granted to the component.

The pinned Chrome fixture loads this checked-in page and its generated loader through exact
browser routes, selects the component with the file picker, and checks the displayed result and
rejections. It does not launch the example's loopback static server.

## Generated interface

The bundle contains exactly one audited Component Model artifact, one deterministic ESM loader,
one `.d.mts` declaration, and `zryna-browser-manifest-v1.json`. The loader exports
`browserComponentIdentity` and `instantiateBrowserComponent(bytes, { deadlineMs? })`. Pass the
component as an ordinary `Uint8Array` backed by an `ArrayBuffer`. The loader copies those bytes,
requires the exact expected length and SHA-256, then instantiates only the exact audited core
module retained inside the component. This scalar path uses the verified `i32` core exports;
current browsers do not need a general Component Model runtime. The component's canonical
lifted exports and core function names come from the sealed backend interface. No inferred ABI
or caller-declared function map is accepted.

The resolved object has one immutable property per exact source-level logical export. Each
function requires the verified arity and primitive signed `i32` arguments. Strings, boxed
numbers, fractions, out-of-range values, negative zero, `null`, and missing arguments reject
before guest entry. Results are checked as signed `i32`. The declaration gives TypeScript types;
runtime checks remain authoritative. The optional integer `deadlineMs` is 1–30,000, default
5,000. The loader checks elapsed time after hashing and instantiation and before/after a call;
WebAssembly compilation and a synchronous guest call cannot be preempted by this API. A late
result is never returned after an observed deadline excess.

The browser world is exactly `zryna:capability-profiles/browser@0.1.0` with no imports. DOM,
network, filesystem, environment, clocks, randomness, threads, async APIs, non-scalar values,
framework integration, package registry publication and component `run` remain unavailable to
guest code. Loader integrity hashing uses the browser's Web Crypto API; elapsed-time checks use
the browser performance clock. Those host APIs are not passed to the guest. This version is
tested against the pinned Chrome for Testing 153.0.8010.12 browser fixture; other browsers have
no verified compatibility claim.

## Manifest and failure codes

The browser manifest has `version: 1`, `profile: "zryna-browser-component-v1"`, `command:
"build"`, one `component` target, three ordered artifacts (`webassembly-component`,
`ecmascript-module`, `typescript-declarations`), and the unchanged M1 entrypoint/source/stem,
invocation, results and diagnostics fields. Its `browser` record contains the binding revision,
exact WIT world, WIT source digest, component/core/interface SHA-256 hashes, and core offset.
All paths are portable and bundle-relative; no absolute host path, credential, timestamp or
environment value is emitted. A stale loader or substituted component fails with `ZRYNA-B3992`;
wrong byte carrier/length fails with `ZRYNA-B3991`; invalid deadline or observed excess uses
`ZRYNA-B3993`; changed core export topology uses `ZRYNA-B3994`; altered generated interface
metadata uses `ZRYNA-B3995`. Scalar carrier and arity errors
reuse `ZRYNA-B2001`, `ZRYNA-B2002` and `ZRYNA-B2102`.
