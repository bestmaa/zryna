# Release notes

## 0.5.0 — M3 formatter candidate

- Add explicit M3 editor formatting for admitted data/ownership source and saved imports below the trusted workspace folder.
- Bind the M3 server to that folder at startup and require the matching 0.5.0 capability before sending source.
- Keep explicit Run limited to the verified scalar and M2 profiles.

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
M3 formatting and marketplace publication remain pending. A built or installed VSIX is not a
marketplace release. No new compiler tag or binary release accompanies this package.
