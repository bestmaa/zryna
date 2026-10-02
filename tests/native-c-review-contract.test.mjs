import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { readFile } from 'node:fs/promises';
import test from 'node:test';
import './native-c-declaration-design-cases.mjs';

const read = name => readFile(new URL(`../${name}`, import.meta.url), 'utf8');
const vectors = JSON.parse(await read('tests/native-c-abi-v0/review-vectors.json'));
const review = await read('spec/abi/NATIVE_C_INTEROP_V0_REVIEW.md');
const header = await read('tests/native-c-abi-v0/candidate.h');
const acceptance = await read('spec/abi/NATIVE_C_INTEROP_V0_ACCEPTANCE.md');
const abi = await read('spec/abi/NATIVE_C_INTEROP_V0.md');
const sourceContract = await read('spec/abi/NATIVE_C_INTEROP_V0_SOURCE.md');
const digest = wire => createHash('sha256')
  .update('ZRYNA-NATIVE-C-DECLARATION-V0\0', 'ascii').update(wire).digest('hex');

test('native C review identity pins exact bytes and keeps ordered and typed inputs distinct', () => {
  const wire = '{"abi":"zryna-native-c-interop-v0","convention":"sysv-amd64-c-v0",'
    + '"direction":"import","library":"fixture-c-v0","parameters":["c-i32","c-i32"],'
    + '"result":"c-i32","symbol":"add","target":"x86_64-unknown-linux-gnu","version":0}\n';
  assert.equal(vectors.identityExample.wire, wire);
  assert.equal(digest(wire), '3576a6ed8e30f3859d35e624c2d33699f473cfdb30b05a0d7513fcadb04c5cfa');
  assert.equal(vectors.identityExample.sha256, digest(wire));
  for (const changed of [wire.slice(0, -1), wire.replace('fixture-c-v0', 'fixture-c-v1'),
    wire.replace('"import"', '"export"'), wire.replace('"c-i32"', '"c-int"'),
    wire.replace('unknown-linux-gnu', 'pc-windows-msvc'), wire.replace('"version":0', '"version":1')]) {
    assert.notEqual(digest(changed), digest(wire));
  }
  assert.notEqual(digest('["bytes-in","count"]\n'), digest('["count","bytes-in"]\n'));
});

test('fixed operation inventory covers every inert header declaration and exact raw signature', () => {
  const signatures = [
    ['add', ['c-i32', 'c-i32'], 'c-i32'],
    ['sum_bytes', ['bytes-in', 'count', 'i32-out'], 'c-i32'],
    ['fixture_open', ['c-i32', 'handle-out'], 'c-i32'],
    ['fixture_read', ['handle-in', 'i32-out'], 'c-i32'],
    ['fixture_close', ['handle-in'], 'unit'],
    ['fixture_copy_bytes', ['bytes-in', 'count', 'bytes-owned-out', 'count-out'], 'c-i32'],
    ['fixture_release_bytes', ['bytes-release'], 'unit'],
    ['zryna_c_v0_e_add', ['c-i32', 'c-i32'], 'c-i32'],
  ];
  assert.deepEqual(vectors.operations.map(op => [op.symbol, op.parameters, op.result]), signatures);
  assert.deepEqual([...header.matchAll(/^(?:int32_t|void) (\w+)\(/gm)].map(match => match[1]),
    signatures.map(row => row[0]));
  assert.deepEqual(vectors.operations.map(op => op.direction),
    ['import', 'import', 'import', 'import', 'import', 'import', 'import', 'export']);
  for (const [symbol] of signatures) assert.ok(acceptance.includes(`\`${symbol}(`), symbol);
});

test('fixed signed addition and bounded byte sum use independent mathematical oracles', () => {
  assert.equal(vectors.wrapping.length, 5);
  for (const row of vectors.wrapping) {
    const modulo = ((BigInt(row.left) + BigInt(row.right)) % (2n ** 32n) + 2n ** 32n) % (2n ** 32n);
    const signed = modulo > 2147483647n ? modulo - 2n ** 32n : modulo;
    assert.equal(signed, BigInt(row.result));
  }
  assert.equal(4096n * 255n, 1044480n);
  assert.ok(1044480n < 2147483647n);
});

test('contract freezes every specified exact and first-extra resource axis', () => {
  const exact = [1048576, 16, 16, 256, 16, 8, 128, 115, 257, 256, 4096, 65536,
    256, 4096, 16, 16, 16, 4096, 64, 256];
  assert.deepEqual(vectors.bounds.map(row => row.exact), exact);
  assert.equal(new Set(vectors.bounds.map(row => row.metric)).size, exact.length);
  for (const row of vectors.bounds) {
    assert.equal(row.firstExtra, row.exact + 1);
    assert.ok(review.includes(String(row.exact)), row.metric);
    assert.ok(review.includes(String(row.firstExtra)), row.metric);
  }
  assert.equal(Buffer.byteLength('é'.repeat(32768)), 65536);
  assert.equal(Buffer.byteLength('é'.repeat(32768) + 'a'), 65537);
  assert.equal(Buffer.byteLength('zryna_c_v0_e_' + 'a'.repeat(115)), 128);
  assert.equal(Buffer.byteLength('zryna_c_v0_e_' + 'a'.repeat(116)), 129);
});

test('review vectors retain separate failure, syntax and ambiguity requirements', () => {
  const ids = vectors.cases.map(row => row.id);
  assert.equal(ids.length, 30);
  assert.equal(new Set(ids).size, ids.length);
  for (const id of ['scalar-export', 'sum-first-extra', 'wrapper-first-extra',
    'bytes-allocation-failure', 'copy-trap', 'malformed-releasable', 'malformed-unproved',
    'wrong-library', 'wrong-kind', 'double-release', 'prefix-cleanup', 'release-fault',
    'transitive-native', 'host-fault', 'hostile-ir']) assert.ok(ids.includes(id), id);
  assert.equal(vectors.syntaxExclusions.length, 23);
  assert.equal(new Set(vectors.syntaxExclusions).size, 23);
  assert.deepEqual(vectors.ambiguities.map(row => row.id), ['typedef', 'symbol-case',
    'library-alias', 'duplicate-key', 'parameter-order', 'raw-versus-wrapper',
    'private-layout', 'buffer-handle-export']);
  assert.equal(vectors.status, 'reference-only-not-executed');
  for (let decision = 1; decision <= 6; decision += 1) assert.ok(review.includes(`D${decision}`));
  for (const document of [review, acceptance, abi, sourceContract]) {
    assert.match(document, /specified-only normative future\s+contract/);
    assert.match(document, /normal\s+integration/);
  }
  assert.match(review, /not historical sign-off or executed conformance/);
});
