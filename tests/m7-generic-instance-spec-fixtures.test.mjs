import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { readFile } from 'node:fs/promises';
import test from 'node:test';

const instanceFixture = JSON.parse(
  await readFile(new URL('../spec/language/generic-instantiation-v1-fixtures.json', import.meta.url), 'utf8'),
);
const layoutFixture = JSON.parse(
  await readFile(new URL('../spec/memory-model/generic-enum-layout-v1-fixtures.json', import.meta.url), 'utf8'),
);
const lane32 = (value) => {
  const lane = Buffer.alloc(4);
  lane.writeUInt32LE(value);
  return lane;
};
const join = (...parts) => Buffer.concat(parts);
const digest = (bytes) => createHash('sha256').update(bytes).digest('hex');
const child = (bytes) => join(lane32(bytes.length), bytes);
const i32 = Buffer.from([1]);

test('function and type instance keys occupy disjoint ordered namespaces', () => {
  assert.equal(instanceFixture.format, 'zryna.generic-instantiation-v1.proposed-fixtures');
  assert.equal(instanceFixture.status, 'candidate-only');
  const genericFunction = join(Buffer.from([0x40]), lane32(0), lane32(0), lane32(1), child(i32));
  const functionRoot = join(Buffer.from([0x41]), lane32(0), lane32(0));
  const innerBox = Buffer.from(layoutFixture.cases.find(({ name }) => name === 'Box<i32>').typeKeyHex, 'hex');
  const outerBox = join(Buffer.from([0x12]), lane32(0), lane32(0), lane32(1), child(innerBox));
  const cases = [
    [genericFunction, instanceFixture.identities.genericFunction],
    [functionRoot, instanceFixture.identities.nonGenericFunctionRoot],
    [outerBox, instanceFixture.identities.finiteNestedBox],
  ];
  for (const [actual, expected] of cases) {
    assert.equal(actual.toString('hex'), expected.keyHex);
    assert.equal(digest(actual), expected.sha256);
  }

  const option = Buffer.from(layoutFixture.cases.find(({ name }) => name === 'Option<i32>').typeKeyHex, 'hex');
  const ordered = [
    ['Box<i32>', innerBox], ['Option<i32>', option],
    ['identity<i32>', genericFunction], ['non-generic function root', functionRoot],
  ].sort((left, right) => Buffer.compare(left[1], right[1])).map(([name]) => name);
  assert.deepEqual(ordered, instanceFixture.mixedKeyOrder);
  assert.deepEqual(instanceFixture.semanticCases.map(({ expected }) => expected), [
    'admit', 'admit', 'ZRYNA-M7003',
  ]);
});
