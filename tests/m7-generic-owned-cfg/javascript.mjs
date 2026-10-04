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
assert.deepEqual(Object.keys(module).sort(), ['early', 'option', 'reinitialized', 'replacement', 'scalar', 'selfMove', 'updated']);
const oracles = {'replacement': ['α', 'new', '残'], 'reinitialized': ['old', 'new'], 'option': ['old', 'new'], 'scalar': [], 'updated': [], 'early': ['early', 'value', 'outer'], 'selfMove': ['self']};
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
  reset(); assert.equal(module[name](7), name === 'updated' ? 8 : 7); check(expected);
}
for (const [name, failure, expected] of [['replacement',1,[]],['replacement',2,['α']],['replacement',3,['残','α']],['option',2,['old']],['early',3,['value','outer']]]) {
  reset(failure); assert.throws(() => module[name](7), /ZRYNA-RT-STATUS-1/); check(expected);
  reset(); assert.equal(module[name](7),7); check(oracles[name]);
}
console.log(JSON.stringify({ target: 'javascript', observations, actualAllocationsReleasedExactlyOnce: true }));
