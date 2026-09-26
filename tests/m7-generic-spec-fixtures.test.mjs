import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { readFile } from 'node:fs/promises';
import test from 'node:test';

const fixture = JSON.parse(
  await readFile(new URL('../spec/memory-model/generic-enum-layout-v1-fixtures.json', import.meta.url), 'utf8'),
);

const bytes = (...parts) => Buffer.concat(parts);
const lane32 = (value) => {
  const lane = Buffer.alloc(4);
  lane.writeUInt32LE(value);
  return lane;
};
const lane64 = (value) => {
  const lane = Buffer.alloc(8);
  lane.writeBigUInt64LE(BigInt(value));
  return lane;
};
const lanes32 = (...values) => bytes(...values.map(lane32));
const hash = (value) => createHash('sha256').update(value).digest('hex');

function sealedRecord(tag, id, dropKind, runtimeKind, size, alignment, payload = Buffer.alloc(0)) {
  return bytes(
    lane32(32 + payload.length), lanes32(tag, id, dropKind, runtimeKind),
    lane64(size), lane64(alignment), payload,
  );
}

const child = (key) => bytes(lane32(key.length), key);
const i32Key = Buffer.from([1]);
const boolKey = Buffer.from([0]);
const keys = new Map([
  ['Option<i32>', bytes(Buffer.from([0x14]), lane32(1), child(i32Key))],
  ['Box<i32>', bytes(Buffer.from([0x12]), lanes32(0, 0, 1), child(i32Key))],
  ['Choice<i32,bool>', bytes(Buffer.from([0x13]), lanes32(0, 0, 2), child(i32Key), child(boolKey))],
  ['Result<i32,bool>', bytes(Buffer.from([0x15]), lane32(2), child(i32Key), child(boolKey))],
]);

const enumTail = (firstPayloadId, secondPayloadId) => bytes(
  lane32(2), lane64(4), lane64(4), lanes32(0, firstPayloadId, 1, secondPayloadId),
);
const records = new Map([
  ['Option<i32>', sealedRecord(12, 3, 0, 0, 8, 4, bytes(lanes32(1, 1), enumTail(0xffffffff, 1)))],
  ['Box<i32>', sealedRecord(10, 3, 0, 0, 4, 4, bytes(lanes32(0, 0, 1, 1, 1, 0, 1), lane64(0)))],
  ['Choice<i32,bool>', sealedRecord(11, 3, 0, 0, 8, 4, bytes(lanes32(0, 0, 2, 1, 0), enumTail(1, 0)))],
  ['Result<i32,bool>', sealedRecord(13, 3, 0, 0, 8, 4, bytes(lanes32(2, 1, 0), enumTail(1, 0)))],
]);

function fingerprintDocument(target, newRecord) {
  const stringLayout = target === 1 ? [12, 4] : [24, 8];
  return bytes(
    Buffer.from('ZRYNA-GENERIC-AGGREGATE-LAYOUT-V1\0', 'ascii'), lanes32(target, 4),
    sealedRecord(1, 0, 0, 0, 1, 1),
    sealedRecord(2, 1, 0, 0, 4, 4),
    sealedRecord(6, 2, 2, 2, ...stringLayout),
    newRecord,
  );
}

test('proposed generic type keys and both target layout fingerprints match semantic fixtures', () => {
  assert.equal(fixture.format, 'zryna.generic-enum-layout-v1.proposed-fixtures');
  assert.equal(fixture.status, 'candidate-only');
  assert.equal(fixture.fingerprintDomain, 'ZRYNA-GENERIC-AGGREGATE-LAYOUT-V1\0');
  assert.deepEqual(fixture.cases.map(({ name }) => name), [...keys.keys()]);

  for (const entry of fixture.cases) {
    const key = keys.get(entry.name);
    const record = records.get(entry.name);
    assert.equal(key.toString('hex'), entry.typeKeyHex, `${entry.name} key`);
    assert.equal(hash(key), entry.typeKeySha256, `${entry.name} key digest`);
    assert.equal(record.toString('hex'), entry.recordHex, `${entry.name} record`);
    for (const [target, lengthField, digestField] of [
      [1, 'linear32DocumentBytes', 'linear32Sha256'],
      [2, 'linuxX8664DocumentBytes', 'linuxX8664Sha256'],
    ]) {
      const document = fingerprintDocument(target, record);
      assert.equal(document.length, entry[lengthField], `${entry.name} target ${target} size`);
      assert.equal(hash(document), entry[digestField], `${entry.name} target ${target} digest`);
      const changed = Buffer.from(document);
      changed[changed.length - 1] ^= 1;
      assert.notEqual(hash(changed), entry[digestField], `${entry.name} mutation`);
    }
  }
});
