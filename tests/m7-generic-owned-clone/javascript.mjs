import { readFileSync } from 'node:fs';
import assert from 'node:assert/strict';
import { pathToFileURL } from 'node:url';

const live = new Map();
let sequence = 0, allocation = 0, fault = 0, released = [], allocated = [];
const decoder = new TextDecoder('utf-8', { fatal: true });
function fresh(bytes) {
  allocation++;
  if (allocation === fault) { events.push('fail:'+allocation); return { status: 1 }; }
  const handle = Object.freeze({ identity: ++sequence });
  const copy = new Uint8Array(bytes);
  live.set(handle, copy);
  events.push('alloc:'+(allocated.length+1)+':'+decoder.decode(copy));
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
    events.push('drop:'+(allocated.indexOf(handle.identity)+1)+':'+decoder.decode(bytes));
    released.push({ identity: handle.identity, text: decoder.decode(bytes) });
    bytes.fill(0);
    live.delete(handle);
    return 0;
  },
});
const module = await import(pathToFileURL(process.argv[2]).href);
assert.deepEqual(Object.keys(module).sort(), ['optionNone', 'optionSome', 'resultErr', 'resultOk']);
const oracle = JSON.parse(readFileSync(new URL('./oracle.json', import.meta.url)));
let events = [], observations = 0;
function reset(fail = 0) { assert.equal(live.size, 0); allocation = 0; fault = fail; released = []; allocated = []; events = []; }
function check(expected) {
  assert.deepEqual(events, expected);
  assert.equal(live.size, 0, 'unreleased actual allocation');
  assert.equal(new Set(released.map(v => v.identity)).size, released.length);
  assert.deepEqual(released.map(v => v.identity).sort((a,b)=>a-b), allocated.slice().sort((a,b)=>a-b));
  observations++;
}

const cases = oracle.cases;
for (const [name,flag,value,events] of cases) { reset(); assert.equal(module[name](flag),value); check(events); }
const failures = oracle.failures;
for (const [name,flag,allocation,events] of failures) {
  reset(allocation); assert.throws(() => module[name](flag), /ZRYNA-RT-STATUS-1/); check(events);
  const control=cases.find(c=>c[0]===name&&c[1]===flag);
  reset(); assert.equal(module[name](flag),control[2]); check(control[3]);
}
console.log(JSON.stringify({ target: 'javascript', observations, actualAllocationsReleasedExactlyOnce: true }));
