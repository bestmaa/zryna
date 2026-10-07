import assert from 'node:assert/strict';
import { pathToFileURL } from 'node:url';

const live = new Map();
let sequence = 0, allocation = 0, fault = 0, released = [], allocated = [];
const decoder = new TextDecoder('utf-8', { fatal: true });
function fresh(bytes) {
  allocation++;
  if (allocation === fault) return { status: 1 };
  const handle = Object.freeze({ identity: ++sequence });
  const copy = new Uint8Array(bytes);
  live.set(handle, copy);
  allocated.push(handle.identity);
  return { status: 0, handle };
}
globalThis[Symbol.for('zryna.generic.ownership.runtime.v1')] = Object.freeze({
  stringFromUtf8Copy(bytes, length) {
    assert.equal(length, bytes.length);
    assert(bytes instanceof Uint8Array);
    decoder.decode(bytes);
    return fresh(bytes);
  },
  stringClone(handle) {
    assert(live.has(handle), 'clone of unavailable allocation');
    return fresh(live.get(handle));
  },
  stringRelease(handle) {
    assert(live.has(handle), 'duplicate or forged release');
    const bytes = live.get(handle);
    released.push({ identity: handle.identity, text: decoder.decode(bytes) });
    bytes.fill(0);
    live.delete(handle);
    return 0;
  },
});
const module = await import(pathToFileURL(process.argv[2]).href);
assert.deepEqual(Object.keys(module).sort(), ['clones', 'err', 'exclusive', 'lexical', 'nested', 'nestednone', 'none', 'ok', 'some']);
const oracles = {
  some: ['callee', 'α', '残'], none: ['none'], ok: ['ok'], err: ['err'],
  exclusive: ['exclusive'], clones: ['clone', 'clone'], lexical: ['scope'], nested: ['deep', 'after'], nestednone: [],
};
let observations = 0;
function reset(fail = 0) { assert.equal(live.size, 0); allocation = 0; fault = fail; released = []; allocated = []; }
function check(expected) {
  assert.deepEqual(released.map(v => v.text), expected);
  assert.equal(live.size, 0, 'unreleased actual allocation');
  assert.equal(new Set(released.map(v => v.identity)).size, released.length);
  assert.deepEqual(released.map(v => v.identity).sort((a,b)=>a-b), allocated.slice().sort((a,b)=>a-b));
  observations++;
}
for (const [name, expected] of Object.entries(oracles)) {
  reset(); assert.equal(module[name](7), 7); check(expected);
}
for (const [failure, expected] of [[1, []], [2, ['α']], [3, ['α', '残']]]) {
  reset(failure); assert.throws(() => module.some(7), /ZRYNA-RT-STATUS-1/); check(expected);
  reset(); assert.equal(module.some(7), 7); check(oracles.some);
}
reset(2); assert.throws(() => module.clones(7), /ZRYNA-RT-STATUS-1/); check(['clone']);
for (const invalid of [-0, true, '7', null, 1.5, NaN, 2147483648]) { reset(); assert.throws(() => module.some(invalid)); check([]); }
reset(); assert.throws(() => module.some()); assert.throws(() => module.some(7,8)); check([]);
console.log(JSON.stringify({ target: 'javascript', observations, actualAllocationsReleasedExactlyOnce: true }));
