# Native C v0 isolated prototype

This is the disposable prototype stage for #417 under the integrated
[ABI](../../spec/abi/NATIVE_C_INTEROP_V0.md),
[source](../../spec/abi/NATIVE_C_INTEROP_V0_SOURCE.md),
[review](../../spec/abi/NATIVE_C_INTEROP_V0_REVIEW.md) and
[acceptance](../../spec/abi/NATIVE_C_INTEROP_V0_ACCEPTANCE.md) contracts.
It does not satisfy #417's complete acceptance or establish public support.

## Source and declaration projection

`admission.mjs` composes the existing independent sidecar design checker with
`declarations.mjs`. The latter authenticates the exact supplied source set,
parses complete functions and matches every primitive and import/export binding
against actual UTF-8 byte spans. A digest or a call-looking substring inside a
comment cannot supply parsed-source correspondence.

`source.mjs` accepts the fixed prototype grammar: explicit functions and types,
const locals, scalar values and addition, direct reserved intrinsics, returns,
and immediate `if (status !== 0)` terminal foreign-error guards. This is a
restricted prototype grammar, not the complete selected-language grammar.
Additional valid forms need a separate implementation; unsupported forms reject
the complete prototype input. Foreign token aliases, ordinary direct calls,
loops, arbitrary conditionals and cross-function foreign tokens are unavailable.

`resources.mjs` checks the fixed source slice's exact ABI argument types,
byte/count pairing, fresh output slots, status dominance, acquisition identity,
nominal allocator/release pairing and linear consumption. It records symbolic
prechecks and reverse cleanup plans. Unknown dynamic statuses remain
`HostAbiFailure`; only explicitly declared nonzero statuses may become
`ForeignError`. The canonical read fixture has no recoverable status.

These JavaScript records are mutable test observations. They are not sealed
syntax, semantics, IR, MIR, runtime-layout or driver link authority. They execute
no wrapper, acquisition, copy or release. The separate Rust implementation must
independently authenticate and verify its inputs before producing any authority.
Existing M3 authorities and their vocabulary remain unchanged.

Run the dependency-free source projection tests with the pinned Node 22.22.1:

```sh
node --test tests/native-c-prototype.test.mjs
```

The complete `admission.mjs` composition additionally requires the repository's
existing locked Ajv dependency and its declaration-design tests. Passing source
projection alone is not a pass for that composition.

## Tiny C fixture

`fixture.c` implements the seven exact imported operations from the unchanged
reviewed `candidate.h`. Wrapping addition avoids signed overflow and out-of-range
signed conversion. The handle allocator checks negative seeds before allocation;
allocation failure returns status 2 without a new allocation or output write.
Owned bytes use a distinct allocation/release kind and canonical `(NULL,0)` empty
results. Raw input precondition violations abort the test process instead of
inventing a recoverable C status.

`fixture_control.h` exposes test-only allocation injection, output-write counters
and acquisition-number release traces. Those instrumentation symbols are outside
the foreign declaration set and are not an accepted production link surface.
The raw harness counts writes separately from sentinel equality, preserves input
bytes and checks accepted-prefix reverse releases. Its explicit C cleanup does
not prove generated wrapper cleanup or a verifier's exactly-once obligation.

After obtaining a coordinated execution slot, compile only on Linux x86-64 LP64
with the pinned accepted GCC toolchain and private evidence output paths:

```sh
gcc -std=c11 -Wall -Wextra -Werror -pedantic -O1 -g \
  -fsanitize=address,undefined -fno-omit-frame-pointer \
  tests/native-c-prototype/fixture.c tests/native-c-prototype/fixture_test.c \
  -o /private/evidence/native-c-fixture
ASAN_OPTIONS=detect_leaks=1 /private/evidence/native-c-fixture
gcc -std=c11 -Wall -Wextra -Werror -pedantic -fsyntax-only \
  tests/native-c-abi-v0/header_contract.c
```

Run all eleven intentional `REJECT_*` header variants from the acceptance
contract as expected compilation failures. Output binaries and receipts belong
outside the source tree; test instrumentation is not a published artifact.

## Reverse client and remaining acceptance

`scalarPrototypeHeader` emits a disposable exact scalar C header from checked
declaration records. `reverse_client.c` requires that header and a real Zryna
export object; there is no stand-in C implementation of the export. Header-only
or raw-fixture success cannot be reported as a Zryna reverse-client pass.

Independent Rust declaration, raw IR and raw native MIR rejection, accepted
layout/ownership composition, audited objects and linker inputs, generated raw
bindings, executed safe wrappers, malformed-result conditional release, cleanup
faults, resource reservation, unsupported selections, process-fault observations,
sanitizers and complete exact-revision Linux/Windows contribution gates remain
required. SQLite and Rust-through-C-shim retain separate library proofs. Public
activation remains a separate reviewed stage.
