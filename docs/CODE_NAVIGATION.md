# Code navigation by task

Reviewed against main `9e31e6249aee44c93579c82c2e87fdafdb4b8b7d`. Paths and commands below are navigation pointers, not a second specification or proof of execution.
Start with [CONTRIBUTING](../CONTRIBUTING.md), then the selected component's README and scoped guidance.
[zryna.workspace.json](../zryna.workspace.json) owns registration/dependencies; [ARCHITECTURE](ARCHITECTURE.md) owns phase boundaries and [STRICT_WORKSPACE](STRICT_WORKSPACE.md) owns enforcement.
Resolve disagreements there, rather than changing this index into another authority.
Source-size policy: [reviewed inventory](../scripts/repository-structure-policy.json),
[read-only checker](../scripts/check-repository-structure.mjs); run `pnpm structure:check` and `node --test tests/repository-structure.test.mjs`.

Public execution is default M1 `I32V1`, explicit M2 `--profile control-flow-v1`, or explicit M3 `--profile data-ownership-v1`.
M3 uses audited target/runtime and atomic manifest-v3 bundles, not a general-purpose allocator or public aggregate ABI.
Use [GETTING_STARTED](GETTING_STARTED.md) for M1/M2, [M3_GETTING_STARTED](M3_GETTING_STARTED.md) for M3, and [CLI](CLI.md) for exact contracts.
## 1. Syntax recognition, source spans, or frontend transport

- Diagnostic transport: [v2 contract](../spec/diagnostics/STRUCTURED_DIAGNOSTICS_V2.md), [schema](../schemas/zryna-diagnostics-v2.schema.json), and [diagnostics component](../crates/zryna-diagnostics/README.md). Run `pnpm diagnostics:contract` and `cargo test --locked -p zryna-diagnostics`; preserve the existing text/JSON-v1 APIs.
- Language-server transport: [public protocol](LANGUAGE_SERVER.md), `apps/zryna-language-server/src/`, and driver `diagnostic_sessions`; focus with `cargo test --locked -p zryna-language-server`, preserving framing/process, stale revision, cancellation, limits, Unicode/CRLF and recovery evidence without adding semantic authority. Scalar/M2 formatting lives in driver `diagnostic_sessions/formatting`; editor client, lexical grammar and explicit Run in `editors/vscode-zryna`, checked with `pnpm editor:check`.
- Revision-bound internal query sessions: `crates/zryna-driver/src/diagnostic_sessions.rs` composes one retained `SourceMap` with unchanged diagnostic v2 bytes and the protocol-v2 definition slice from `crates/zryna-semantics/src/definition_queries.rs`, bounded scheduling and final active-revision publication checks. Focus with `cargo test --locked -p zryna-driver diagnostic_sessions`; this is an internal host boundary, not a transport or editor service.
- Start: [adapter README](../adapters/typescript-6/README.md), [FRONTENDS](FRONTENDS.md), and [v2](SYNTAX_PROTOCOL_V2.md), [v3 control-flow](../spec/language/CONTROL_FLOW_MODULES_V1.md), [v4](SYNTAX_PROTOCOL_V4.md), or [v4 provider conformance](PROVIDER_CONFORMANCE_V4.md).
- Parser-side entry: `adapters/typescript-6/src/worker.mjs`, `worker-v3.mjs`, or `worker-v4.mjs`; choose the protocol explicitly. Native lexical entry: `crates/zryna-frontend/src/native_lexer.rs`; raw-byte admission: `src/native_lexer/admission.rs`. The live pinned-provider differential is `tests/native_lexer_provider.rs` with `scripts/native-lexer-provider-witness.mjs`; this remains a partial stage only, before parsing, snapshot construction, provider handshake, or selection.
- Trust boundary: `crates/zryna-syntax/src/v4.rs::{decode_snapshot,verify_snapshot}` (or matching older protocol); process isolation/handshake in `crates/zryna-frontend/src/worker.rs`.
- Focus: `pnpm adapter:check`, `pnpm adapter:test`, `pnpm protocol:test`; v4 changes: `pnpm m3:syntax:quick` including `worker_process` integration tests, plus `pnpm provider:conformance:v4` for provider equivalence.
- Keep provider output syntax-only; name/type/module resolution belongs downstream. Finish with the full gates below.

## 2. M1/M2 names, locals, calls, branches, or module discovery

