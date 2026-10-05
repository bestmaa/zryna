import assert from 'node:assert/strict';
import { test } from 'node:test';
import { connect } from '../examples/playground/restricted/client.mjs';
import { createController } from '../examples/playground/restricted/controller.mjs';
import { utf8 } from '../examples/playground/restricted/limits.mjs';

// Mock fetch responses are transport-only; no compiler, executable or browser proof is implied.
const capability = 'a'.repeat(64);
const token = 'b'.repeat(64);
const authority = { compilerSha256: 'c'.repeat(64), toolkitSha256: 'd'.repeat(64),
  policySha256: 'e'.repeat(64), bindingTemplateSha256: 'f'.repeat(64) };
const session = { version: 1, authority, witSha256: '1'.repeat(64),
  template: Buffer.from('transport-only template').toString('base64') };
const receipt = { revision: 4, sourceSha256: '2'.repeat(64), frameSha256: '3'.repeat(64),
  compilerSha256: authority.compilerSha256, toolkitSha256: authority.toolkitSha256,
  policySha256: authority.policySha256 };
const json = value => new Response(JSON.stringify(value), { headers: { 'content-type': 'application/json' } });
const compileHeaders = { 'x-zryna-job': token,
  'x-zryna-receipt': Buffer.from(JSON.stringify(receipt)).toString('base64') };

function fixture(t, routes = {}) {
  const calls = [];
  t.mock.method(globalThis, 'fetch', async (path, options) => {
    calls.push({ path, options, value: options.body === undefined ? null : JSON.parse(options.body) });
    assert.equal(options.mode, 'same-origin');
    assert.equal(options.credentials, 'omit');
    assert.equal(options.cache, 'no-store');
    assert.equal(options.redirect, 'error');
    assert.equal(options.headers.authorization, `Bearer ${capability}`);
    assert.equal(options.method, path === '/session' ? 'GET' : 'POST');
    if (path !== '/session') assert.equal(options.headers['content-type'], 'application/json');
    if (routes[path]) return await routes[path](options, calls.at(-1).value);
    if (path === '/session') return json(session);
    if (path === '/finish' || path === '/cancel') return json({ finished: true });
    throw new Error(`Unexpected transport route ${path}`);
  });
  return calls;
}

test('invalid capability never starts a session fetch', async t => {
  const calls = fixture(t);
  for (const value of ['', 'A'.repeat(64), '<script>', null, 'a'.repeat(63)]) {
    await assert.rejects(connect(value), /CAPABILITY/);
  }
  assert.equal(calls.length, 0);
});

test('session returns fixed material and rejects response-selected routes or authority fields', async t => {
  fixture(t);
  const options = await connect(capability);
  assert.deepEqual({ ...options.authority }, authority);
  assert.equal(options.witSha256, session.witSha256);
  assert.deepEqual(options.templateBytes, utf8('transport-only template'));
  for (const changes of [{ compilerUrl: 'https://foreign.invalid' }, { version: 2 },
    { template: '!!' }, { witSha256: 'invalid' }]) {
    fixture(t, { '/session': () => json({ ...session, ...changes }) });
    await assert.rejects(connect(capability));
  }
  fixture(t, { '/session': () => json({ ...session, authority: { ...authority, executable: '/other' } }) });
  const unexpected = await connect(capability);
  assert.throws(() => createController({ ...unexpected, WorkerClass: class {} }), /SHAPE/);
});

test('compile body cannot replace fixed session authority and finish uses its exact returned token', async t => {
  const payload = utf8(JSON.stringify({ authority: { compilerSha256: '0'.repeat(64) },
    route: '/arbitrary-execution', argv: ['ignored transport-only data'] }));
  const calls = fixture(t, { '/compile': () => new Response(payload, { headers: compileHeaders }) });
  const options = await connect(capability);
  const result = await options.host.compile({ version: 1, revision: 4, source: 'edited source' },
    new AbortController().signal);
  assert.deepEqual({ ...options.authority }, authority);
  assert.deepEqual(result.bytes, payload);
  assert.deepEqual({ ...result.receipt }, receipt);
  await options.host.finish(null, false);
  assert.deepEqual(calls.map(call => call.path), ['/session', '/compile', '/finish']);
  assert.deepEqual(calls[2].value, { token, cancelled: false });
  await options.host.finish(null, true);
  assert.equal(calls[3].path, '/cancel');
  assert.deepEqual(calls[3].value, { cancelled: true });
});

