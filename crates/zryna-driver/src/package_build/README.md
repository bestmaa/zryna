# Authenticated package builds

The public entry is [`execute_package_build`](../package_build.rs), re-exported by the driver.
It builds an already resolved pure-source package graph in dependency order, authenticates cache
hits through the supplied `PureSourceCompiler`, and publishes complete target output inventories.
It does not turn package metadata into executable authority.

## Contracts and ownership

`PackageBuildRequest` supplies retained `PackageResolutionSuccess`, explicit compiler/profile/
host/target identities, offline or frozen mode, and validated cache/output roots. The compiler
callback receives authenticated source bytes and compiled dependencies; it returns dependency
bytes and declared root outputs. Success retains the plan identity, per-target cache outcomes,
output hashes and sizes, and the package-build manifest path. Invalid plans, source changes,
forged cache bytes, undeclared outputs, or failed publication return `PackageBuildError`.

| Private area | Responsibility |
| --- | --- |
| [plan.rs](plan.rs) | Exact plan projection, identity and target/output validation |
| [cache.rs](cache.rs) | Retained cache root, complete records and byte authentication |
| [staging.rs](staging.rs) | Fixed staged inventory and filesystem identity checks |
| [publication.rs](publication.rs) | Complete create-only outputs and manifest publication |

The driver composes resolver-owned package/source authority with a caller-supplied pure-source
compiler; `zryna-package` remains below this orchestration. Backends must not resolve packages or
read ambient package paths. Network acquisition, package scripts, native recipes, registry
resolution, and a general public package compiler do not belong here. See
[resolved build plans](../../../../spec/package/RESOLVED_BUILD_PLAN_V0.md) and
[source trust](../../../../spec/package/SOURCE_TRUST_V0.md).

## Existing focused tests

```sh
cargo test --locked -p zryna-driver package_build::tests
```

The [test root](tests.rs) includes cold/warm builds and dependency order.
[Graph limits](tests/graph.rs), [hostile cache/filesystem inputs](tests/hostile.rs),
[identity invalidation](tests/invalidation.rs), and [source trust](tests/source_trust.rs)
exercise the owning boundary, including self-consistent cache forgery and source replacement.
