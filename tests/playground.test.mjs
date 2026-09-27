import assert from 'node:assert/strict';
import { test } from 'node:test';
import { corpus, parseI32 } from '../examples/playground/corpus.mjs';
import { createRunner } from '../examples/playground/controller.mjs';
import { boundedBytes, verifyManifest } from '../examples/playground/transport.mjs';

class FakeWorker {
  static instances = [];
  terminated = false;
  constructor(url, options) {
    assert.equal(url, '/examples/playground/worker.mjs');
    assert.deepEqual(options, { type: 'module' });
    FakeWorker.instances.push(this);
  }
  postMessage(value) { this.request = value; }
  terminate() { this.terminated = true; }
  reply(value) { this.onmessage({ data: { id: this.request.id, ...value } }); }
}

function fixture() {
  FakeWorker.instances.length = 0;
  const timers = new Map();
  let nextTimer = 0;
  return {
    timers,
    runner: createRunner({ WorkerClass: FakeWorker,
      schedule: fn => { const id = ++nextTimer; timers.set(id, fn); return id; },
      unschedule: id => timers.delete(id) }),
  };
}

test('fixed corpus pins the existing browser scalar observations', () => {
  assert.deepEqual(corpus.map(entry => entry.expected),
    [42, -2147483648, 2147483647, 0, -5]);
  assert.deepEqual(corpus.map(entry => entry.args),
    [[20, 22], [2147483647, 1], [-2147483648, -1], [0, 0], [-7, 2]]);
  for (const entry of corpus) {
    assert.deepEqual(entry.args.map(value => parseI32(String(value))), entry.args);
  }
});

test('input admission rejects malformed and out-of-range values', () => {
  for (const text of ['', '00', '-0', '+1', ' 1', '1 ', '1.0', '1e2',
    '2147483648', '-2147483649', '999999999999', 'NaN']) {
    assert.throws(() => parseI32(text));
  }
  assert.equal(parseI32('2147483647'), 2147483647);
  assert.equal(parseI32('-2147483648'), -2147483648);
});

test('one worker at a time; successful response terminates it', async () => {
  const { runner, timers } = fixture();
  const first = runner.run([20, 22]);
  assert.equal(FakeWorker.instances.length, 1);
  await assert.rejects(runner.run([0, 0]), /One run at a time/);
  const worker = FakeWorker.instances[0];
  worker.reply({ ok: true, value: 42, componentSha256: 'a'.repeat(64) });
  assert.deepEqual(await first, { value: 42, componentSha256: 'a'.repeat(64) });
  assert.equal(worker.terminated, true);
  assert.equal(timers.size, 0);
});

test('cancel and expiration terminate workers and ignore late responses', async () => {
  const { runner, timers } = fixture();
  const cancelled = runner.run([20, 22]);
  const oldWorker = FakeWorker.instances[0];
  assert.equal(runner.stop(), true);
  await assert.rejects(cancelled, /Cancelled/);
  assert.equal(oldWorker.terminated, true);
  assert.equal(timers.size, 0);

  const expired = runner.run([0, 0]);
  const worker = FakeWorker.instances[1];
  const deadline = [...timers.values()][0];
  deadline();
  await assert.rejects(expired, /expired/);
  assert.equal(worker.terminated, true);
  worker.reply({ ok: true, value: 0, componentSha256: 'a'.repeat(64) });
  assert.equal(timers.size, 0);
  assert.equal(runner.stop(), false);
});

test('malformed worker output terminates the worker', async () => {
  const { runner } = fixture();
  const result = runner.run([20, 22]);
  const worker = FakeWorker.instances[0];
  worker.reply({ ok: true, value: '42', componentSha256: 'a'.repeat(64) });
  await assert.rejects(result, /Invalid worker response/);
  assert.equal(worker.terminated, true);
});

test('streamed artifact accepts exact limit and cancels first extra byte', async () => {
  let cancelled = false;
  const body = new ReadableStream({
    start(controller) {
      controller.enqueue(new Uint8Array([1, 2]));
      controller.enqueue(new Uint8Array([3]));
    },
    cancel() { cancelled = true; },
  });
  const fetcher = async () => ({ ok: true, headers: new Headers(), body });
  await assert.rejects(boundedBytes('/fixed', 2, fetcher), /oversized/);
  assert.equal(cancelled, true);
  const exact = await boundedBytes('/fixed', 2, async () => new Response(
    new Uint8Array([1, 2])));
  assert.deepEqual(exact, new Uint8Array([1, 2]));
});

test('manifest rejects unsupported profile, source and component substitution', () => {
  const hash = 'a'.repeat(64);
  const identity = { revision: '1', world: 'zryna:capability-profiles/browser@0.1.0',
    componentSha256: hash, interfaceSha256: 'b'.repeat(64) };
  const manifest = {
    version: 1, profile: 'zryna-browser-component-v1', command: 'build',
    entrypoint: 'examples/universal/add.zry', stem: 'playground-add',
    source_sha256: 'c'.repeat(64), targets: ['component'],
    browser: { revision: identity.revision, world: identity.world,
      component_sha256: hash, interface_sha256: identity.interfaceSha256 },
    artifacts: [
      { path: 'component/playground-add.wasm', sha256: hash },
      { path: 'component/playground-add.mjs' },
      { path: 'component/playground-add.d.mts' },
    ],
  };
  verifyManifest(manifest, manifest.source_sha256, identity);
  for (const mutation of [
    { profile: 'data-ownership-v1' }, { source_sha256: 'd'.repeat(64) },
    { browser: { ...manifest.browser, component_sha256: 'd'.repeat(64) } },
    { targets: ['component', 'native'] },
  ]) {
    assert.throws(() => verifyManifest({ ...manifest, ...mutation },
      manifest.source_sha256, identity), /identity mismatch/);
  }
});
