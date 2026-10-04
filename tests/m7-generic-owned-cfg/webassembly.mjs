import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
const memory = new WebAssembly.Memory({ initial: 1024, maximum: 1024 });
const view = new DataView(memory.buffer), bytes = new Uint8Array(memory.buffer);
const decoder = new TextDecoder('utf-8', { fatal: true });
let next = 32 * 1024 * 1024, allocation = 0, fault = 0, released = [], allocated = [];
const live = new Map();
function fresh(source, out) {
  allocation++;
  if (allocation === fault) return 1;
  const pointer = next;
  next += Math.max(1, source.length);
  assert(next <= bytes.length);
  bytes.set(source, pointer);
  live.set(pointer, source.length);
  allocated.push(pointer);
  view.setUint32(out, pointer, true);view.setUint32(out+4, source.length, true);view.setUint32(out+8, source.length, true);
  return 0;
}
const runtime = {
  memory,
  stringFromUtf8Copy(pointer, length, out) { const source=bytes.slice(pointer,pointer+length);decoder.decode(source);return fresh(source,out); },
  stringClone(pointer,length,capacity,out) { assert.equal(live.get(pointer),length);assert.equal(capacity,length);return fresh(bytes.slice(pointer,pointer+length),out); },
  stringRelease(pointer,length,capacity) {
    assert(live.has(pointer),'duplicate or forged actual release');assert.equal(live.get(pointer),length);assert.equal(capacity,length);
    released.push({identity:pointer,text:decoder.decode(bytes.subarray(pointer,pointer+length))});bytes.fill(0,pointer,pointer+length);live.delete(pointer);return 0;
  },
};
const { instance }=await WebAssembly.instantiate(await readFile(process.argv[2]),{'zryna.generic.ownership.runtime.v1':runtime});
const module=instance.exports;
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
  reset(failure); assert.throws(() => module[name](7), WebAssembly.RuntimeError); check(expected);
  reset(); assert.equal(module[name](7),7); check(oracles[name]);
}
console.log(JSON.stringify({ target: 'webassembly', observations, actualAllocationsReleasedExactlyOnce: true }));