- Start: [semantics README](../crates/zryna-semantics/README.md), [M2 control-flow semantics](M2_CONTROL_FLOW_SEMANTICS.md), [module closure](M2_MODULE_CLOSURE.md).
- Entries: `crates/zryna-semantics/src/lib.rs::{SemanticInput::try_new,lower}` for M1; `src/control_flow_v1.rs::lower` for M2.
- Filesystem/module authority: `crates/zryna-driver/src/module_closure.rs::discover_module_closure`; do not put resolution into the adapter or backend.
- Focus: `cargo test --locked -p zryna-semantics`; closure tests in driver `module_closure_tests.rs`; `pnpm m2:quick` for cross-phase M2 checks.
- Source legality and backend profile acceptance are separate. Finish with the full gates below.

## 3. Internal M3 ownership, constructors, borrowing, or cleanup

- Start: [M3 composition](M3_OWNERSHIP_COMPOSITION.md), its [evidence map](M3_OWNERSHIP_COMPOSITION_EVIDENCE.md), and [Copy aggregate semantics](M3_COPY_AGGREGATE_SEMANTICS.md).
- Entry: `crates/zryna-semantics/src/data_ownership_v1.rs::{SemanticInput::try_new,lower}`; follow the selected private lowering route, not every child module.
- Named internal imports: `data_ownership_v1/import_resolution.rs` authenticates relative `.zry` paths, export visibility, aliases/collisions and module reachability/cycles; `function_catalog.rs` retains canonical target identities for call lowering. The completed #272 route accepts the sealed by-value ownership graph and is mapped by `M3_OWNED_CALL_CLOSURE_MATRIX.md`; it adds no nominal type-import grammar or borrowed import.
- Complete enum matching: `owned_aggregate_lowering/structured_match.rs` plans exhaustive sealed variants and active payloads; `structured_match_local.rs` commits joined locals. `M3_COMPLETE_ENUM_MATCHING_MATRIX.md` binds the #273 source, independent-IR and resource evidence; #275 borrowing is completed through its separate matrix.
- Non-indexed owned borrowing: `owned_aggregate_lowering/nonindexed_borrow_shape.rs` selects admitted roots and static/refined payload places; `lexical_indexed_preparation.rs` and `structured_call_arguments.rs` preserve exact lexical and call authority. `M3_NONINDEXED_OWNED_BORROWING_MATRIX.md` binds #275 source, independent-IR and resource evidence.
- Aggregate preparation: `src/data_ownership_v1/owned_aggregate_lowering/{driver,constructor_preparation,constructor_resources}.rs`; Vec route: `owned_vec_lowering/{driver,constructors}.rs`.
- Preparation lifecycle: `owned_aggregate_lowering/preparation_value.rs` builds the bound plan; `preparation_execution.rs` consumes it and `preparation_local_commit.rs` commits the local destination. `constructor_preparation.rs` owns the expression walk.
- Shared typed constructor authority: `owned_constructor_plan.rs`; relevant tests live under `src/data_ownership_v1/tests/` and are registered by its parent tests module.
- Indexed source: `owned_aggregate_lowering/{ordinary_indexed_array_preparation,chained_indexed_preparation,fresh_indexed_preparation}.rs` prepare ordinary access; `lexical_indexed_{statements,scope,preparation}.rs` own persistent aliases. `preparation_indexed_consumption.rs` replays both through the shared plan. See `docs/M3_INDEXED_SOURCE_OPERATIONS.md` for the operation/exclusion matrix.
- Private helpers: `owned_cfg_finalization.rs` finalizes owned CFGs; `copy_lowering/expressions/constructors.rs` handles Copy aggregate constructors; `owned_aggregate_lowering/assignment_planning/source.rs` plans assignment sources; `owned_cleanup_contexts.rs` contains cleanup diagnostic contexts.
- Focus: `pnpm m3:data:quick`; for authority changes also `pnpm m3:owned:quick` and `pnpm m3:contract`. Find the exact neighboring constructor/borrow/cleanup test before selecting a filter.
- Mixed-construction preparation and its bounded evidence are described in the composition map above; this is internal work, not public M3 activation. Preserve both legacy and mixed diagnostic schedules, failure state and resource evidence; finish with full gates.

## 4. Type/layout identity, raw IR, or hostile verification

