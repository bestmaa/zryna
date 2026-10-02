import assert from 'node:assert/strict';
import { test } from 'node:test';
import { limits, sha256, utf8 } from '../examples/playground/restricted/limits.mjs';
import { decodeJson } from '../examples/playground/restricted/json.mjs';
import { boundedResponse, decodeCompilation, decodeSourceRequest } from '../examples/playground/restricted/protocol.mjs';
import { verifyReport } from '../examples/playground/restricted/report.mjs';

const encode = value => utf8(JSON.stringify(value));
const source = 'export function main(): i32 { return 42; }\n';
const expected = { revision: 1, source, witSha256: 'a'.repeat(64) };
const report = { schema_version: 1, diagnostics: [{ code: 'TS1005', severity: 'error',
  path: 'src/main.zry', byte_start: 0, byte_end: 1, line_start: 1, column_start: 1,
  line_end: 1, column_end: 2, message: '<img src=x onerror=alert(1)>', guidance: 'fix syntax' }] };

async function rejected() {
  return { format: 'zryna.playground-compilation.v1', version: 1, revision: 1,
    sourcePath: 'src/main.zry', sourceSha256: await sha256(utf8(source)), sourceBytes: utf8(source).length,
    profile: 'browser-component-v1', status: 'rejected', report, failure: null,
    identity: null, exports: [], artifacts: [] };
}

function frame(metadata, payload = new Uint8Array()) {
  const header = encode(metadata);
  const result = new Uint8Array(4 + header.length + payload.length);
  new DataView(result.buffer).setUint32(0, header.length, true);
  result.set(header, 4); result.set(payload, 4 + header.length);
  return result;
}

test('closed input rejects duplicates, unsupported authority and invalid UTF-8 before dispatch', () => {
  for (const bytes of [utf8('{"version":1,"revision":1,"source":"","source":"evil"}'),
    utf8('{"version":1,"revision":1,"source":"","sou\\u0072ce":"evil"}'),
    encode({ version: 1, revision: 1, source: '', target: 'native' }),
    encode({ version: 1, revision: 0, source: '' }), encode({ version: 2, revision: 1, source: '' }),
    utf8('{"version":1,"revision":1,"source":"\\ud800"}'), new Uint8Array([255]),
    utf8('{"version":1,"revision":1,"source":""}{}'),
    utf8('\ufeff{"version":1,"revision":1,"source":""}'),
    utf8('{"version":1,"revision":1.0,"source":""}'),
    utf8('{"version":1,"revision":1e0,"source":""}'),
  ]) assert.throws(() => decodeSourceRequest(bytes), /PLAYGROUND-/);
  const good = decodeSourceRequest(encode({ version: 1, revision: 2, source: 'é\r\n' }));
  assert.equal(good.source, 'é\r\n');
});

test('exact source/wire/depth limits and first extra reject deterministically', () => {
  const bytes = encode({ version: 1, revision: 1, source: 'é'.repeat(2048) });
  assert.equal(utf8(decodeSourceRequest(bytes).source).length, limits.source);
  const exact = new Uint8Array(limits.request).fill(32); exact.set(bytes);
  assert.equal(decodeSourceRequest(exact).revision, 1);
  assert.throws(() => decodeSourceRequest(new Uint8Array(limits.request + 1)), /BYTES/);
  assert.throws(() => decodeSourceRequest(encode({ version: 1, revision: 1, source: 'x'.repeat(4097) })), /REQUEST/);
  const escaped = utf8('{"version":1,"revision":9007199254740991,"source":"' + '\\u0000'.repeat(4096) + '"}');
  assert(escaped.length < limits.request);
  assert.equal(decodeSourceRequest(escaped).source, '\0'.repeat(4096));
  assert.doesNotThrow(() => decodeJson(utf8('['.repeat(8) + '0' + ']'.repeat(8)), 100));
  assert.throws(() => decodeJson(utf8('['.repeat(9) + '0' + ']'.repeat(9)), 100), /JSON-LIMIT/);
});

test('locations resolve exact Unicode scalar and CRLF bytes; canonical order cannot be substituted', () => {
  const source = '\ufeffé😀\r\nx\ry\n';
  const record = { ...report.diagnostics[0], byte_start: 11, byte_end: 12,
    line_start: 2, column_start: 1, line_end: 2, column_end: 2 };
  const value = { schema_version: 1, diagnostics: [record] };
  assert.doesNotThrow(() => verifyReport(value, source));
  for (const mutation of [{ byte_start: 4 }, { line_start: 1 }, { column_end: 9 },
    { path: '../../secret' }, { code: 'TSnot-a-code' }]) {
    assert.throws(() => verifyReport({ ...value, diagnostics: [{ ...record, ...mutation }] }, source));
  }
  const later = { ...record, byte_start: 13, byte_end: 14, line_start: 3, line_end: 3 };
  assert.doesNotThrow(() => verifyReport({ ...value, diagnostics: [record, later] }, source));
  assert.throws(() => verifyReport({ ...value, diagnostics: [later, record] }, source), /ORDER/);
  const astral = { ...record, message: '😀' }; const bmp = { ...record, message: '\uffff' };
  assert.doesNotThrow(() => verifyReport({ ...value, diagnostics: [bmp, astral] }, source));
  assert.throws(() => verifyReport({ ...value, diagnostics: [astral, bmp] }, source), /ORDER/);
});

