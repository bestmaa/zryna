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
assert.deepEqual(Object.keys(module).sort(), ['consumed', 'flow', 'terminal']);
let observations = 0;
function reset(fail = 0) { assert.equal(live.size, 0); allocation = 0; fault = fail; released = []; allocated = []; }
function check(expected) {
  assert.deepEqual(released.map(v => v.text), expected);
  assert.equal(live.size, 0, 'unreleased actual allocation');
  assert.equal(new Set(released.map(v => v.identity)).size, released.length);
  assert.deepEqual(released.map(v => v.identity).sort((a,b)=>a-b), allocated.slice().sort((a,b)=>a-b));
  observations++;
}

const cases = [
  ['terminal',true,7,['β','α']], ['terminal',false,7,['α','β']],
  ['flow',true,11,['入','yes','repeat','outer']], ['flow',false,13,['no','tail','outer']],
  ['consumed',true,7,['item','after']], ['consumed',false,7,['item','after']],
];
for (const [name,flag,value,events] of cases) { reset(); assert.equal(module[name](flag),value); check(events); }
const failures = [
  ['terminal',true,1,[]], ['terminal',false,2,['α']],
  ['flow',true,2,['outer']], ['flow',true,3,['yes','outer']],
  ['flow',true,4,['入','yes','outer']], ['flow',false,3,['no','outer']],
  ['consumed',true,2,['item']], ['consumed',false,2,['item']],
];
for (const [name,flag,allocation,events] of failures) {
  reset(allocation); assert.throws(() => module[name](flag), /ZRYNA-RT-STATUS-1/); check(events);
  const control=cases.find(c=>c[0]===name&&c[1]===flag);
  reset(); assert.equal(module[name](flag),control[2]); check(control[3]);
}
console.log(JSON.stringify({ target: 'javascript', observations, actualAllocationsReleasedExactlyOnce: true }));
