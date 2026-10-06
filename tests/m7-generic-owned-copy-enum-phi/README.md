# Private finite Copy enum branch proof

Continuing `if` arms can replace mutable whole places whose original type is exactly
`Option<i32>` or `Result<i32,bool>`. Existing Copy capability, typed block parameters
and parallel edge operands transport the complete values. An unchanged arm supplies
its incoming value; a returning arm supplies no edge. Lexical owners finish before
continuation, and retained owner identities, availability, loans and parents agree
exactly. Borrowed roots reject before replacement evaluation.

| Export | `true` | `false` | Observation |
| --- | ---: | ---: | --- |
| `optionSingle` | 11 | 13 | One changed arm, implicit else |
| `optionVariants` | 19 | 23 | Some/None replacement |
| `resultSingle` | 41 | 29 | One changed Result arm |
| `resultVariants` | 31 | 43 | Ok/Err replacement |
| `optionEarly` | 47 | 49 | One direct return, one continuing arm |
| `optionNested` | 59 | 61 | Nested enum replacement |
| `resultRepeated` | 69 | 41 | Repeated replacement |
| `parallel` | 660 | 648 | Four simultaneous enum transports |
| `bothReturn` | 89 | 97 | Two returns, no join |

The parallel observation uses `a+b+b+c+c+c+d+d+d+d`. Independent Option-pair,
Result-pair and simultaneous swaps produce 662/624/626 on the true path and
680/686/718 on the false path. These differ from the fixed correct outcomes;
the rejected earlier summed-score draft remains preserved outside this fixture.
This detects the listed permutations, without claiming general injectivity.

Exact source bytes and v5 DTOs were frozen and validated before producer changes.
Independent source-first `oracle.json` fixes 18 normal observations and 72 allocation
faults per module form. `native-stdout-oracle.json` fixes all 1,194 stdout lines,
including each clean retry. Unicode and empty lexical Strings, retained caller and
opaque callee owners verify exact successful allocation identity and reverse cleanup.
Failed output slots never become cleanup owners. The empty allocation identity is
private harness behavior and does not establish a general empty-String ABI.

One mandatory source/typed-CFG/ownership/runtime seal enters all three emitters.
Emission repeats byte-identically. Each form checks 162 JavaScript observations,
162 core-Wasm observations, 90 native successes and 72 cleanup-before-SIGILL faults.
Native inventory attacks preserve the inherited object gate. This Linux proof does
not establish every runtime trap class or supported Windows generic execution.

Independent hand-authored typed graphs precede source production. They exercise full
canonical key identity, payload/ordinal typing, edge arity, dominance, parallel
transport, retained owners and forged cleanup plans. Same-type swaps and literal
drift can pass typing but fail source authentication. Separate exact-source tests
reject hostile keywords, assignment roles, Unicode byte spans and all four newline
terminators across expression-bearing return, with pristine recovery after attacks.
No arbitrary inactive-payload projection opcode is introduced or assumed.

Sparse captures charge `3+L`; each join parameter charges `m+2+2L`, where `L` is the
complete key length and `m` counts continuing arms. Charges precede reserves and
key clones; full snapshot charges retain all key bytes, availability, loans and
parents. Independent genuine source with 400 enum locals pins Option at 30 accepted
branches versus 31 rejected, and Result at 22 versus 23. Same-variant and asymmetric
Some/None cases have separately fixed credits. The unchanged limits are 1,048,576
state units and 256 parameters, including a 257-place union split 129/128 across
arms. Rejected first-extra cases recover on fresh small inputs. Scalar key costs
and diagnostics remain unchanged.

Run with pinned Rust 1.97.1, Node 22.22.1, frozen dependencies, two jobs/test threads
and an isolated `CARGO_TARGET_DIR`:

```sh
python3 tests/m7-generic-owned-copy-enum-phi/run.py /absolute/disposable/proof-output
```

Original opaque `T`, `Option<T>` and `Result<i32,T>` gain no capability from Copy
specialization, including unused originals. Non-Copy owners, other Copy enum forms,
borrowed replacements and enum loop-header replacement remain excluded. This finite
proof changes no wire opcode, runtime, ABI, provider, dependency or public profile.
Full #416 remains incomplete; public admission and #417 remain separate work.
