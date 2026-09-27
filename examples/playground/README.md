# Bounded scalar playground example

This local page runs the existing public `browser-component-v1` bundle for the fixed
`examples/universal/add.zry` source. Its five presets use the same signed `i32`
observations as the browser component corpus. You can edit the two scalar arguments;
the page does not compile edited source or report compiler diagnostics. It reports
input validation errors and bounded runtime failures.

From the repository root, with the pinned Node and Rust toolchains:

```text
cargo run --locked -p zryna -- build examples/universal/add.zry --profile browser-component-v1 --target component --name playground-add --node <absolute Node 22.22.1 path>
node examples/playground/serve.mjs
```

Open `http://127.0.0.1:8001/examples/playground/`. The create-only build fails if
`.zryna/out/playground-add.build` already exists; use a clean workspace for a new
revision. The local server checks the exact source digest and all three artifact
hashes against the browser manifest before it listens. It serves only the page,
fixed source, and exact bundle files from loopback. The generated loader checks
the component digest and interface before instantiation. The page displays the
component SHA-256 after a successful run. Keep the source, manifest, loader,
declarations and component from one build together.

The guest receives no imports or host capabilities. One worker runs at a time;
each run has a five-second outer deadline and Cancel terminates that worker.
The worker bounds source, manifest and component response bytes, and the server
sets a restrictive content security policy. The fixed component has the audited
scalar interface; no package, native, filesystem, account or user supplied code
execution is accepted. Browser process memory does not have a separate hard cap.
This is a local example, not a public hosted playground or M6 closure claim.

`node --test tests/playground.test.mjs` checks corpus values, input admission,
worker concurrency, cancellation, expiration and malformed worker responses.
The existing `browser_component_` CLI tests and pinned real-browser fixture remain
the artifact and browser parity authority. A browser compiler and exact M6 tooling
evidence are still required for the full issue 410 scope.
