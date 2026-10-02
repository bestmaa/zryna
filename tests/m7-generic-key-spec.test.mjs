import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { readFile } from 'node:fs/promises';
import test from 'node:test';

const fixture = JSON.parse(await readFile(new URL('../spec/language/generic-review-v1-fixtures.json', import.meta.url), 'utf8'));
const u32 = (n) => { const value = Buffer.alloc(4); value.writeUInt32LE(n); return value; };
const join = (...parts) => Buffer.concat(parts);
const child = (bytes) => join(u32(bytes.length), bytes);
const option = (bytes) => join(Buffer.from([0x14]), u32(1), child(bytes));
const result = (left, right) => join(Buffer.from([0x15]), u32(2), child(left), child(right));
const scalar = Buffer.from([1]);
const hash = (bytes) => createHash('sha256').update(bytes).digest('hex');

// Independent bounded reference decoder for the proposal, not a compiler
// verifier. It proves prefix/count/namespace ambiguity decisions in fixed keys.
function inspect(bytes) {
  if (bytes.length > 4096) return 'ZRYNA-M7201';
  const pending = [{ start: 0, end: bytes.length, depth: 0, typeOnly: false }];
  while (pending.length) {
    const item = pending.pop();
    let cursor = item.start;
    const take = (length) => {
      if (cursor + length > item.end) throw new Error('truncated key');
      const start = cursor;
      cursor += length;
      return start;
    };
    const lane = () => bytes.readUInt32LE(take(4));
    const tag = bytes[take(1)];
    let count = 0;
    let application = false;
    if ([0, 1, 2].includes(tag)) {
      // Complete primitive leaf.
    } else if ([0x10, 0x11].includes(tag)) {
      lane(); lane();
    } else if ([0x12, 0x13, 0x40].includes(tag)) {
      if (item.typeOnly && tag === 0x40) throw new Error('function is not a type');
      lane(); lane(); count = lane();
      if (count < 1 || count > 2) throw new Error('generic arity');
      application = tag !== 0x40;
    } else if ([0x14, 0x15].includes(tag)) {
      count = lane();
      if (count !== (tag === 0x14 ? 1 : 2)) throw new Error('family arity');
      application = true;
    } else if ([0x21, 0x22, 0x23].includes(tag)) {
      count = 1; application = true;
    } else if (tag === 0x20) {
      const length = lane();
      if (length > 1048576) throw new Error('array length');
      count = 1; application = true;
    } else if (tag === 0x41 && !item.typeOnly) {
      lane(); lane();
    } else {
      throw new Error('unknown tag or non-type child');
    }
    const depth = item.depth + (application ? 1 : 0);
    if (depth > 64) return 'ZRYNA-M7201';
    for (let argument = 0; argument < count; argument += 1) {
      const length = lane();
      if (length === 0) throw new Error('empty child');
      const start = take(length);
      pending.push({ start, end: start + length, depth, typeOnly: true });
    }
    if (cursor !== item.end) throw new Error('trailing bytes');
  }
  return 'admit';
}

test('canonical reference decoder rejects ambiguous, truncated and forged key namespaces', () => {
  const valid = option(scalar);
  assert.equal(inspect(valid), 'admit');
  const wrongArity = Buffer.from(valid); wrongArity.writeUInt32LE(2, 1);
  const wrongLength = Buffer.from(valid); wrongLength.writeUInt32LE(2, 5);
  const zeroLength = Buffer.from(valid); zeroLength.writeUInt32LE(0, 5);
  const root = join(Buffer.from([0x41]), u32(0), u32(0));
  const fn = join(Buffer.from([0x40]), u32(0), u32(0), u32(1), child(scalar));
  assert.equal(inspect(fn), 'admit');
  assert.equal(inspect(root), 'admit');
  for (const bytes of [
    wrongArity, wrongLength, zeroLength, valid.subarray(0, valid.length - 1),
    join(valid, scalar), option(root), option(fn), Buffer.from([0x99]),
  ]) assert.throws(() => inspect(bytes));
  assert.notEqual(fn.toString('hex'), root.toString('hex'));
  assert.notEqual(option(scalar).toString('hex'), result(scalar, scalar).toString('hex'));
  assert.equal(inspect(join(Buffer.from([0x20]), u32(2), child(scalar))), 'admit');
  assert.throws(() => inspect(join(Buffer.from([0x20]), u32(1048577), child(scalar))));
});

test('canonical nested keys admit depth 64 and reject 65 including inherited M3 containers', () => {
  let value = scalar;
  for (let depth = 1; depth <= 65; depth += 1) {
    value = option(value);
    assert.equal(inspect(value), depth <= 64 ? 'admit' : 'ZRYNA-M7201');
    const vec = join(Buffer.from([0x21]), child(value));
    assert.equal(inspect(vec), depth <= 63 ? 'admit' : 'ZRYNA-M7201');
  }
});

test('balanced closed Result keys isolate the exact 4096-byte and first-extra key ceiling', () => {
  const balanced = (nodes) => {
    if (nodes === 0) return scalar;
    const left = Math.floor((nodes - 1) / 2);
    return result(balanced(left), balanced(nodes - 1 - left));
  };
  for (const entry of fixture.keyBoundary) {
    let bytes = balanced(entry.resultNodes);
    for (let index = 0; index < entry.optionWrappers; index += 1) bytes = option(bytes);
    assert.equal(bytes.length, entry.keyBytes);
    assert.equal(hash(bytes), entry.sha256);
    assert.equal(inspect(bytes), entry.expected);
    // Node counts are below the data-instance/type ceilings, depth below 64.
    assert.ok(entry.resultNodes + entry.optionWrappers < 4096);
  }
});