- Start: [layout README](../crates/zryna-layout/README.md), [IR README](../crates/zryna-ir/README.md), [M3 IR contract](M3_DATA_OWNERSHIP_IR.md).
- Layout authority: `crates/zryna-layout/src/lib.rs::{verify,VerifiedLayouts::type_by_id}`. Raw graphs are not sealed layouts.
- IR authority: `crates/zryna-ir/src/lib.rs::verify`, `src/control_flow_v1.rs::verify`, or `src/data_ownership_v1.rs::verify`; select one profile, preserving the others.
- Indexed authority: `src/data_ownership_v1/indexed_borrows.rs` retains exact container/referent bounds; `indexed_access.rs` seals transient child projection without a dynamic place. Neighboring `tests/indexed_access*.rs` provide independent malformed-input and resource evidence.
- Focus: `pnpm m3:layout`; `cargo test --locked -p zryna-ir` and `cargo test --locked -p zryna-ir --doc`; inspect the matching profile's hostile/raw fixtures.
- Include forged authority, resource boundaries, deterministic replay, and opaque-view tests as applicable. Full verification is not replaceable by producer checks.

## 5. ABI carriers or internal ownership-runtime declarations

- Start: [scalar ABI component](../crates/zryna-abi/README.md), [ownership ABI component](../crates/zryna-ownership-runtime-abi/README.md), [runtime-ABI contract](M3_OWNERSHIP_RUNTIME_ABI.md).
- Scalar authority is `crates/zryna-abi/src/`; M3 sealed declarations/transitions are `crates/zryna-ownership-runtime-abi/src/`, with its registered `include/` header.
- Focus: `cargo test --locked -p zryna-abi`; `pnpm m3:runtime-abi:quick`; use existing shared carrier/transition fixtures rather than target-local competing rules.
- Declaration/transition verification is not an implemented allocator or runtime. Finish with the full gates below.

## 6. JavaScript, core WebAssembly, or scalar component output

- Start: [JavaScript README](../crates/zryna-backend-javascript/README.md) or [WebAssembly README](../crates/zryna-backend-webassembly/README.md).
- Entries: each backend's `src/lib.rs::{emit,emit_control_flow,emit_data_ownership}` consumes the corresponding sealed IR, never source syntax. M3 code is isolated under `src/data_ownership_v1/`.
- Scalar core sealing: `crates/zryna-backend-webassembly/src/scalar_audit.rs` owns the unchanged WASM1/I32V1 validation and instruction audit.
- Scalar component: `crates/zryna-backend-webassembly/src/scalar_component/` owns deterministic wrapping plus the independent final-byte topology/import/type/export audit; `src/wit_world_audit/browser.rs` authenticates the exact empty browser capability world and `src/wit_world_audit/pins.rs` owns its compile-time source closure. Driver selection and publication live in `crates/zryna-driver/src/{pipeline.rs,pipeline/preparation.rs}`.
- Focus: `cargo test --locked -p zryna-backend-javascript` or `cargo test --locked -p zryna-backend-webassembly`; use [the M3 target contract](M3_TARGET_BACKENDS.md) for the new focused execution and audit cases.
- Publication and runtime invocation belong to the driver. The scalar component is build-only, default-M1-only, deliberately absent from `all`, and outside the advertised [v0.1.0 preview support matrix](DEVELOPER_PREVIEW.md); it does not activate a host. Preserve byte/capability audits, scalar carriers, and deterministic output; finish with full gates.

## 7. Native lowering, object audit, linking, or process execution

- Start: [native MIR README](../crates/zryna-native-mir/README.md), [native backend README](../crates/zryna-backend-native/README.md), [M2 native contract](M2_NATIVE_BACKEND.md).
- MIR `src/lib.rs::{lower,verify}` independently seals claims; `src/data_ownership_v1/` owns the M3 raw-to-verified profile; backend `src/lib.rs::{select_object_target,emit_object}` emits/audits objects. M2 has separate profile modules.
- Linking/execution: `crates/zryna-driver/src/native.rs::{discover_linux_native_toolchain,compile_native_invocation,run_native_invocation}`; process failures belong here, not in code generation.
- Focus: `cargo test --locked -p zryna-native-mir`, `cargo test --locked -p zryna-backend-native`, then the relevant driver native tests and `pnpm m2:quick`.
- Object emission and Linux GNU link/run have different prerequisites. Do not infer Windows native support from Windows Rust tests; finish with full gates.

