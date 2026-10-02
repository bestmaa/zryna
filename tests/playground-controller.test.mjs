import assert from 'node:assert/strict';
import { test } from 'node:test';
import { createController } from '../examples/playground/restricted/controller.mjs';
import { sha256, utf8 } from '../examples/playground/restricted/limits.mjs';

const source = 'export function main(): i32 { return 42; }';
const witSha256 = 'a'.repeat(64);
const authority = Object.fromEntries(['compilerSha256', 'toolkitSha256', 'policySha256', 'bindingTemplateSha256']
  .map((key, index) => [key, String(index + 1).repeat(64)]));
const deferred = () => { let resolve; const promise = new Promise(done => { resolve = done; });
  return { promise, resolve }; };

// Transport fixtures exercise controller lifetime; they are not compiler or browser evidence.
async function compiledFrame(request) {
  const content = [new Uint8Array([0, 97, 115, 109, 1, 0, 0, 0]),
    utf8('export const fixture = 1;'), utf8('export declare const fixture: number;')];
  const hashes = await Promise.all(content.map(sha256));
  const metadata = { format: 'zryna.playground-compilation.v1', version: 1,
    revision: request.revision, sourcePath: 'src/main.zry', sourceSha256: await sha256(utf8(request.source)),
    sourceBytes: utf8(request.source).length, profile: 'browser-component-v1', status: 'compiled',
    report: { schema_version: 1, diagnostics: [] }, failure: null,
    identity: { revision: 'zryna.browser-component-bindings.v1', world: 'zryna:capability-profiles/browser@0.1.0',
      witSha256, componentSha256: hashes[0], coreSha256: hashes[0], interfaceSha256: 'b'.repeat(64),
      coreOffset: 0, coreBytes: 8 }, exports: [{ logical: 'main', component: 'zryna-export-6d61696e', core: 'main', arity: 0 }],
    artifacts: ['component', 'loader', 'declarations'].map((role, index) =>
      ({ role, bytes: content[index].length, sha256: hashes[index] })) };
  const header = utf8(JSON.stringify(metadata));
  const bytes = new Uint8Array(4 + header.length + content.reduce((sum, bytes) => sum + bytes.length, 0));
  new DataView(bytes.buffer).setUint32(0, header.length, true);
  bytes.set(header, 4);
  let offset = 4 + header.length;
  for (const value of content) { bytes.set(value, offset); offset += value.length; }
  const receipt = { revision: request.revision, sourceSha256: metadata.sourceSha256,
    frameSha256: await sha256(bytes), compilerSha256: authority.compilerSha256,
    toolkitSha256: authority.toolkitSha256, policySha256: authority.policySha256 };
  return { token: 'compile-token', bytes, receipt };
}

function fixture() {
  const workers = []; const timers = new Map(); const events = [];
  class WorkerClass {
    constructor(url, options) {
      assert.equal(url, '/examples/playground/restricted/worker.mjs');
      assert.deepEqual(options, { type: 'module' });
      events.push('worker'); workers.push(this);
    }
    postMessage(request) { this.request = request; }
    terminate() { this.terminated = true; }
    reply(overrides = {}) {
      this.onmessage({ data: { revision: this.request.revision, logical: this.request.logical,
        componentSha256: this.request.componentSha256, ok: true, value: 42, error: null, ...overrides } });
    }
  }
  const host = {
    compile: compiledFrame,
    async beginEvaluation() { events.push('armed'); return 'evaluation-token'; },
    async finish(token, cancelled) { events.push({ token, cancelled }); },
  };
  const controller = createController({ host, witSha256, authority, templateBytes: new Uint8Array([1]), WorkerClass,
    schedule: (fn, ms) => { const token = Symbol(); timers.set(token, { fn, ms }); return token; },
    unschedule: token => timers.delete(token) });
  controller.edit(source);
  return { controller, host, workers, timers, events };
}

