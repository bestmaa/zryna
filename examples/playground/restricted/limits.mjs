// Transport budgets do not change compiler, syntax, ABI or backend resource authorities.
export const limits = Object.freeze({ source: 4096, request: 32768, header: 98304,
  frame: 1277956, component: 1048576, binding: 65536, report: 65536,
  diagnostics: 256, text: 4096, depth: 8, collection: 512, work: 16384,
  compileMs: 10000, evaluateMs: 5000, teardownMs: 2000 });
export const sourcePath = 'src/main.zry';
export const world = 'zryna:capability-profiles/browser@0.1.0';
export const bindingRevision = 'zryna.browser-component-bindings.v1';

export function fail(reason) { throw new Error(`PLAYGROUND-${reason}`); }
export const isHash = value => typeof value === 'string' && /^[a-f0-9]{64}$/.test(value);
export const isI32 = value => typeof value === 'number' && Number.isInteger(value) &&
  value >= -2147483648 && value <= 2147483647 && !Object.is(value, -0);
export const isRevision = value => Number.isSafeInteger(value) && value > 0;

export function exact(value, keys) {
  if (!value || typeof value !== 'object' || Array.isArray(value)) fail('SHAPE');
  const actual = Object.keys(value).sort();
  const expected = [...keys].sort();
  if (JSON.stringify(actual) !== JSON.stringify(expected)) fail('SHAPE');
}

export function utf8(value) {
  if (typeof value !== 'string') fail('TEXT');
  for (let i = 0; i < value.length; i++) {
    const unit = value.charCodeAt(i);
    if (unit >= 0xd800 && unit <= 0xdbff) {
      const next = value.charCodeAt(++i);
      if (!(next >= 0xdc00 && next <= 0xdfff)) fail('UTF8');
    } else if (unit >= 0xdc00 && unit <= 0xdfff) fail('UTF8');
  }
  return new TextEncoder().encode(value);
}

export async function sha256(bytes) {
  return Array.from(new Uint8Array(await crypto.subtle.digest('SHA-256', bytes)))
    .map(byte => byte.toString(16).padStart(2, '0')).join('');
}
