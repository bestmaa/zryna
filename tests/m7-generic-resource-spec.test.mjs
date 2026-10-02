import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import test from 'node:test';

const fixture = JSON.parse(await readFile(new URL('../spec/language/generic-review-v1-fixtures.json', import.meta.url), 'utf8'));
const maxima = new Map(fixture.limits.map(({ metric, limit }) => [metric, limit]));
const outcome = (metric, count) => count <= BigInt(maxima.get(metric))
  ? 'admit' : fixture.limits.find((entry) => entry.metric === metric).firstExtraCode;

test('reference boundaries cover all seven independent proposed ceilings with exact and first extra', () => {
  assert.deepEqual([...maxima], [
    ['parameters', 2], ['arguments', 2], ['functions', 4096], ['data', 4096],
    ['edges', 65536], ['depth', 64], ['keyBytes', 4096],
  ]);
  for (const entry of fixture.limits) {
    assert.equal(outcome(entry.metric, BigInt(entry.limit)), 'admit', entry.metric);
    assert.equal(outcome(entry.metric, BigInt(entry.limit) + 1n), entry.firstExtraCode, entry.metric);
  }
  assert.equal(outcome('functions', (1n << 64n)), 'ZRYNA-M7201');
  assert.equal(outcome('edges', (1n << 64n)), 'ZRYNA-M7201');
});

test('synthetic instance inventories and ordered edge pairs deduplicate and replay without insertion order', () => {
  const lane = (n) => { const bytes = Buffer.alloc(4); bytes.writeUInt32LE(n); return bytes; };
  const functionKey = (index) => Buffer.concat([
    Buffer.from([0x40]), lane(0), lane(index), lane(1), lane(1), Buffer.from([1]),
  ]).toString('hex');
  const dataKey = (index) => Buffer.concat([
    Buffer.from([0x14]), lane(1), lane(9), Buffer.from([0x10]), lane(0), lane(index),
  ]).toString('hex');
  const functions = Array.from({ length: 4096 }, (_, i) => functionKey(i));
  const data = Array.from({ length: 4096 }, (_, i) => dataKey(i));
  const pairs = [];
  for (let from = 0; from < 256; from += 1) {
    for (let to = 0; to < 256; to += 1) pairs.push(`${functions[from]}|${data[to]}`);
  }
  const inventory = (input) => [...new Set(input)].sort();
  assert.equal(inventory(functions).length, 4096);
  assert.equal(inventory(data).length, 4096);
  assert.equal(inventory(pairs).length, 65536);
  assert.deepEqual(inventory([...pairs].reverse().concat(pairs.slice(0, 100))), inventory(pairs));
  const firstExtra = `${functionKey(65535)}|${data[0]}`;
  assert.equal(outcome('edges', BigInt(inventory([...pairs, firstExtra, firstExtra]).length)), 'ZRYNA-M7201');
  assert.equal(inventory([...pairs, firstExtra]).at(-1), firstExtra);
  for (const [metric, keys, extra] of [['functions', functions, functionKey(65535)], ['data', data, dataKey(65535)]]) {
    assert.equal(outcome(metric, BigInt(inventory(keys.concat(keys)).length)), 'admit');
    assert.equal(outcome(metric, BigInt(inventory([...keys, extra]).length)), 'ZRYNA-M7201');
  }
  const root = Buffer.concat([Buffer.from([0x41]), lane(0), lane(0)]).toString('hex');
  assert.equal(inventory([`${root}|${data[0]}`, `${root}|${data[0]}`]).length, 1);
  assert.equal(inventory([`${data[0]}|${data[0]}`, `${data[0]}|${data[0]}`]).length, 1);
});

test('byte ceiling reference cases do not wrap or confuse byte count with nested application depth', () => {
  const exact = Buffer.alloc(4096);
  const extra = Buffer.concat([exact, Buffer.from([0])]);
  assert.equal(outcome('keyBytes', BigInt(exact.byteLength)), 'admit');
  assert.equal(outcome('keyBytes', BigInt(extra.byteLength)), 'ZRYNA-M7201');
  assert.equal(outcome('depth', 64n), 'admit');
  assert.equal(outcome('depth', 65n), 'ZRYNA-M7201');
  // Synthetic byte/graph fixtures isolate arithmetic. They are not claimed valid
  // canonical type keys, source programs or executable instantiation results.
});