## 8. CLI options, manifests, or create-only publication

- Start: [CLI reference](CLI.md), [driver README](../crates/zryna-driver/README.md), [manifest v2](M2_MANIFEST_V2.md), or the internal [M3 candidate driver and manifest](M3_CANDIDATE_DRIVER.md). For standalone creation and explicit project-root admission, use the [project guide](STANDALONE_PROJECTS.md), `apps/zryna/src/{project,project_filesystem,project_filesystem_capture,project_filesystem_windows}.rs`, `crates/zryna-driver/src/project.rs`, and `apps/zryna/tests/cli.rs`; the compiler checkout and project root remain separate authorities.
- CLI parsing/rendering: `apps/zryna/src/main.rs::main`; explicit profile preselection: `apps/zryna/src/profile.rs::selects_typed_scalars`; orchestration: `crates/zryna-driver/src/lib.rs::compile_to_verified_ir` and `src/pipeline.rs::{build_workspace,run_workspace,build_control_flow_workspace,run_control_flow_workspace}`; target preparation is isolated in `src/pipeline/preparation.rs`.
- Source-to-IR driver tests: `crates/zryna-driver/src/tests.rs`.
- M3 candidate closure and dispatch: `crates/zryna-driver/src/{ownership_closure,ownership_pipeline}.rs`; strict manifest and transaction: `src/{ownership_manifest,ownership_publication}.rs`; complete internal build/run entrypoints: `src/ownership_commands.rs`.
- Focus: `cargo test --locked -p zryna --test cli`, `cargo test --locked -p zryna-driver`; use pipeline fault/publication tests for transaction changes.
- Keep architecture validation first, one verified program per request, and create-only whole-bundle commit. Finish with full gates.

## 9. Workspace layout, dependency rules, CI, or gate scheduling

- Start: [STRICT_WORKSPACE](STRICT_WORKSPACE.md), `zryna.workspace.json`, and the relevant registered component README.
- Enforcement: `crates/zryna-architecture/src/lib.rs::validate_workspace`; pull-request and manual CI: `.github/workflows/ci.yml`; main documentation publication: `.github/workflows/documentation.yml`; local gate entrypoints: `scripts/run-preflight.mjs`, `scripts/run-m0-conformance.mjs`.
- Contract-lane routing: `scripts/classify-workflow-paths.mjs`; focus: `node --test tests/workflow-routing.test.mjs`. Downloadable release work starts at [release v1](../spec/release/DISTRIBUTION_RELEASE_V1.md), with schemas/producers/validators under `schemas/` and `scripts/distribution-release/`, cases in `tests/{distribution-release,distribution-build-input,source-build-receipt,preassembly-gates}-v1.test.mjs` and `tests/distribution-release-producers.test.mjs`, and `pnpm release:contract`. Published clean-host verification uses [the installation guide](BETA_INSTALLATION.md), `scripts/distribution-release/{run-published-acceptance,run-installed-acceptance,installed-execution,installed-lifecycle}.mjs`, and `tests/release-{published,installed}-acceptance.test.mjs`. For gate changes also run `node --test tests/preflight.test.mjs tests/m0-conformance.test.mjs`.
- Architecture focus: `cargo test --locked -p zryna-architecture`; `cargo run --locked -p zryna -- architecture check`. Windows transaction identity: [filesystem foundation](../crates/zryna-windows-filesystem/README.md), its opaque capability lifecycle, and `cargo test --locked -p zryna-windows-filesystem` on Windows. Standalone-project creation retains the complete stage topology through final validation, then consumes it to close every descendant before the original stage handle performs the no-replace rename. A post-seal commit failure preserves the untrusted stage and reports cleanup category `ZRYNA-C2004`; it does not reopen children for cleanup.
- Gate predicate/timing helpers are imported by those tests. Preserve required checks, exact commands, pins, failure propagation, and security settings; timing headroom is not a performance claim.
- Inspect actual current CI policy before editing: timeout values are intentionally not duplicated here. Finish with full gates and required hosted checks.

## 10. Guides, roadmap/contracts, or website documentation bundles

