import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { readFile } from 'node:fs/promises';
import test from 'node:test';
import Ajv from 'ajv';

const base = new URL('../spec/memory-model/', import.meta.url);
const fixture = JSON.parse(await readFile(new URL('generic-owned-layout-v1-fixtures.json', base), 'utf8'));
const schema = JSON.parse(await readFile(new URL('generic-owned-layout-v1.schema.json', base), 'utf8'));
const validate = new Ajv({ strict: true, allErrors: true }).compile(schema);
const hash = (bytes) => createHash('sha256').update(bytes).digest('hex');
const u32 = (value) => { const bytes = Buffer.alloc(4); bytes.writeUInt32LE(value); return bytes; };
const u64 = (value) => { const bytes = Buffer.alloc(8); bytes.writeBigUInt64LE(BigInt(value)); return bytes; };
const join = (...values) => Buffer.concat(values);
const ints = (...values) => join(...values.map(u32));
const child = (value) => join(u32(value.length), value);
const key = (tag, ...arguments_) => join(Buffer.from([tag]), u32(arguments_.length), ...arguments_.map(child));

function record(tag, id, drop, size, alignment, tail = Buffer.alloc(0)) {
  return join(u32(32 + tail.length), ints(tag, id, drop, drop), u64(size), u64(alignment), tail);
}

function enumLayout(payloads, layouts) {
  const alignment = Math.max(4, ...payloads.filter((id) => id !== null).map((id) => layouts[id].alignment));
  const area = Math.max(0, ...payloads.filter((id) => id !== null).map((id) => layouts[id].size));
  const alignUp = (size) => Math.ceil(size / alignment) * alignment;
  const offset = alignUp(4);
  return { size: alignUp(offset + area), alignment, offset, area };
}

test('owned and nested enum records independently reproduce both target layouts and digests', () => {
  assert.equal(validate(fixture), true, JSON.stringify(validate.errors));
  const innerKey = key(0x14, Buffer.from([2]));
  const keys = [innerKey, key(0x14, innerKey), key(0x15, innerKey, Buffer.from([2]))];
  assert.deepEqual(fixture.cases.map(({ name }) => name), [
    'Option<String>', 'Option<Option<String>>', 'Result<Option<String>,String>',
  ]);
  for (const [target, stringSize, stringAlignment] of [[1, 12, 4], [2, 24, 8]]) {
    const layouts = [
      { size: 1, alignment: 1 }, { size: 4, alignment: 4 },
      { size: stringSize, alignment: stringAlignment },
    ];
    const records = [record(1, 0, 0, 1, 1), record(2, 1, 0, 4, 4), record(6, 2, 2, stringSize, stringAlignment)];
    for (const [index, arguments_, payloads] of [[0, [2], [null, 2]], [1, [3], [null, 3]], [2, [3, 2], [3, 2]]]) {
      const entry = fixture.cases[index];
      const expected = entry.targets.find((value) => value.target === target);
      const shape = enumLayout(payloads, layouts);
      layouts.push(shape);
      const tail = join(
        u32(arguments_.length), ints(...arguments_), u32(2), u64(shape.offset), u64(shape.area),
        ints(0, payloads[0] ?? 0xffffffff, 1, payloads[1]),
      );
      const encoded = record(index === 2 ? 13 : 12, index + 3, 1, shape.size, shape.alignment, tail);
      records.push(encoded);
      const document = join(Buffer.from('ZRYNA-GENERIC-AGGREGATE-LAYOUT-V1\0'), ints(target, records.length), ...records);
      assert.equal(entry.typeId, index + 3);
      assert.equal(entry.typeKeyHex, keys[index].toString('hex'));
      assert.equal(entry.typeKeySha256, hash(keys[index]));
      assert.deepEqual([expected.size, expected.alignment, expected.payloadOffset, expected.payloadAreaSize],
        [shape.size, shape.alignment, shape.offset, shape.area]);
      assert.equal(expected.recordHex, encoded.toString('hex'));
      assert.equal(expected.documentBytes, document.length);
      assert.equal(expected.sha256, hash(document));
      for (const byteOffset of [0, 38, document.length - 1]) {
        const changed = Buffer.from(document);
        changed[byteOffset] ^= 1;
        assert.notEqual(hash(changed), expected.sha256);
      }
      // Mutate family, closed argument ID, variant ordinal and ownership metadata.
      for (const byteOffset of [4, 12, 40, encoded.length - 8]) {
        const changed = Buffer.from(encoded);
        changed[byteOffset] ^= 1;
        assert.notDeepEqual(changed, Buffer.from(expected.recordHex, 'hex'));
      }
    }
  }
  for (const entry of fixture.cases) assert.notEqual(entry.targets[0].sha256, entry.targets[1].sha256);
});

test('owned layout review schema rejects unknown fields, wrong family and malformed digests', () => {
  for (const mutate of [
    (value) => { value.cases[0].targets[0].runtimePassed = true; },
    (value) => { value.cases[0].name = 'Result<String>'; },
    (value) => { value.cases[0].targets[0].sha256 = '00'; },
    (value) => { value.status = 'implemented'; },
  ]) {
    const changed = structuredClone(fixture);
    mutate(changed);
    assert.equal(validate(changed), false);
  }
});
