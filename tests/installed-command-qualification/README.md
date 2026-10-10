# Installed H1 qualification

Run with pinned Rust 1.97.1, Node 22.22.1 and frozen dependencies. Set
`ZRYNA_QUALIFICATION_NODE_ROOT` to the extracted exact upstream Node directory (including
LICENSE), and `CARGO_TARGET_DIR` to an external disposable build directory. Use jobs 2.

```sh
CARGO_NET_OFFLINE=true node tests/installed-command-qualification/run.mjs /absolute/external/evidence
```

The harness actually executes the architecture command, hashes real tool/source inputs,
checks pinned Node/npm/Rust material bytes and the exact candidate Cargo.lock/runtime closure,
then compiles a build-bound CLI through normal payload preparation and verifies its archive
with the unchanged production content verifier. The H1 candidate binding pins all four source
inputs, the existing recipe digest and unchanged Rust material record. It is selected only in
main-candidate context; all tagged and predecessor release lock pins remain intact.
It extracts and relocates that complete candidate,
then executes positive/negative H1 cases from an unrelated directory with empty PATH and no
inherited compiler/runtime overrides. Its one-file project contains no repository marker,
package manifest or lockfile. It checks typed outcomes, grant counts, component/manifest hashes,
private value omission, override and dependency refusal, unchanged existing output and
installation/provider/runtime tamper rejection followed by recovery.

This is an optimized **content-verified review candidate**, not an approved release or signed
archive. It records the actual observed branch/head/tree separately from the installation
wire's intended-main compatibility reference. It asserts no protected-main membership,
release recipe execution, reproduction, signature authentication or publication authority.
Archive content verification establishes neither production assembly nor release admission.
No protected workflow context or successful gate receipt is fabricated or submitted.
Production admission remains forbidden. Linux foreign-owner eligibility and local Windows
validation remain separate; this harness cannot turn an unavailable owner fixture into proof.

The workflow runs the exact candidate on Ubuntu 24.04 and Windows Server 2022 and uploads
receipts plus every actual invocation. Interrupted runs have no successful qualification receipt.