- Start: [DOCUMENTATION_BUNDLES](DOCUMENTATION_BUNDLES.md), [ROADMAP](ROADMAP.md), and the specific contract being documented. Beta archive prerequisites: [distribution definition](BETA_DISTRIBUTION.md) and [material evidence](BETA_BOOTSTRAP_MATERIALS.md); these do not activate downloadable binaries.
- Cross-target composition: [decision table](../spec/language/CROSS_TARGET_PROFILES_V1.md), [documentation checks](../tests/cross-target-profile-contract.test.mjs), private [entrypoint](../crates/zryna-driver/src/profile_composition/mod.rs), [independent verifier](../crates/zryna-driver/src/profile_composition/verification.rs), and [focused tests](../crates/zryna-driver/src/profile_composition/tests.rs). Run `pnpm docs:check`, `cargo test --locked -p zryna-driver profile_composition`, and driver doc-tests. WIT identities/quotas remain #167's authority; composition grants no source, artifact or host authority.
- Source docs are under `docs/` and `spec/`; export registration is `docs/website-bundle-v1.json`; implementation is `scripts/docs/{bundle,export,check}.mjs`.
- Focus: `pnpm docs:check`; for M3 authority changes also `pnpm m3:contract`. Inspect `tests/docs-bundle.test.mjs` and the relevant contract test.
- Export is an explicit whitelist/provenance operation, not implicit inclusion of every Markdown file. CI artifact success is not evidence of website deployment.
- Keep supported behavior separate from future milestones. Finish with the required contribution gates, not a docs-only substitute.

## 11. M4 WIT worlds or host-capability policy

- Start: [WIT capability profiles v1](../spec/wit/CAPABILITY_PROFILES_V1.md) and the [M4 roadmap](ROADMAP.md).
- WIT source: `spec/wit/capability-profiles-v1/worlds.wit`; machine-readable authority: `tests/wit-capability-profiles-v1.json` and `schemas/zryna-{wit-capability-profiles,capability-request}-v1.schema.json`. Validator: `scripts/wit-capabilities/validate.mjs`; positive, negative, malformed, replay and boundary evidence: `tests/wit-capability-contract.test.mjs` and `tests/wit-capability-v1/fixtures/`.
- Resolved dependency audit: `crates/zryna-backend-webassembly/src/wit_world_audit.rs`; exact source pins: its private `wit_world_audit/pins.rs`; provenance and hostile evidence: `crates/zryna-backend-webassembly/tests/wit-world-audit-v1/` and `tests/wit_world_audit.rs`. Run `cargo test --locked -p zryna-backend-webassembly --test wit_world_audit`.
- Private command self-check source: [architecture and envelopes](WASI_COMMAND_SELF_CHECK_V1.md), backend `src/component_command/` and driver `src/command_runtime/`. Focused filters are `component_command` and `command_runtime` respectively; execution evidence remains pending the required lanes.
- Typed JS/WASM design: [adapter contracts](../spec/interop/JS_WASM_ADAPTERS_V1.md) and [conversion/lifecycle proof plan](../spec/interop/JS_WASM_ADAPTER_CONFORMANCE_V1.md); `tests/js-wasm-adapter-contract.test.mjs` checks design consistency through `pnpm docs:check`, without executing adapters.
- Private scalar ESM interface: `crates/zryna-driver/src/scalar_adapter_interface.rs` binds one authenticated `ControlFlowV1` closure and sealed scalar ABI to exact deterministic ESM bytes plus a pure browser/Node policy; the existing M2 JavaScript preparation path is its first internal consumer. Focus with `cargo test --locked -p zryna-driver scalar_adapter_interface`; this is not loader generation, Component Model output, host-operation admission, or public activation. Private scalar host calls: [consumer](../crates/zryna-driver/src/scalar_adapter_interface/consumer.rs), [shared runtime seam](../crates/zryna-driver/src/pipeline_runtime.rs), and [host conformance lane](../tests/scalar-host/README.md). The browser resource test requires separately reviewed acquisition pins and explicit nonzero execution; static or default tests do not establish cross-host acceptance.
- Browser scalar component profile: [quickstart and manifest](BROWSER_COMPONENT_V1.md), sealed component metadata in `crates/zryna-backend-webassembly/src/scalar_component/`, generated bindings in `crates/zryna-driver/src/browser_component/`, atomic artifact staging in `crates/zryna-driver/src/pipeline/artifacts.rs`, public CLI selection in `apps/zryna/src/main.rs`, and Node/pinned-browser fixture in `tests/browser-component/`. Focus with the `browser_component_` CLI tests, including the separately ignored real-browser test.
- Focus: the resolved-audit test above, `pnpm wit:contract`, then `pnpm docs:check`. Preserve the `specified-only` boundary: component emission, bindings, runtime/CLI activation, cross-target dependency composition and JS/WASM resource adapters are separate work.