test('real provider codes and hostile diagnostic text remain inert source-bound data', async () => {
  const result = await decodeCompilation(frame(await rejected()), expected);
  assert.equal(result.metadata.report.diagnostics[0].code, 'TS1005');
  assert.equal(result.metadata.report.diagnostics[0].message, report.diagnostics[0].message);
  assert.deepEqual(result.artifacts, []);
});

test('stale/substituted/malformed rejected replies cannot masquerade as compiler output', async () => {
  const metadata = await rejected();
  for (const mutation of [ { revision: 2 }, { sourceSha256: 'b'.repeat(64) },
    { sourcePath: '../secret.zry' }, { profile: 'data-ownership-v1' },
    { status: 'compiled' }, { report: { schema_version: 2, diagnostics: [] } },
    { artifacts: [{ role: 'loader', bytes: 1, sha256: 'b'.repeat(64) }] },
    { unexpected: true },
  ]) await assert.rejects(decodeCompilation(frame({ ...metadata, ...mutation }), expected));
  await assert.rejects(decodeCompilation(frame(metadata, new Uint8Array([0])), expected));
  const corrupt = frame(metadata); corrupt[0] = 255; corrupt[1] = 255;
  await assert.rejects(decodeCompilation(corrupt, expected));
});

test('framing verifies internal byte consistency; synthetic fixture is not executable authority', async () => {
  const component = new Uint8Array([0, 97, 115, 109, 1, 0, 0, 0]);
  const loader = utf8('export const fixture = 1;');
  const declarations = utf8('export declare const fixture: number;');
  const metadata = { ...await rejected(), status: 'compiled', report: { schema_version: 1, diagnostics: [] },
    identity: { revision: 'zryna.browser-component-bindings.v1', world: 'zryna:capability-profiles/browser@0.1.0',
      witSha256: expected.witSha256, componentSha256: await sha256(component),
      coreSha256: await sha256(component), interfaceSha256: 'b'.repeat(64), coreOffset: 0, coreBytes: 8 },
    exports: [{ logical: 'main', component: 'zryna-export-6d61696e', core: 'main', arity: 0 }], artifacts: [] };
  const payloads = [component, loader, declarations];
  for (const [i, role] of ['component', 'loader', 'declarations'].entries()) {
    metadata.artifacts.push({ role, bytes: payloads[i].length, sha256: await sha256(payloads[i]) });
  }
  const payload = new Uint8Array(payloads.reduce((sum, bytes) => sum + bytes.length, 0));
  let offset = 0; for (const bytes of payloads) { payload.set(bytes, offset); offset += bytes.length; }
  const bytes = frame(metadata, payload);
  assert.equal((await decodeCompilation(bytes, expected)).artifacts.length, 3);
  const substituted = bytes.slice(); substituted[substituted.length - 1] ^= 1;
  await assert.rejects(decodeCompilation(substituted, expected), /ARTIFACT-HASH/);
  for (const identity of [{ ...metadata.identity, witSha256: 'c'.repeat(64) },
    { ...metadata.identity, coreOffset: 1 }, { ...metadata.identity, coreSha256: 'c'.repeat(64) }]) {
    await assert.rejects(decodeCompilation(frame({ ...metadata, identity }, payload), expected));
  }
});

test('abort during a hanging read settles, including hostile cancellation, and fresh reads recover', async () => {
  for (const alreadyAborted of [false, true]) {
    let cancelled = false;
    const abort = new AbortController();
    const response = { ok: true, redirected: false, headers: new Headers(), body: new ReadableStream({
      cancel() { cancelled = true; return new Promise(() => {}); },
    }) };
    if (alreadyAborted) abort.abort();
    const result = boundedResponse(response, abort.signal);
    const rejection = assert.rejects(result, /CANCELLED/);
    abort.abort(); await rejection;
    assert.equal(cancelled, true);
    assert.deepEqual(await boundedResponse(new Response(new Uint8Array([1]))), new Uint8Array([1]));
  }
});

test('stream first extra byte cancels transport and redirect cannot replace authority', async () => {
  let cancelled = false;
  const response = { ok: true, redirected: false, headers: new Headers(), body: new ReadableStream({
    start(controller) { controller.enqueue(new Uint8Array(limits.frame + 1)); },
    cancel() { cancelled = true; },
  }) };
  await assert.rejects(boundedResponse(response), /RESPONSE-LIMIT/);
  assert.equal(cancelled, true);
  const exact = await boundedResponse(new Response(new Uint8Array(limits.frame)));
  assert.equal(exact.length, limits.frame);
  await assert.rejects(boundedResponse({ ...response, redirected: true }), /RESPONSE/);
});
