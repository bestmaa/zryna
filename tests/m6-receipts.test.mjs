import assert from 'node:assert/strict';
import test from 'node:test';
import { createGateReceipt, gateObservation, retainedDescriptor,
  validateGateReceipt, validateRetainedReceipts } from '../scripts/m6/receipts.mjs';
import { retainedFixture, encode, hash } from './m6/fixture.mjs';

function reseal(record, carriers, index, transform) {
  const gate = record.gates[index];
  const carrier = carriers.find(row => row.path === gate.receipt.path);
  const value = JSON.parse(carrier.data.toString());
  transform(value);
  carrier.data = encode(value);
  gate.receipt.bytes = carrier.data.length;
  gate.receipt.sha256 = hash(carrier.data);
  return value;
}

function rejectReceipt(transform, pattern = /./) {
  const { record, carriers } = retainedFixture();
  reseal(record, carriers, 0, transform);
  assert.throws(() => validateRetainedReceipts(record, carriers), pattern);
}

test('every complete synthetic receipt consumes its exact input and output byte inventory', () => {
  const { record, carriers, values } = retainedFixture();
  const receipts = validateRetainedReceipts(record, carriers);
  assert.equal(receipts.length, 23);
  assert.equal(carriers.length, 92);
  assert.deepEqual(JSON.parse(JSON.stringify(receipts)), values);
  assert.equal(receipts[0].execution.stderr.bytes, 0);
});

test('assembly preserves supplied observations, materials and execution without creating results', () => {
  const { record, values } = retainedFixture();
  const { integration, observation, materials, execution } = values[0];
  const input = createGateReceipt({ integration, observation, materials, execution });
  assert.deepEqual(JSON.parse(input.toString()), values[0]);
  assert.deepEqual(gateObservation(record.gates[0]), observation);
});

test('missing gate receipt and missing retained input or empty stderr fail closed', () => {
  for (const path of ['evidence/project-linux.json', 'retained/project-linux/input.txt', 'retained/project-linux/stderr.txt']) {
    const { record, carriers } = retainedFixture();
    assert.throws(() => validateRetainedReceipts(record, carriers.filter(row => row.path !== path)), /M6-RETAINED/);
  }
});

test('same-size input or output substitution cannot borrow the unchanged receipt digest', () => {
  for (const suffix of ['input.txt', 'stdout.txt']) {
    const { record, carriers } = retainedFixture();
    const carrier = carriers.find(row => row.path === `retained/project-linux/${suffix}`);
    carrier.data = Buffer.from(carrier.data);
    carrier.data[0] ^= 1;
    assert.throws(() => validateRetainedReceipts(record, carriers), /M6-RETAINED-DIGEST/);
  }
});

test('truncation and a nonempty substitution for retained empty stderr are detected', () => {
  const { record, carriers } = retainedFixture();
  carriers.find(row => row.path === 'retained/project-linux/input.txt').data = Buffer.from('short');
  assert.throws(() => validateRetainedReceipts(record, carriers), /M6-RETAINED-DIGEST/);
  const next = retainedFixture();
  next.carriers.find(row => row.path === 'retained/project-linux/stderr.txt').data = Buffer.from('x');
  assert.throws(() => validateRetainedReceipts(next.record, next.carriers), /M6-RETAINED-DIGEST/);
});

test('gate receipt substitution is detected before parsing or observing its metadata', () => {
  const { record, carriers } = retainedFixture();
  const carrier = carriers.find(row => row.path === 'evidence/project-linux.json');
  carrier.data = Buffer.from(carrier.data);
  carrier.data[0] ^= 1;
  assert.throws(() => validateRetainedReceipts(record, carriers), /M6-RECEIPT-DIGEST/);
});

test('resealed receipt cannot substitute integration or any independently recorded observation', () => {
  for (const change of [v => { v.integration.tree = '3'.repeat(40); },
    v => { v.observation.command = ['other-command']; }, v => { v.observation.environment.runtime = 'other'; },
    v => { v.observation.cases[0].passed++; }, v => { v.observation.id = 'project-windows'; },
    v => { v.observation.finishedAt = '2026-10-02T00:00:02.000Z'; }]) {
    rejectReceipt(change, /M6-RECEIPT-OBSERVATION/);
  }
});

test('resealed receipt binds exact material order, original source, version and binary digest', () => {
  for (const change of [v => { v.materials.pop(); }, v => { v.materials.reverse(); },
    v => { v.materials[0].version = '9.9.9'; }, v => { v.materials[0].sha256 = hash('other'); },
    v => { v.materials[0].source.revision = `${'3'.repeat(40)}:${'2'.repeat(40)}`; },
    v => { v.materials[0].url += '-other'; }]) rejectReceipt(change, /M6-RECEIPT-MATERIALS/);
});

test('receipt format, execution and retained descriptors have closed field sets', () => {
  for (const change of [v => { v.version = 2; }, v => { v.format = 'unknown'; },
    v => { v.extra = true; }, v => { v.execution.extra = true; },
    v => { delete v.execution.profile; }, v => { v.execution.inputs[0].extra = true; }]) rejectReceipt(change);
});