## 12. M5 package manifests, lockfiles, resolution, or package-instance identity

- Start: [package and release contract v1](../spec/package/PACKAGE_RELEASE_V1.md), [package-instance identity v1](../spec/package/PACKAGE_INSTANCE_IDENTITY_V1.md), and the [M5 roadmap](ROADMAP.md).
- Canonical schema, serialization, fixtures, and validator are `schemas/zryna-package-release-v1.schema.json`, `scripts/package-release/`, and `tests/package-release-v1/`; #360 semantic decisions are checked by `tests/package-instance-identity-contract.test.mjs`.
- Source-only resolver: `crates/zryna-package`; retained local/Git-cache filesystem capabilities and lock publication: `crates/zryna-driver/src/package_resolution.rs`; CLI dispatch: `apps/zryna/src/package.rs`; user contract: [package resolution](PACKAGE_RESOLUTION.md). Focus with `cargo test --locked -p zryna-package`, `cargo test --locked -p zryna-driver package_resolution`, `pnpm package:contract`, then `pnpm docs:check`. Preserve the source-only boundary: package imports, registry acquisition, build execution, native recipes, and package publication remain separate work.

## 13. M5 resolved build/source trust plans or cache identity

- Start: [resolved build plan v0](../spec/package/RESOLVED_BUILD_PLAN_V0.md), [source trust policy v0](../spec/package/SOURCE_TRUST_V0.md), and the [M5 roadmap](ROADMAP.md).
- Closed source-only shape: `schemas/zryna-resolved-build-plan-v0.schema.json`; validation entrypoint, #168 authority projection, and structural budget checks: `scripts/build-plan/validate.mjs`, `scripts/build-plan/package-authority.mjs`, and `scripts/build-plan/budgets.mjs`.
- Positive, cache-miss, stale-input, wrong-target, missing-library, undeclared-tool, boundary, trust-policy and interrupted-publication evidence: `tests/resolved-build-plan-v0.test.mjs`, `tests/resolved-build-plan-v0/source-only.json`, and `tests/package-source-trust.test.mjs`.
- Implemented pure-source execution: [package build engine](PACKAGE_BUILD_ENGINE.md), `crates/zryna-driver/src/package_build.rs`, canonical planning in `src/package_build/plan.rs`, cache admission/publication in `src/package_build/{cache,publication}.rs`, and hostile tests below `src/package_build/tests/`. The explicit compiler interface consumes only immutable resolver-authenticated source bytes and cannot be selected by package content.
- Focus: `pnpm build-plan:contract`, `pnpm package:contract`, then `pnpm docs:check`. Reuse #168 package/provenance digests and #357 profile composition; preserve #360 package-instance identity, #362 execution/trust policy, driver-owned compilation/link/publication, and the native appendix's pending-#364 status.

## Required completion checks for every route

The focused commands above are editing aids, not submission evidence by themselves. Follow current CONTRIBUTING and the checked gate registries:

```sh
pnpm install --frozen-lockfile
pnpm preflight
pnpm m0:check
```

Use the repository-pinned toolchains. Keep Linux and Windows M0/required hosted checks mandatory before merge. Quick filters omit some expensive/ignored boundaries; retain the complete gate's required ignored-test execution and doctests.
For a custom filter, first inspect `-- --list` and verify actual nonzero matching tests; never report discovery as execution. Record the exact revision, command, exit status, and executed/ignored counts; do not reuse stale binaries as evidence for changed source.

## Keeping this index current

When moving a file or changing an entrypoint, update its route and relative links in the same change.
Recheck the component's manifest registration/dependencies, existing README, neighboring tests, and gate script references.
Prefer stable entry symbols over line numbers and dynamic test counts. Search within the selected component before expanding to callers. This file guides navigation; it grants no new source admission, dependency edge, public profile, or reduced verification requirement.
M3 conformance: [contract](M3_CONFORMANCE.md), [registry](../tests/m3-conformance-v1.json), [candidate corpus](../crates/zryna-driver/src/ownership_commands/conformance.rs), and [commands](../scripts/lib/m3-gates.mjs).
