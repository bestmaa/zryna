# Private generic scalar branch proof

This fixture exercises the inherited assignment rules inside the private #416 owned
execution lane. A continuing `if` arm may update a mutable concrete `bool` or `i32`
place. The join passes each changed place through an ordinary typed block parameter.
An arm that returns contributes no join edge; an unchanged arm passes the incoming
value. Nested branches, repeated branches and branches inside a scalar `while` use
the same edge transport. For example:

```zryna
let code:i32=3;
if(flag) { code=7; } else { code=9; }
return Result.err<T,i32>(code);
```

The opaque generic owner `T` retains its original ownership obligation. All continuing
arms must agree exactly on owner and loan state. Opaque `T`, String, owned Option/Result
and borrowed-place replacements do not acquire an implicit owner phi or clone.

| Export | `true` | `false` | Join behavior |
| --- | ---: | ---: | --- |
| `single` | 11 | 13 | One changed arm, implicit else |
| `double` | 7 | 9 | Both arms replace i32 |
| `early` | 17 | 19 | One arm returns directly |
| `nested` | 24 | 29 | bool/i32 join, then another branch |
| `looped` | 1 | 5 | Nested branches and loop-carried scalar state |
| `optional` | 31 | 41 | Scalar join before Some/None return and matching |

`oracle.json` fixes these twelve outcomes and their actual allocation/release order
before emission. Every reachable allocation site has a status-1 failure and a clean
retry: 35 failures per module form. The six paths use Unicode and empty branch-local
Strings, a retained caller String and an unused opaque callee owner. Failed output
slots never become cleanup owners. Successes and faults verify exactly once release.

The frozen v5 syntax snapshots bind the exact single-file and imported source bytes.
One mandatory source/typed-CFG/ownership/runtime seal enters the JS, core-Wasm and
Linux x86-64 native emitters. The harness checks fixed observations, real runtime
allocation identities and cleanup before native SIGILL. It does not establish all
inherited trap classes or supported Windows generic execution.

Run with the repository's pinned Rust 1.97.1 and Node 22.22.1, frozen dependencies,
`CARGO_BUILD_JOBS=2`, `RUST_TEST_THREADS=2`, and an isolated `CARGO_TARGET_DIR`:

```sh
python3 tests/m7-generic-owned-scalar-phi/run.py /absolute/disposable/proof-output
```

Independent semantic tests pin 256 changed join parameters versus the first extra
257, including a union split across arms. A 400-local genuine-source fixture pins
the unchanged aggregate source-state ceiling at 94 accepted branches versus 95
rejected branches. A separate hand-authored IR graph verifies edge arity, types,
dominance and retained-owner cleanup without consulting source lowering.

This proof changes no opcode, wire domain, runtime import, header, scalar ABI,
dependency, provider or public profile. Full #416 acceptance remains incomplete;
public admission and native C interop under #417 are separate work.
