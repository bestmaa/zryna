import assert from 'node:assert/strict';
import { request } from 'node:http';
import { test } from 'node:test';
import { createService } from '../examples/playground/restricted/service.mjs';
import { sha256, utf8 } from '../examples/playground/restricted/limits.mjs';

// Synthetic frames and host doubles exercise HTTP transport only; nothing is executed.
const source = 'export function value(): i32 { return 42; }';
const sourceRequest = { version: 1, revision: 7, source };
const witSha256 = 'd'.repeat(64);
const templateBytes = utf8('transport-only template; never imported');

function deferred() {
  let resolve; let reject;
  const promise = new Promise((yes, no) => { resolve = yes; reject = no; });
  return { promise, resolve, reject };
}

async function frame(input, changes = {}) {
  const contents = [new Uint8Array([0, 97, 115, 109, 1, 0, 0, 0]),
    utf8('transport-only loader'), utf8('transport-only declarations')];
  const hashes = await Promise.all(contents.map(sha256));
  const metadata = { format: 'zryna.playground-compilation.v1', version: 1,
    revision: input.revision, sourcePath: 'src/main.zry', sourceSha256: await sha256(utf8(input.source)),
    sourceBytes: utf8(input.source).length, profile: 'browser-component-v1', status: 'compiled',
    report: { schema_version: 1, diagnostics: [] }, failure: null,
    identity: { revision: 'zryna.browser-component-bindings.v1',
      world: 'zryna:capability-profiles/browser@0.1.0', witSha256, componentSha256: hashes[0],
      coreSha256: hashes[0], interfaceSha256: 'e'.repeat(64), coreOffset: 0, coreBytes: 8 },
    exports: [{ logical: 'value', component: 'zryna-export-76616c7565', core: 'value', arity: 0 }],
    artifacts: ['component', 'loader', 'declarations'].map((role, index) => ({ role,
      bytes: contents[index].length, sha256: hashes[index] })), ...changes };
  const header = utf8(JSON.stringify(metadata));
  const bytes = new Uint8Array(4 + header.length + contents.reduce((sum, value) => sum + value.length, 0));
  new DataView(bytes.buffer).setUint32(0, header.length, true);
  bytes.set(header, 4);
  let offset = 4 + header.length;
  for (const content of contents) { bytes.set(content, offset); offset += content.length; }
  return { bytes, metadata };
}

async function fixture(t, overrides = {}) {
  const calls = { compile: [], evaluate: [], finish: [] };
  const host = {
    async compile(bytes, signal, token) {
      calls.compile.push({ bytes, signal, token });
      return (await frame(JSON.parse(new TextDecoder().decode(bytes)))).bytes;
    },
    async beginEvaluation(token, input) { calls.evaluate.push({ token, input }); },
    async finish(token, cancelled) { calls.finish.push({ token, cancelled }); },
    ...overrides,
  };
  const authority = { compilerSha256: 'a'.repeat(64), toolkitSha256: 'b'.repeat(64),
    policySha256: 'c'.repeat(64), bindingTemplateSha256: await sha256(templateBytes) };
  const service = createService({ host, authority, witSha256, templateBytes,
    resources: new Map([['/examples/playground/restricted/', { bytes: utf8('<p>Transport fixture</p>'),
      type: 'text/html; charset=utf-8' }]]) });
  const location = await service.listen();
  t.after(() => service.close());
  const capability = new URL(location.url).hash.slice(1);
  const auth = { authorization: `Bearer ${capability}`, 'sec-fetch-site': 'same-origin',
    origin: location.origin, 'content-type': 'application/json' };
  const send = (path, value = null, options = {}) => new Promise((resolve, reject) => {
    const body = options.raw ?? (value === null ? undefined : JSON.stringify(value));
    const headers = { ...auth, ...(body === undefined ? {} : { 'content-length': Buffer.byteLength(body) }),
      ...options.headers };
    for (const [key, value] of Object.entries(headers)) if (value === null) delete headers[key];
    const outgoing = request(`${location.origin}${path}`, { method: options.method ??
      (body === undefined ? 'GET' : 'POST'), headers, agent: false }, response => {
      const chunks = [];
      response.on('data', bytes => chunks.push(bytes));
      response.on('error', reject);
      response.on('end', () => resolve({ status: response.statusCode, headers: response.headers,
        bytes: new Uint8Array(Buffer.concat(chunks)),
        json() { return JSON.parse(Buffer.concat(chunks).toString('utf8')); } }));
    });
    outgoing.setTimeout(4000, () => outgoing.destroy(new Error('Test transport deadline')));
    outgoing.on('error', reject);
    outgoing.end(body);
  });
  return { calls, host, authority, service, send, auth, ...location };
}