async function running(fixture) {
  const metadata = await fixture.controller.compile();
  const pending = fixture.controller.evaluate('main', []);
  await new Promise(resolve => setImmediate(resolve));
  const worker = fixture.workers.at(-1);
  worker.request.componentSha256 = metadata.identity.componentSha256;
  return { pending, worker };
}

test('host watchdog is armed first; concurrent work rejects and success terminates worker', async () => {
  const f = fixture(); const { pending, worker } = await running(f);
  assert.deepEqual(f.events.slice(-2), ['armed', 'worker']);
  await assert.rejects(f.controller.compile(), /BUSY/);
  await assert.rejects(f.controller.evaluate('main', []), /BUSY/);
  worker.reply();
  assert.equal((await pending).value, 42);
  assert.equal(worker.terminated, true);
  assert.equal(f.timers.size, 0);
  assert.equal(f.controller.busy, false);
});

test('editing cancels evaluation and late output cannot overwrite a new revision', async () => {
  const f = fixture(); const { pending, worker } = await running(f);
  const rejection = assert.rejects(pending, /CANCELLED/);
  assert.equal(f.controller.edit(source + '\n'), 3);
  await rejection;
  worker.reply({ value: 99 });
  assert.equal(worker.terminated, true);
  await assert.rejects(f.controller.evaluate('main', []), /COMPILE-FIRST/);
  assert.equal((await f.controller.compile()).revision, 3);
});

test('late compilation after cancellation stays stale and admission waits for host teardown', async () => {
  const f = fixture(); const reply = deferred(); const teardown = deferred();
  f.host.compile = () => reply.promise;
  f.host.finish = () => teardown.promise;
  const pending = f.controller.compile(); const rejection = assert.rejects(pending, /CANCELLED/);
  f.controller.edit(source + '\r\n');
  reply.resolve(await compiledFrame({ revision: 2, source }));
  await new Promise(resolve => setImmediate(resolve));
  assert.equal(f.controller.busy, true);
  await assert.rejects(f.controller.compile(), /BUSY/);
  teardown.resolve(); await rejection;
  assert.equal(f.controller.busy, false);
  await assert.rejects(f.controller.evaluate('main', []), /COMPILE-FIRST/);
});

test('deadline, malformed observation and host teardown failure each prevent reuse of live work', async () => {
  const expired = fixture(); const first = await running(expired);
  const rejection = assert.rejects(first.pending, /DEADLINE/);
  const timer = [...expired.timers.values()][0]; assert.equal(timer.ms, 5000); timer.fn();
  await rejection; assert.equal(first.worker.terminated, true);
  first.worker.reply(); assert.equal(expired.controller.busy, false);

  const malformed = fixture(); const second = await running(malformed);
  second.worker.reply({ value: -0 });
  await assert.rejects(second.pending, /WORKER-RESPONSE/);
  assert.equal(second.worker.terminated, true);

  const poisoned = fixture(); poisoned.host.finish = async () => { throw new Error('still populated'); };
  await assert.rejects(poisoned.controller.compile(), /HOST-TEARDOWN/);
  await assert.rejects(poisoned.controller.compile(), /HOST-TEARDOWN/);
});

test('source/argument bounds and unsupported exports reject before host evaluation', async () => {
  const f = fixture();
  f.controller.edit('x'.repeat(4097));
  await assert.rejects(f.controller.compile(), /SOURCE-LIMIT/);
  f.controller.edit(source); await f.controller.compile();
  for (const [name, args] of [['main', [1]], ['missing', []], ['main', [Infinity]]]) {
    await assert.rejects(f.controller.evaluate(name, args), /ARGUMENTS/);
  }
  assert.equal(f.workers.length, 0);
});

test('independent host expiry invalidates a queued observation without poisoning proven recovery', async () => {
  const f = fixture(); const { pending, worker } = await running(f);
  f.host.finish = async () => true;
  worker.reply();
  await assert.rejects(pending, /EVALUATION-DEADLINE/);
  assert.equal(f.controller.busy, false);
  assert.equal(worker.terminated, true);
  f.host.finish = async () => false;
  assert.equal((await f.controller.compile()).status, 'compiled');
});
