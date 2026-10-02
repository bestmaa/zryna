# Release notes

## 0.5.0 — M3 formatting and initial marketplace publication

Published on 2 October 2026 to the
[VS Code Marketplace](https://marketplace.visualstudio.com/items?itemName=zryna.zryna) and
[Open VSX](https://open-vsx.org/extension/zryna/zryna) as `zryna.zryna`.

- Add explicit M3 editor formatting for admitted data/ownership source and saved imports below the trusted workspace folder.
- Bind the M3 server to that folder at startup and require the matching 0.5.0 capability before sending source.
- Keep explicit Run limited to the verified scalar and M2 profiles.
- Install with `code --install-extension zryna.zryna@0.5.0` in VS Code, or use the Open VSX
  listing in a compatible client. A separately configured matching server 0.5.0 is required;
  the public compiler v0.2.3 server and older 0.4.0 server are incompatible.
- Verified separate clean registry installations and nine real Windows VS Code 1.138.0 host checks
  for each installed package, including scalar definition, M2 diagnostics/recovery, M2/M3
  formatting/idempotence and actual scalar/M2 JavaScript and M2 WebAssembly results. Exact package,
  source and setup identities are recorded in the [publication evidence](https://github.com/zryna/zryna/blob/main/docs/LANGUAGE_SERVER.md#publication-and-installed-host-evidence).

The immutable published VSIX retains its pre-publication documentation. These repository release
notes record the later publication without replacing that package or changing its version.
Portable setup `0.1.0-candidate.3` remains a review candidate with production admission forbidden;
this extension publication creates no new compiler/server release or beta/stable setup.

## 0.4.0 — M2 editor candidate

- Add an explicit editor profile selection for scalar-v2 and control-flow-v1 source.
- Require matching server 0.4.0 capabilities and exact installed source revision before sending source.
- Bind the unchanged compiler 0.2.3 to a separately verified portable setup candidate.
- Extend formatting and explicit Run within the bounded control-flow-v1 language profile.

## 0.3.0 — portable setup candidate

- Verify complete portable installations against an independently supplied manifest digest.
- Require matching server 0.3.0 and exact installed source revision before sending source.
- Reuse the immutable compiler 0.2.3 runtime without a checkout or separate Node installation.
- Retain development configuration, workspace trust and explicit saved-source Run.

## 0.2.0

- Add lexical syntax highlighting.
- Add explicit saved-source scalar Run with export/target selection and validated i32 arguments.
- Show compiler results/errors and open generated JavaScript or reveal actual Wasm/output.
- Preserve trusted-workspace and user-level executable settings; never run on open/save.

## 0.1.0 — initial local preview

- Scalar diagnostics and Go to Definition through the compiler-owned language server.
- Deterministic document and complete-function range formatting for scalar-format-v1.
- Explicit user-configured local compiler/runtime paths and capability compatibility checks.
- Cancellation, stale-result rejection, bounded transport and inert diagnostic rendering.

The original packages required a matching source build; public v0.2.3 servers lack formatting.
The M3 package's prior exact-source Linux/Windows reproducible setup and fresh portable acceptance
passed in #500. Actual registry publication and installed-package Windows host acceptance are
recorded above; a local VSIX build alone is not publication. No new compiler tag or binary release
accompanies this package.