test('cross-gate retained paths cannot escape their declared receipt namespace', () => {
  for (const path of ['retained/project-windows/input.txt', 'retained/project-linux-other/input.txt',
    '../outside', '/outside', 'retained/project-linux/../outside']) {
    rejectReceipt(v => { v.execution.inputs[0].path = path; });
  }
});

test('receipt retained paths reject duplicate, case and file-directory aliasing', () => {
  for (const path of ['retained/project-linux/input.txt', 'retained/project-linux/Input.txt',
    'retained/project-linux/input.txt/child']) {
    rejectReceipt(v => { v.execution.stdout.path = path; });
  }
});

test('unreferenced, duplicated, case-alias and parent-alias captured carriers reject', () => {
  for (const path of ['retained/project-linux/extra.txt', 'retained/project-linux/input.txt',
    'retained/project-linux/Input.txt', 'retained/project-linux/input.txt/child']) {
    const { record, carriers } = retainedFixture();
    carriers.push({ path, data: Buffer.from('x') });
    assert.throws(() => validateRetainedReceipts(record, carriers));
  }
});

test('retained carrier shapes and finite inventory/byte budgets reject malformed buffers', () => {
  const { record, carriers } = retainedFixture();
  assert.throws(() => validateRetainedReceipts(record, null), /M6-RETAINED-INVENTORY/);
  assert.throws(() => validateRetainedReceipts(record, []), /M6-RETAINED-INVENTORY/);
  assert.throws(() => validateRetainedReceipts(record, Array(257).fill(carriers[0])), /M6-RETAINED-INVENTORY/);
  for (const data of ['not-bytes', new ArrayBuffer(2), Buffer.alloc(262145)]) {
    const next = retainedFixture();
    next.carriers[0].data = data;
    assert.throws(() => validateRetainedReceipts(next.record, next.carriers), /M6-RETAINED-INVENTORY/);
  }
  const next = retainedFixture();
  next.carriers[0].extra = true;
  assert.throws(() => validateRetainedReceipts(next.record, next.carriers), /SHAPE/);
});

test('descriptor endpoint budgets allow empty output but reject overflow and unsafe counts', () => {
  for (const bytes of [0, 262144]) assert.doesNotThrow(() => retainedDescriptor(
    { path: 'retained/project-linux/file', bytes, sha256: hash('synthetic') }, 'retained/project-linux/'));
  for (const bytes of [-1, 262145, Number.MAX_SAFE_INTEGER + 1]) {
    assert.throws(() => retainedDescriptor({ path: 'retained/project-linux/file', bytes,
      sha256: hash('synthetic') }, 'retained/project-linux/'), /M6-RETAINED/);
  }
  for (const inputs of [[], Array(65).fill({})]) rejectReceipt(v => { v.execution.inputs = inputs; }, /M6-RECEIPT-EXECUTION/);
});

test('a referenced finite inventory cannot exceed the aggregate retained-byte budget', () => {
  for (const count of [63, 64]) {
    const { record, carriers } = retainedFixture();
    const data = Buffer.alloc(262144, 120);
    const descriptors = Array.from({ length: count }, (_, index) => ({
      path: `retained/project-linux/large-${index}.bin`, bytes: data.length, sha256: hash(data),
    }));
    carriers.splice(carriers.findIndex(row => row.path === 'retained/project-linux/input.txt'), 1);
    carriers.push(...descriptors.map(row => ({ path: row.path, data })));
    reseal(record, carriers, 0, v => {
      v.execution.inputs = descriptors;
      v.execution.corpusSha256 = descriptors[0].sha256;
    });
    if (count === 63) assert.doesNotThrow(() => validateRetainedReceipts(record, carriers));
    else assert.throws(() => validateRetainedReceipts(record, carriers), /M6-RETAINED-INVENTORY/);
  }
});

test('bounded execution profile, corpus digest and working directory cannot be unchecked strings', () => {
  for (const profile of ['', 'Synthetic', 'x'.repeat(65)]) rejectReceipt(v => { v.execution.profile = profile; }, /M6-RECEIPT-EXECUTION/);
  rejectReceipt(v => { v.execution.corpusSha256 = 'A'.repeat(64); }, /M6-RECEIPT-EXECUTION/);
  rejectReceipt(v => { v.execution.corpusSha256 = hash('unreferenced-corpus'); }, /M6-/);
  for (const cwd of ['../escape', '/absolute', 'NUL']) rejectReceipt(v => { v.execution.cwd = cwd; });
});

test('noncanonical, duplicate, invalid UTF-8 and oversized gate bytes fail after digest resealing', () => {
  const { record, carriers } = retainedFixture();
  const original = carriers.find(row => row.path === 'evidence/project-linux.json').data;
  const text = original.toString();
  const candidates = [Buffer.from(' ' + text), Buffer.from(text + '{}'),
    original.subarray(0, original.length - 1), Buffer.from(text.replace('"version":1}', '"version":1,"version":1}')),
    Buffer.from(text.replace('"passed":1', '"passed":1e0')), Buffer.from([0xff]), Buffer.alloc(262145, 32)];
  for (const input of candidates) {
    const gate = structuredClone(record.gates[0]);
    gate.receipt.bytes = input.length;
    gate.receipt.sha256 = hash(input);
    assert.throws(() => validateGateReceipt(input, gate, record));
  }
});