async function expectError(reply, code, status = 400) {
  assert.equal(reply.status, status);
  assert.deepEqual(reply.json(), { policyError: code });
}

async function compile(f) {
  const reply = await f.send('/compile', sourceRequest);
  assert.equal(reply.status, 200);
  const token = reply.headers['x-zryna-job'];
  assert.match(token, /^[a-f0-9]{64}$/);
  return { reply, token };
}

async function startUpload(t, f, path, value) {
  const entered = deferred();
  f.service.server.once('request', () => entered.resolve());
  const bytes = JSON.stringify(value);
  let outgoing;
  const pending = new Promise((resolve, reject) => {
    outgoing = request(`${f.origin}${path}`, { method: 'POST', agent: false,
      headers: { ...f.auth, 'content-length': Buffer.byteLength(bytes) } }, response => {
      const chunks = [];
      response.on('data', chunk => chunks.push(chunk));
      response.on('end', () => resolve({ status: response.statusCode,
        json: () => JSON.parse(Buffer.concat(chunks).toString('utf8')) }));
    });
    outgoing.on('error', reject);
    outgoing.write(bytes.slice(0, 10));
  });
  void pending.catch(() => {});
  t.after(() => outgoing.destroy());
  await entered.promise;
  return { pending, complete: () => outgoing.end(bytes.slice(10)) };
}

test('external evaluation expiry returns a closed teardown acknowledgement and fresh admission', async t => {
  const f = await fixture(t);
  const first = await compile(f);
  await f.send('/finish', { token: first.token, cancelled: false });
  const artifact = await frame(sourceRequest);
  const reply = await f.send('/evaluate', { revision: sourceRequest.revision,
    sourceSha256: artifact.metadata.sourceSha256, componentSha256: artifact.metadata.identity.componentSha256 });
  assert.equal(reply.status, 200);
  f.host.finish = async () => true;
  const finished = await f.send('/finish', { token: reply.json().token, cancelled: false });
  assert.deepEqual(finished.json(), { finished: true, expired: true });
  f.host.finish = async () => false;
  const next = await compile(f);
  assert.equal(next.reply.status, 200);
  await f.send('/finish', { token: next.token, cancelled: false });
});

test('loopback resources are read-only and every response has restrictive browser headers', async t => {
  const f = await fixture(t);
  const reply = await f.send('/examples/playground/restricted/', null, { headers: {
    authorization: null, 'sec-fetch-site': null, origin: null } });
  assert.equal(reply.status, 200);
  assert.equal(reply.headers['cache-control'], 'no-store');
  assert.equal(reply.headers['x-content-type-options'], 'nosniff');
  assert.equal(reply.headers['referrer-policy'], 'no-referrer');
  assert.equal(reply.headers['cross-origin-resource-policy'], 'same-origin');
  assert.match(reply.headers['content-security-policy'], /default-src 'none'/);
  assert.match(reply.headers['content-security-policy'], /frame-ancestors 'none'/);
  await expectError(await f.send('/examples/playground/restricted/', {}), 'PLAYGROUND-ROUTE');
  for (const path of ['/package.json', '/run', '/exec', '/%2e%2e/package.json',
    '/compile?target=native', '/compile#x']) {
    assert.equal((await f.send(path, {})).status, 400);
  }
  assert.equal(f.calls.compile.length, 0);
  assert.equal(f.calls.evaluate.length, 0);
});

test('Host, Bearer, fetch-site and POST Origin checks precede every host operation', async t => {
  const f = await fixture(t);
  for (const headers of [{ host: 'localhost:1' }, { authorization: null },
    { authorization: `Bearer ${'0'.repeat(64)}` }, { 'sec-fetch-site': null },
    { 'sec-fetch-site': 'cross-site' }, { origin: null }, { origin: 'https://foreign.invalid' }]) {
    const reply = await f.send('/compile', sourceRequest, { headers });
    assert.equal(reply.status, 400);
  }
  await expectError(await f.send('/session', null, { headers: { authorization: null } }), 'PLAYGROUND-CAPABILITY');
  const session = await f.send('/session');
  assert.equal(session.status, 200);
  assert.deepEqual(session.json(), { version: 1, authority: f.authority, witSha256,
    template: Buffer.from(templateBytes).toString('base64') });
  assert.deepEqual(f.calls, { compile: [], evaluate: [], finish: [] });
});

