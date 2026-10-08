# Contributing

Zryna accepts focused changes that preserve the architecture contract and accurately describe implemented behavior.

Use the [task-oriented code navigation map](docs/CODE_NAVIGATION.md) to find component entrypoints and focused tests.

## Choose and coordinate work

Search existing issues and pull requests before starting. Read the entire issue, its prerequisites,
acceptance criteria and linked contracts; a parent milestone is not permission to change every
component beneath it. Check whether another contributor is already working on the same behavior
or files.

To claim an existing issue, comment with the proposed scope, relevant components and intended
verification, and ask for assignment or confirmation. Coordinate overlapping work with the
maintainer and current contributor before implementation. Keep the issue updated if the scope
changes, a prerequisite blocks progress, or you can no longer continue. Do not open a duplicate
implementation simply because an existing claim has no recent update; ask about its status.

### Bugs and features

For a bug report, use the existing [implementation task form](.github/ISSUE_TEMPLATE/implementation.yml).
Include a minimal reproducer, expected and actual behavior, exact revision, platform/toolchain,
command and relevant diagnostics. Distinguish a regression in supported behavior from an
unimplemented language or target feature. Report suspected vulnerabilities through the
[private security reporting channel](https://github.com/zryna/zryna/security/advisories/new), not
with a public reproducer.

A bug fix should restore the documented contract and add a regression test that fails without
the fix. Explain why the failure occurs and keep unrelated cleanup separate. If the proposed fix
changes syntax, semantics, ABI, public behavior, dependency direction or a security boundary,
discuss that contract change before implementing it.

For a feature or architectural change, propose the behavior and obtain maintainer agreement on
scope and dependencies before substantial implementation. Use the same issue form to identify
the owning component, exclusions, observable acceptance criteria, verification and documentation
impact. A feature proposal is not approved merely because an issue exists. Do not narrow an
accepted issue's requirements to fit a partial implementation; agree on explicit dependent work
and leave the enclosing acceptance open where necessary.

## Prepare and scope the change

Before changing a component:

1. read `README.md`, `docs/ARCHITECTURE.md`, and `docs/STRICT_WORKSPACE.md`;
2. confirm that the component is registered in `zryna.workspace.json`;
3. preserve the declared dependency direction;
4. add stable diagnostics and tests for rejected input;
5. avoid claims for unsupported compiler or platform behavior.

Also read applicable scoped instructions and the component's README. Use the repository-pinned
toolchains and setup requirements in [README.md](README.md), rather than updating pins to match
your local installation.

Start a focused branch from the agreed base, normally current `main`, with a descriptive topic
such as `fix/index-bounds` or `feat/indexed-access`. Keep one coherent issue or agreed stage per
pull request. Identify the files you expect to change and coordinate shared-file edits before
expanding that set. Preserve unrelated work and avoid repository-wide formatting, mechanical
rewrites or dependency updates in a behavioral fix.

Prefer small, cohesive modules with explicit ownership and private interfaces. New production
source files have a hard 300-module-line limit. Physical blank and comment lines count; JS/TS
also counts pinned printer output and meaningful statements/bindings so compression cannot hide size. Existing larger files are recorded at authenticated, frozen migration ceilings and must
not grow; reductions in the trusted base lower the ceiling. Run `pnpm structure:check` during
development. Mixed production/test files count in full. This does not limit user-authored `.zry`
programs. Use `pnpm build` for normal repository compilation: it checks source size and Rust formatting,
fetches locked dependencies, then enforces architecture boundaries before the locked workspace build. Bare `cargo build` does
not run the repository-wide enforcement pipeline.
The exact coverage, trusted-base selection and exception schema are documented in
[Strict workspace](docs/STRICT_WORKSPACE.md#source-size-and-navigation-policy).

Historical physical-size exceptions cannot override the 300-line module limit or raise frozen
migration ceilings. Their records require an exact path, responsible owner/team, concrete reason, numeric ceiling,
review reference and UTC expiry date in `scripts/repository-structure-policy.json`. Classifications,
baseline and exceptions require explicit maintainer review; checker notices do not prove that
approval occurred. Do not use minification, compressed formatting, arbitrary splitting, duplicated
helpers, unnecessary public API or weakened visibility to satisfy a size ceiling. Split by cohesive
responsibility and retain existing formatter/lint requirements. Update navigation when moving
its declared targets; not every private helper needs a navigation entry.

New components must be created through the planned canonical creation command once it is available.
Until then, component additions require a focused architecture proposal and simultaneous updates
to both workspace manifests, documentation, tests, and CI. An unregistered component or undeclared
dependency is not an acceptable temporary workaround.

## Develop and verify

During development, choose the smallest relevant tests from the navigation map and inspect the
existing test names before using a filter. For example:

```bash
cargo test --locked -p zryna-semantics -- --list
cargo test --locked -p zryna-semantics data_ownership_v1
pnpm m3:owned:quick
pnpm m2:quick
pnpm docs:check
```

These are examples for different components, not a requirement to run every quick lane after
every edit. Confirm that a filtered run actually executes the intended nonzero tests; listing
tests is discovery, not execution. Add positive and negative evidence at the owning boundary.
Changes to a verifier or trust boundary need independent malformed-input rejection tests, not
only inputs accepted by the matching producer. Preserve stable diagnostic codes and source spans,
resource-boundary checks, cleanup/retention proofs and deterministic recovery where applicable.

Run every required check before submitting a change:

```bash
pnpm install --frozen-lockfile
cargo fetch --locked
pnpm preflight
pnpm m0:check
```

Finish the approved local checks and batch related fixes before publishing one review candidate.
Avoid publishing intermediate revisions only to use hosted checks as an edit loop. The manual CI
dispatch remains available when maintainers deliberately need the complete required and contract
matrix; supply its `structure_base` input with the full trusted ancestor commit. Generated
next-channel documentation bundles are retained for seven days because each
main-branch publication supersedes the preceding candidate; release artifacts follow their
separate release retention policy.

Run `pnpm preflight` during the edit loop. It stops at the first portable contract, formatting,
architecture, workspace-check, frontend, or syntax failure and normally reuses warm local build state. It is a
fast diagnostic gate, not a substitute for the complete Linux and Windows `pnpm m0:check` proof
required before merge.

The canonical M0 gate includes locked Rust dependency fetching, architecture validation,
formatting, strict Clippy, workspace tests and doc-tests, warning-free rustdoc, adapter and protocol
checks, and the conformance-registry self-tests on both supported CI operating systems.

Run additional full gates required by the issue and affected contracts, including `pnpm m2:check`
for required M2 cross-target regression proof. Focused semantic, IR, layout or runtime-ABI work
must retain its applicable complete suites and required ignored/resource tests. Quick commands
and filters deliberately omit some expensive checks; they do not waive those proofs. Do not add
skip switches, weaken limits or suppress failing checks to obtain a passing result.

Report the exact revision, commands, exit status, executed/ignored counts and platform for tests
actually run. Clearly distinguish a pass, failure, skipped test and a check not run. Explain any
environment blocker; do not report another revision's binaries, a test listing or an unrelated
CI run as verification of the current patch. Required Linux and Windows hosted checks must pass
before merge; local success on one platform is not a portability claim.

## Documentation, review and integration

Update affected contracts, component documentation, diagnostic expectations and navigation in the
same change. Keep implemented source behavior, verified compiler obligations and executed target
behavior distinct. An internal IR capability does not establish runtime, backend, CLI or public
profile support. Follow the existing documentation export rules rather than assuming every new
Markdown file belongs in the website bundle.

Use the [pull request template](.github/pull_request_template.md). Link the issue, summarize the
problem and solution, explain component/dependency ownership, and state compatibility, security
and documentation effects. Include only observed verification results and identify anything
still outstanding. Use a closing reference only when the change satisfies the whole referenced
issue; otherwise describe the completed portion and remaining dependency explicitly.

Review your diff before requesting review: remove accidental files and unrelated changes, check
that tests exercise the intended failure or behavior, and ensure documentation matches the patch.
A draft pull request may be used for early feedback, with unfinished work and missing checks
clearly marked; it is not ready for integration until the required evidence is complete.

Respond to review with concrete changes or an explanation of the tradeoff. Coordinate revisions
to shared contracts and keep the branch compatible with its agreed base. After resolving merge
conflicts or changing reviewed code, rerun the affected checks and required final gates on the
updated revision. Maintainer review and all required checks govern integration; an open pull
request or passing focused test does not authorize merge or issue closure.
