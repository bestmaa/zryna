// Generated browser scalar binding. Its constants are filled from one audited component.
const exports = Object.freeze(__EXPORTS__);
const expectedBytes = __COMPONENT_BYTES__;
const coreOffset = __CORE_OFFSET__;
const coreBytes = __CORE_BYTES__;
export const browserComponentIdentity = Object.freeze({
  revision: '__REVISION__',
  world: '__WORLD__',
  componentSha256: '__COMPONENT_SHA256__',
  interfaceSha256: '__INTERFACE_SHA256__',
});

function checkI32(value) {
  if (typeof value !== 'number') throw new TypeError('ZRYNA-B2001');
  if (value !== (value | 0) || Object.is(value, -0)) throw new RangeError('ZRYNA-B2002');
}

function withinDeadline(start, deadlineMs) {
  if (performance.now() - start > deadlineMs) throw new Error('ZRYNA-B3993');
}

async function interfaceDigest() {
  const encode = new TextEncoder();
  const chunks = [encode.encode('ZRYNA-SCALAR-COMPONENT-INTERFACE\0\x01')];
  function length(value) {
    const bytes = new Uint8Array(8);
    new DataView(bytes.buffer).setBigUint64(0, BigInt(value), true);
    chunks.push(bytes);
  }
  function name(value) {
    if (typeof value !== 'string') throw new Error('ZRYNA-B3995');
    const bytes = encode.encode(value);
    length(bytes.length);
    chunks.push(bytes);
  }
  for (const entry of exports) {
    name(entry.logical);
    name(entry.component);
    name(entry.core);
    if (!Number.isSafeInteger(entry.arity) || entry.arity < 0 || entry.arity > 256) {
      throw new Error('ZRYNA-B3995');
    }
    length(entry.arity);
  }
  const total = chunks.reduce((sum, chunk) => sum + chunk.length, 0);
  const bytes = new Uint8Array(total);
  let offset = 0;
  for (const chunk of chunks) { bytes.set(chunk, offset); offset += chunk.length; }
  return Array.from(new Uint8Array(await crypto.subtle.digest('SHA-256', bytes)))
    .map(byte => byte.toString(16).padStart(2, '0')).join('');
}

export async function instantiateBrowserComponent(bytes, options = {}) {
  if (!(bytes instanceof Uint8Array) || Object.getPrototypeOf(bytes) !== Uint8Array.prototype ||
      !(bytes.buffer instanceof ArrayBuffer) ||
      bytes.byteLength !== expectedBytes) throw new TypeError('ZRYNA-B3991');
  if (options === null || typeof options !== 'object') throw new TypeError('ZRYNA-B3993');
  const deadlineMs = options.deadlineMs ?? 5000;
  if (!Number.isInteger(deadlineMs) || deadlineMs < 1 || deadlineMs > 30000) {
    throw new TypeError('ZRYNA-B3993');
  }
  const start = performance.now();
  const retained = new Uint8Array(bytes);
  const interfaceSha256 = await interfaceDigest();
  withinDeadline(start, deadlineMs);
  if (interfaceSha256 !== browserComponentIdentity.interfaceSha256) throw new Error('ZRYNA-B3995');
  const digest = Array.from(new Uint8Array(await crypto.subtle.digest('SHA-256', retained)))
    .map(byte => byte.toString(16).padStart(2, '0')).join('');
  withinDeadline(start, deadlineMs);
  if (digest !== browserComponentIdentity.componentSha256) throw new Error('ZRYNA-B3992');
  const core = retained.subarray(coreOffset, coreOffset + coreBytes);
  const { instance } = await WebAssembly.instantiate(core, {});
  withinDeadline(start, deadlineMs);
  const actual = Object.keys(instance.exports).sort();
  const expected = exports.map(entry => entry.core).sort();
  if (JSON.stringify(actual) !== JSON.stringify(expected)) throw new Error('ZRYNA-B3994');
  const bound = Object.create(null);
  for (const entry of exports) {
    const target = instance.exports[entry.core];
    if (typeof target !== 'function') throw new Error('ZRYNA-B3994');
    Object.defineProperty(bound, entry.logical, { enumerable: true, value: (...args) => {
      if (args.length !== entry.arity) throw new TypeError('ZRYNA-B2102');
      for (const arg of args) checkI32(arg);
      const callStart = performance.now();
      withinDeadline(callStart, deadlineMs);
      const value = target(...args);
      withinDeadline(callStart, deadlineMs);
      checkI32(value);
      return value;
    } });
  }
  return Object.freeze(bound);
}