test('closed request validation rejects generic commands and malformed source before provider use', async t => {
  const f = await fixture(t);
  const malformed = ['{}', '[]', '{"version":1,"revision":true,"source":""}',
    '{"version":1,"revision":1,"source":"","sou\\u0072ce":"other"}',
    '{"version":1,"revision":1,"source":"\\ud800"}',
    JSON.stringify({ ...sourceRequest, source: 'x'.repeat(4097) }),
    ...['argv', 'target', 'path', 'command', 'authority', 'package'].map(key =>
      JSON.stringify({ ...sourceRequest, [key]: 'unavailable' }))];
  for (const raw of malformed) assert.equal((await f.send('/compile', null, { raw })).status, 400);
  for (const headers of [{ 'content-type': 'text/plain' }, { 'content-encoding': 'gzip' }]) {
    assert.equal((await f.send('/compile', sourceRequest, { headers })).status, 400);
  }
  assert.equal(f.calls.compile.length, 0);
  assert.equal(f.calls.evaluate.length, 0);
  assert.equal((await compile(f)).reply.status, 200);
});

test('escaped UTF-8 source admits the byte limit and rejects the first extra request byte', async t => {
  const f = await fixture(t);
  const input = { ...sourceRequest, source: '\0'.repeat(4096) };
  const raw = JSON.stringify(input).padEnd(32768, ' ');
  const reply = await f.send('/compile', null, { raw });
  assert.equal(reply.status, 200);
  assert.equal(f.calls.compile.length, 1);
  await f.send('/finish', { token: reply.headers['x-zryna-job'], cancelled: false });
  assert.equal((await f.send('/compile', null, { raw: raw + ' ' })).status, 400);
  assert.equal(f.calls.compile.length, 1);
});

test('cancellation during request upload cannot invoke the provider after acknowledged cleanup', async t => {
  const f = await fixture(t);
  const upload = await startUpload(t, f, '/compile', sourceRequest);
  await expectError(await f.send('/compile', sourceRequest), 'PLAYGROUND-BUSY', 409);
  assert.deepEqual((await f.send('/cancel', { cancelled: true })).json(), { finished: true });
  upload.complete();
  await expectError(await upload.pending, 'PLAYGROUND-CANCELLED');
  assert.equal(f.calls.compile.length, 0, 'an upload from a released job cannot start the provider');
});

test('cancelled evaluation upload cannot arm the supervisor after its job was released', async t => {
  const f = await fixture(t); const { token } = await compile(f);
  await f.send('/finish', { token, cancelled: false });
  const metadata = (await frame(sourceRequest)).metadata;
  const upload = await startUpload(t, f, '/evaluate', { revision: 7,
    sourceSha256: metadata.sourceSha256, componentSha256: metadata.identity.componentSha256 });
  assert.deepEqual((await f.send('/cancel', { cancelled: true })).json(), { finished: true });
  upload.complete();
  await expectError(await upload.pending, 'PLAYGROUND-CANCELLED');
  assert.equal(f.calls.evaluate.length, 0, 'released evaluation must not arm another supervisor');
});

test('job and receipt bind exact source revision, returned frame and launcher authority', async t => {
  const f = await fixture(t);
  const { reply, token } = await compile(f);
  const receipt = JSON.parse(Buffer.from(reply.headers['x-zryna-receipt'], 'base64').toString('utf8'));
  assert.deepEqual(receipt, { revision: 7, sourceSha256: await sha256(utf8(source)),
    frameSha256: await sha256(reply.bytes), compilerSha256: f.authority.compilerSha256,
    toolkitSha256: f.authority.toolkitSha256, policySha256: f.authority.policySha256 });
  assert.equal(f.calls.compile[0].token, token);
  assert.deepEqual(JSON.parse(new TextDecoder().decode(f.calls.compile[0].bytes)), sourceRequest);
  await expectError(await f.send('/compile', sourceRequest), 'PLAYGROUND-BUSY', 409);
  await expectError(await f.send('/finish', { token: '0'.repeat(64), cancelled: false }), 'PLAYGROUND-JOB');
  assert.equal(f.calls.finish.length, 0);
  assert.deepEqual((await f.send('/finish', { token, cancelled: false })).json(), { finished: true });
  assert.deepEqual(f.calls.finish, [{ token, cancelled: false }]);
});

test('mismatched provider revision and source digest cannot issue a usable compilation receipt', async t => {
  for (const changes of [{ revision: 8 }, { sourceSha256: 'f'.repeat(64) }, { sourceBytes: 1 }]) {
    const f = await fixture(t, { async compile() { return (await frame(sourceRequest, changes)).bytes; } });
    const reply = await f.send('/compile', sourceRequest);
    await expectError(reply, 'PLAYGROUND-CORRELATION');
    assert.equal(reply.headers['x-zryna-job'], undefined);
    assert.equal(reply.headers['x-zryna-receipt'], undefined);
    assert.equal(f.calls.finish.length, 1);
    assert.equal(f.calls.finish[0].cancelled, true);
  }
});