test('aborting a stalled compile body preserves the header token for supervised finish', async t => {
  let cancelled = false;
  const body = new ReadableStream({ start() {}, cancel() { cancelled = true; } });
  const calls = fixture(t, { '/compile': () => new Response(body, { headers: compileHeaders }) });
  const options = await connect(capability);
  const abort = new AbortController();
  const pending = options.host.compile({ version: 1, revision: 4, source: '' }, abort.signal);
  await new Promise(resolve => setImmediate(resolve));
  abort.abort();
  await assert.rejects(pending, /CANCELLED/);
  assert.equal(cancelled, true);
  await options.host.finish(null, true);
  assert.equal(calls.at(-1).path, '/finish');
  assert.deepEqual(calls.at(-1).value, { token, cancelled: true });
});

test('cancellation before response headers uses fixed cancel rather than inventing a job token', async t => {
  const calls = fixture(t, { '/compile': options => new Promise((resolve, reject) => {
    options.signal.addEventListener('abort', () => reject(new Error('transport aborted')), { once: true });
  }) });
  const options = await connect(capability);
  const abort = new AbortController();
  const pending = options.host.compile({ version: 1, revision: 4, source: '' }, abort.signal);
  abort.abort();
  await assert.rejects(pending, /transport aborted/);
  await options.host.finish(null, true);
  assert.deepEqual(calls.at(-1).value, { cancelled: true });
  assert.equal(calls.at(-1).path, '/cancel');
});

test('malformed receipt cannot discard a known job token before teardown', async t => {
  const calls = fixture(t, { '/compile': () => new Response('transport-only body', { headers: {
    ...compileHeaders, 'x-zryna-receipt': '!!' } }) });
  const options = await connect(capability);
  await assert.rejects(options.host.compile({ version: 1, revision: 4, source: '' },
    new AbortController().signal), /ENCODING/);
  await options.host.finish(null, true);
  assert.equal(calls.at(-1).path, '/finish');
  assert.deepEqual(calls.at(-1).value, { token, cancelled: true });
});

test('teardown acknowledgement is exact and failure retains the token for another cleanup attempt', async t => {
  let finishes = 0;
  const calls = fixture(t, {
    '/evaluate': () => json({ token }),
    '/finish': () => json(++finishes === 1 ? { finished: true, extra: 'untrusted' } : { finished: true }),
  });
  const options = await connect(capability);
  assert.equal(await options.host.beginEvaluation({ revision: 4, sourceSha256: '2'.repeat(64),
    componentSha256: '3'.repeat(64) }, new AbortController().signal), token);
  await assert.rejects(options.host.finish(null, true), /SHAPE/);
  await options.host.finish(null, true);
  assert.deepEqual(calls.slice(-2).map(call => call.value),
    [{ token, cancelled: true }, { token, cancelled: true }]);
});

test('host policy errors stay separate and malformed error payloads reject', async t => {
  for (const value of [{ policyError: 'PLAYGROUND-BUSY' }, { policyError: '<script>' },
    { policyError: 'PLAYGROUND-BUSY', diagnostics: [] }]) {
    fixture(t, { '/compile': () => new Response(JSON.stringify(value), { status: 409 }) });
    const options = await connect(capability);
    await assert.rejects(options.host.compile({ version: 1, revision: 4, source: '' },
      new AbortController().signal), value.policyError === 'PLAYGROUND-BUSY' && !value.diagnostics ?
      /PLAYGROUND-BUSY/ : /PLAYGROUND-(RESPONSE|SHAPE)/);
  }
});

test('exact independently expired teardown acknowledgement clears the token and returns expiry', async t => {
  const calls = fixture(t, { '/evaluate': () => json({ token }),
    '/finish': () => json({ finished: true, expired: true }) });
  const options = await connect(capability);
  await options.host.beginEvaluation({ revision: 4, sourceSha256: '2'.repeat(64),
    componentSha256: '3'.repeat(64) }, new AbortController().signal);
  assert.equal(await options.host.finish(token, false), true);
  assert.equal(await options.host.finish(null, true), false);
  assert.equal(calls.at(-1).path, '/cancel');
});
