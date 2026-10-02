import assert from 'node:assert/strict';
import { mkdtemp, readFile, rm, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import path from 'node:path';
import test from 'node:test';

import {
  expectedRegistrySha256,
  loadAndValidateM2Contract,
  validateM2Contract,
} from '../scripts/check-m2-contract.mjs';

function clonedContract() {
  return structuredClone(loadAndValidateM2Contract());
}

test('digest-pins the exact planned M2 contract and dependency ledger', () => {
  const contract = loadAndValidateM2Contract();
  assert.equal(expectedRegistrySha256.length, 64);
  assert.equal(contract.status, 'specified-not-implemented');
  assert.deepEqual(
    contract.issues.map(({ number }) => number),
    [45, 46, 47, 48, 49, 50, 51, 52, 53, 54, 55, 56, 57],
  );
  assert.deepEqual(contract.issues.find(({ number }) => number === 55).dependsOn, [47, 51, 52, 54]);
});

test('freezes the complete arithmetic and comparison inventory without an implicit trap ABI', () => {
  const contract = loadAndValidateM2Contract();
  assert.deepEqual(
    contract.operators.map(({ id }) => id),
    [
      'i32-add',
      'i32-sub',
      'i32-mul',
      'i32-neg',
      'equal',
      'not-equal',
      'i32-less-than-s',
      'i32-less-equal-s',
      'i32-greater-than-s',
      'i32-greater-equal-s',
    ],
  );
  assert.deepEqual(contract.trapCodes, []);
  assert.deepEqual(
    contract.plannedCases.filter(({ feature }) => feature === 'comparison').map(({ id }) => id),
    [
      'signed-less-than',
      'signed-less-equal',
      'signed-greater-than',
      'signed-greater-equal',
      'i32-exact-equality',
      'i32-exact-inequality',
      'boolean-exact-equality',
      'boolean-exact-inequality',
    ],
  );
});

test('rejects every class of contract drift including malformed planned records', () => {
  for (const mutate of [
    (contract) => { contract.profile = 'different'; },
    (contract) => { contract.status = 'implemented'; },
    (contract) => { contract.issues[10].dependsOn.pop(); },
    (contract) => { contract.operators[0].behavior = 'target-defined'; },
    (contract) => { contract.trapCodes.push('target-trap'); },
    (contract) => { contract.limits.irBlocksPerFunction += 1; },
    (contract) => { contract.plannedCases.pop(); },
    (contract) => { contract.plannedCases[0].feature = 'not-arithmetic'; },
    (contract) => { contract.plannedCases[0].inputs[0] = 'not-i32'; },
    (contract) => { contract.plannedCases[0].expected.value = 1.5; },
    (contract) => { contract.plannedInvalidCases[0].phase = 'backend'; },
    (contract) => { contract.plannedInvalidCases[0].expectedFamily = 'WRONG'; },
    (contract) => { contract.plannedInvalidCases[0].ownerIssue = 45; },
    (contract) => { contract.unknown = true; },
  ]) {
    const contract = clonedContract();
    mutate(contract);
    assert.throws(() => validateM2Contract(contract), /invalid M2 contract/);
  }
});

test('bounds canonical registry reads and distinguishes integrity from authentication', async () => {
  const directory = await mkdtemp(path.join(tmpdir(), 'zryna-m2-contract-'));
  try {
    const mutated = clonedContract();
    mutated.status = 'implemented';
    const canonicalPath = path.join(directory, 'canonical.json');
    await writeFile(canonicalPath, `${JSON.stringify(mutated, null, 2)}\n`);
    assert.throws(() => loadAndValidateM2Contract(canonicalPath), /registry digest mismatch/);

    const noncanonicalPath = path.join(directory, 'noncanonical.json');
    await writeFile(noncanonicalPath, JSON.stringify(clonedContract()));
    assert.throws(
      () => loadAndValidateM2Contract(noncanonicalPath, { verifyDigest: false }),
      /registry bytes are not canonical JSON/,
    );

    const oversizedPath = path.join(directory, 'oversized.json');
    await writeFile(oversizedPath, Buffer.alloc(1024 * 1024 + 1, 0x20));
    assert.throws(
      () => loadAndValidateM2Contract(oversizedPath, { verifyDigest: false }),
      /registry exceeds one MiB/,
    );
  } finally {
    await rm(directory, { recursive: true, force: true });
  }
});

test('normative specification freezes the implemented explicit profile boundary', async () => {
  const specification = await readFile(
    new URL('../spec/language/CONTROL_FLOW_MODULES_V1.md', import.meta.url),
    'utf8',
  );
  const normalized = specification.replace(/\s+/g, ' ');
  for (const required of [
    'Status: frozen and implemented as the explicit public M2 `control-flow-v1` profile.',
    '`ControlFlowV1`',
    '`--profile control-flow-v1`',
    '`zryna-manifest-v2.json`',
    'This profile has no language-level arithmetic trap.',
    'Division and remainder are deliberately',
    '`module_resolution: false`',
    'The complete resolved function call graph must be acyclic.',
    'The import graph and resolved call graph must both be acyclic.',
    'The TypeScript provider never reads imported files',
    'component-by-component, handle-relative traversal',
    'Every ID in `0..value-count` has exactly one definition',
    'Live mutable bindings at one merge or loop header',
    'Mutable source locals do not survive as target-dependent storage claims.',
    'Its bytes remain immutable historical governance evidence',
    'must add a separately versioned executable registry',
    'has no heap, GC, allocator',
  ]) {
    assert.ok(normalized.includes(required), `missing normative phrase: ${required}`);
  }
});

test('roadmap ledger exactly matches the digest-pinned issue graph and honest status', async () => {
  const [roadmap, architecture, frontends, status, moduleClosure, semantics, controlFlow, javascript, webassembly, nativeMir, nativeBackend] = await Promise.all(
    ['ROADMAP.md', 'ARCHITECTURE.md', 'FRONTENDS.md', 'STATUS.md', 'M2_MODULE_CLOSURE.md', 'M2_STRAIGHT_LINE_SEMANTICS.md', 'M2_CONTROL_FLOW_SEMANTICS.md', 'M2_JAVASCRIPT_BACKEND.md', 'M2_WEBASSEMBLY_BACKEND.md', 'M2_NATIVE_MIR.md', 'M2_NATIVE_BACKEND.md'].map((name) =>
      readFile(new URL(`../docs/${name}`, import.meta.url), 'utf8'),
    ),
  );
  const contract = loadAndValidateM2Contract();
  const rows = [...roadmap.matchAll(/^\| #(\d+) \| ([^|]+) \| ([^|]+) \| (complete|planned|external closure) \|$/gm)]
    .map((match) => ({
      number: Number(match[1]),
      dependsOnText: match[3].trim(),
      state: match[4],
    }))
    .filter(({ number }) => number >= 45 && number <= 57);
  assert.deepEqual(rows.map(({ number }) => number), contract.issues.map(({ number }) => number));
  for (const row of rows) {
    const issue = contract.issues.find(({ number }) => number === row.number);
    const expectedDependencyText = issue.dependsOn.length === 0
      ? 'M1 closure'
      : issue.dependsOn.map((number) => `#${number}`).join(', ');
    assert.equal(row.dependsOnText, expectedDependencyText);
    assert.equal(
      row.state,
      [45, 46, 47, 48, 49, 50, 51, 52, 53, 54, 55, 56].includes(row.number)
        ? 'complete'
        : 'external closure',
    );
  }
  assert.match(
    roadmap,
    /Current status: contract specified,[\s\S]*M2 native object and typed execution implemented[\s\S]*explicit public `control-flow-v1` path[\s\S]*fixed-oracle aggregate three-target M2 conformance/,
  );
  assert.match(roadmap, /digest-pinned planning inventory/);
  assert.match(architecture, /## Isolated `ControlFlowV1` boundary/);
  assert.match(architecture, /M2 JavaScript backend[\s\S]*direct core WebAssembly backend[\s\S]*M2 native MIR profile[\s\S]*M2 native object and typed link\/run boundary/);
  assert.match(frontends, /versioned raw syntax snapshot/);
  assert.match(frontends, /internal module discovery/);
  assert.match(status, /## Implemented M1 slice/);
  assert.match(status, /## Implemented M2 explicit profile/);
  assert.match(status, /M2 deterministic JavaScript backend[\s\S]*M2 direct core WebAssembly backend[\s\S]*byte-deterministic WebAssembly 1\.0[\s\S]*public driver now composes those boundaries[\s\S]*`zryna-manifest-v2\.json`/);
  assert.match(status, /explicit `control-flow-v1` profile is implemented[\s\S]*required fixed-oracle[\s\S]*Linux and Windows `m2` gate/);
  assert.match(status, /Issue #56 adds the[\s\S]*fixed-oracle three-target M2 conformance registry and aggregate required gate/);
  assert.match(status, /Issue #57 records the separate authenticated website import[\s\S]*does not assert that an external website deployment has occurred/);
  assert.match(status, /do not claim heap values[\s\S]*tracing-GC profile[\s\S]*not a general zero-runtime or GC-free guarantee/);
  assert.doesNotMatch(status, /public status remains M1/);
  assert.match(moduleClosure, /exactly one final full-map protocol-v3 snapshot/);
  assert.match(moduleClosure, /ZRYNA-M2-GRAPH\\0/);
  assert.match(moduleClosure, /canonical verbatim form of that same local disk root is also accepted/);
  assert.match(moduleClosure, /UNC, device, drive-relative, and all other verbatim namespaces/);
  assert.match(moduleClosure, /selected only by explicit[\s\S]*`--profile control-flow-v1`/);
  assert.match(moduleClosure, /M2 Linux x86-64 native backend/);
  assert.doesNotMatch(moduleClosure, /Native, manifest v2,[\s\S]*remain gated/);
  assert.match(semantics, /public result contains only[\s\S]*VerifiedProgram/);
  assert.match(semantics, /straight-line foundation is extended by the internal[\s\S]*M2 control-flow semantic boundary/);
  assert.match(semantics, /M2 Linux x86-64 native backend[\s\S]*emits, links, and[\s\S]*executes/);
  assert.doesNotMatch(semantics, /native object\/execution gates remain unavailable/);
  assert.match(controlFlow, /public result contains only[\s\S]*VerifiedProgram/);
  assert.match(controlFlow, /omitted `else` is the exact empty false path/);
  assert.match(controlFlow, /condition of a `while` is evaluated once on every[\s\S]*visit to its header/);
  assert.match(controlFlow, /`while \(true\)` is still treated as[\s\S]*potentially falling through/);
  assert.match(controlFlow, /manifest-v2 transaction[\s\S]*implemented by Issue #55/);
  const normalizedWebAssembly = webassembly.replace(/\s+/g, ' ');
  for (const required of [
    '`zryna_backend_javascript::emit_control_flow`',
    '`Math`',
    '`Number`',
    '`Object`',
    '`I32Mul`',
    '`DirectCall`',
    '`Return`',
    '`Jump`',
    '`Branch`',
    '`condition === true`',
    'parallel SSA edge semantics',
    '32 MiB',
    '`ZRYNA-J2003`',
    'public driver only for explicit',
  ]) {
    assert.ok(javascript.includes(required), `missing M2 JavaScript contract phrase: ${required}`);
  }
  for (const required of [
    '`zryna_backend_webassembly::emit_control_flow`',
    'type, function, export, and code sections',
    '`I32Mul`',
    '`DirectCall`',
    'parallel SSA edge semantics',
    'constant-host-stack-depth dispatcher',
    '32 MiB',
    '`ZRYNA-W2004`',
    '`ZRYNA-W2005`',
    'over standard input to an inline module',
    'public driver only',
  ]) {
    assert.ok(normalizedWebAssembly.includes(required), `missing M2 WebAssembly contract phrase: ${required}`);
  }
  const normalizedNativeMir = nativeMir.replace(/\s+/g, ' ');
  for (const required of [
    'implemented as a separate internal lowering',
    '`zryna_ir::control_flow_v1::VerifiedProgram`',
    'zryna_m2_i_m<module-id-decimal>_f<declaration-index-decimal>',
    '`zryna_v1_e_<logical-name>`',
    'simultaneous SSA transfers',
    '`DirectCall`',
    '`Branch`',
    '`ZRYNA-N2101`',
    '`ZRYNA-N2113`',
    '`ZRYNA-N2201`',
    '`ZRYNA-N2202`',
    'Raw terminator claims per function / program | 4,096 / 65,536',
    'Aggregate direct-call arguments per program | 16,777,216',
    'Aggregate edge arguments per program | 33,554,432',
    'One provisional entry export name | 128 bytes',
    'Aggregate provisional entry export-name bytes | 2,097,152 bytes',
    '31 unit tests and 5 compile-fail doctests',
    'M2 Linux x86-64 native backend',
  ]) {
    assert.ok(normalizedNativeMir.includes(required), `missing M2 native MIR contract phrase: ${required}`);
  }
  assert.match(roadmap, /Issue #53 implements the internal[\s\S]*verified native MIR profile[\s\S]*independent[\s\S]*raw-to-verified CFG/);
  const normalizedNativeBackend = nativeBackend.replace(/\s+/g, ' ');
  for (const required of [
    '`ValidatedControlFlowNativeObjectArtifact`',
    '`R_X86_64_PLT32`',
    '`PltRelative`',
    '`X86Branch`',
    'explicit addend `-4`',
    '`ZRYNA-N3102`',
    '`ZRYNA-N3103`',
    'artifact-bound scalar ABI',
    'device/inode identity',
    'Issue #55 composes this boundary through public profile selection',
  ]) {
    assert.ok(normalizedNativeBackend.includes(required), `missing M2 native backend phrase: ${required}`);
  }
  assert.match(status, /M2 verified native MIR profile[\s\S]*M2 Linux x86-64 native backend[\s\S]*artifact-bound scalar ABI/);
  assert.doesNotMatch(status, /M2 executable slice is implemented/);
});

test('package exposes one focused digest-pinned contract check', async () => {
  const packageDocument = JSON.parse(await readFile(new URL('../package.json', import.meta.url), 'utf8'));
  assert.equal(packageDocument.scripts['m2:contract'], 'node scripts/check-m2-contract.mjs');
  assert.equal(
    packageDocument.scripts['docs:check'],
    'node --test tests/docs-bundle.test.mjs tests/m2-contract.test.mjs tests/m2-manifest-contract.test.mjs tests/m3-contract.test.mjs tests/m3-public-docs.test.mjs tests/m7-generic-spec-fixtures.test.mjs tests/m7-generic-instance-spec-fixtures.test.mjs tests/m7-generic-review-spec.test.mjs tests/m7-generic-resource-spec.test.mjs tests/m7-generic-owned-layout-spec.test.mjs tests/m7-generic-key-spec.test.mjs',
  );
});