test('evaluation accepts only the exact latest revision and hashes after compile cleanup', async t => {
  const f = await fixture(t); const { token } = await compile(f);
  const expected = (await frame(sourceRequest)).metadata;
  const value = { revision: 7, sourceSha256: expected.sourceSha256,
    componentSha256: expected.identity.componentSha256 };
  await expectError(await f.send('/evaluate', value), 'PLAYGROUND-BUSY', 409);
  await f.send('/finish', { token, cancelled: false });
  for (const invalid of [{ ...value, revision: 8 }, { ...value, sourceSha256: '0'.repeat(64) },
    { ...value, componentSha256: '0'.repeat(64) }, { ...value, argv: [] }]) {
    assert.equal((await f.send('/evaluate', invalid)).status, 400);
  }
  assert.equal(f.calls.evaluate.length, 0);
  const reply = await f.send('/evaluate', value);
  assert.equal(reply.status, 200);
  assert.equal(f.calls.evaluate.length, 1);
  assert.equal(f.calls.evaluate[0].token, reply.json().token);
  assert.deepEqual({ ...f.calls.evaluate[0].input }, value);
  await expectError(await f.send('/compile', sourceRequest), 'PLAYGROUND-BUSY', 409);
  await f.send('/cancel', { cancelled: true });
  assert.equal(f.calls.finish.at(-1).cancelled, true);
});

test('cancellation aborts in-flight compile and reservation remains held until teardown acknowledgement', async t => {
  const entered = deferred(); const cleaned = deferred();
  const f = await fixture(t, {
    async compile(bytes, signal) {
      entered.resolve(signal);
      return await new Promise((resolve, reject) => signal.addEventListener('abort', () =>
        reject(new Error('PLAYGROUND-CANCELLED')), { once: true }));
    },
    async finish() { await cleaned.promise; },
  });
  const pending = f.send('/compile', sourceRequest);
  const signal = await entered.promise;
  const cancellation = f.send('/cancel', { cancelled: true });
  await new Promise(resolve => signal.addEventListener('abort', resolve, { once: true }));
  assert.equal(signal.aborted, true);
  await expectError(await f.send('/compile', sourceRequest), 'PLAYGROUND-BUSY', 409);
  cleaned.resolve();
  assert.deepEqual((await cancellation).json(), { finished: true });
  await expectError(await pending, 'PLAYGROUND-CANCELLED');
  f.host.compile = async bytes => (await frame(JSON.parse(new TextDecoder().decode(bytes)))).bytes;
  await compile(f);
});

test('failed teardown permanently closes job admission instead of silently recovering', async t => {
  const f = await fixture(t, { async finish() { throw new Error('cleanup could not be proved'); } });
  const { token } = await compile(f);
  await expectError(await f.send('/finish', { token, cancelled: false }), 'PLAYGROUND-HOST-TEARDOWN');
  await expectError(await f.send('/compile', sourceRequest), 'PLAYGROUND-HOST-TEARDOWN');
  assert.equal(f.calls.compile.length, 1);
});

test('overlapping finish acknowledgements cannot release another pending cleanup', async t => {
  const entered = deferred(); const first = deferred(); const second = deferred();
  let finishes = 0;
  const f = await fixture(t, { async finish() {
    const index = ++finishes;
    if (index === 1) { entered.resolve(); await first.promise; }
    if (index === 2) await second.promise;
  } });
  t.after(() => { first.resolve(); second.resolve(); });
  const { token } = await compile(f);
  const a = f.send('/finish', { token, cancelled: false });
  await entered.promise;
  const uploaded = deferred();
  f.service.server.once('request', incoming => incoming.once('end', () => uploaded.resolve()));
  const b = f.send('/finish', { token, cancelled: false });
  void b.catch(() => {});
  await uploaded.promise;
  await new Promise(resolve => setImmediate(resolve));
  first.resolve();
  await a;
  try {
    // Shared cleanup may settle both callers; an independent cleanup must keep admission closed.
    if (finishes > 1) await expectError(await f.send('/compile', sourceRequest), 'PLAYGROUND-BUSY', 409);
  } finally {
    second.resolve();
    await b;
  }
  assert.equal(finishes, 1, 'one job must have exactly one shared teardown operation');
});
