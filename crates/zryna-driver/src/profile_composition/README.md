# Private profile composition verification

[`mod.rs`](mod.rs) verifies a bounded fixed instance graph against sealed program/source and
authenticated WIT authorities. Its `verify` entry and `ValidatedComposition` result are private;
this directory is not a public profile selector. The crate-visible
[`compile_pure_command`](command_source.rs) binds one authenticated `I32V1` source instance to
the unchanged command world and an empty request.

## Contracts and ownership

Inputs are explicit graph/profile selections, reservations, host policy and approval claims,
matching authorities, and independently checked summary/witness claims. Success retains the exact
input and authority binding with derived requirements, quotas and witnesses. Revalidation rejects
changed source, graph, world or policy. This proves composition consistency; it does not grant host
capabilities, dispatch a component, or approve execution.

| Private area | Responsibility |
| --- | --- |
| [model.rs](model.rs), [graph.rs](graph.rs) | Closed vocabulary and bounded graph validation |
| [authority.rs](authority.rs) | Source/program/WIT binding |
| [policy.rs](policy.rs), [quota.rs](quota.rs) | Host narrowing, requirements and deduplicated quota accounting |
| [verification.rs](verification.rs) | Independent claim checks and sealed result construction |
| [command_source.rs](command_source.rs) | One source-bound command composition seam |

The driver may depend on frontend, verified IR and backend WIT audit authority. Those lower
components must not depend on driver policy or reconstruct an approval from a digest. Runtime
admission, imported host operations, CLI activation and WIT schema ownership belong elsewhere.
See [cross-target profiles](../../../../spec/language/CROSS_TARGET_PROFILES_V1.md).

## Existing focused tests

```sh
cargo test --locked -p zryna-driver profile_composition::tests
```

The [test root](tests.rs), [rejection tests](tests/rejection.rs),
[graph/resource bounds](tests/bounds.rs), and [quota tests](tests/quotas.rs) cover forged
summaries and authority, replay binding, exact/first-extra limits, and shared-instance accounting.
