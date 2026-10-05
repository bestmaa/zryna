import assert from 'node:assert/strict';
import { EventEmitter } from 'node:events';
import { test } from 'node:test';
import { createBrowserWatchdog } from '../examples/playground/restricted/watchdog.mjs';

// CDP doubles verify external scheduling/acknowledgement; no Chrome isolation is claimed.
const token = 'a'.repeat(64);
const contextId = 'captured-context';
const workerUrl = 'http://127.0.0.1:12345/examples/playground/restricted/worker.mjs';
const target = (changes = {}) => ({ targetId: 'worker-1', type: 'worker', url: workerUrl,
  browserContextId: contextId, ...changes });

class Session extends EventEmitter {
  calls = [];
  targets = [];
  closeAcknowledgement = true;
  async send(method, input) {
    this.calls.push([method, input]);
    if (method === 'Target.getTargets') return { targetInfos: [...this.targets] };
    if (method === 'Target.closeTarget') {
      if (this.closeAcknowledgement) this.targets = this.targets.filter(target => target.targetId !== input.targetId);
      return { success: this.closeAcknowledgement };
    }
    return {};
  }
  create(info = target()) {
    this.targets.push(info); this.emit('Target.targetCreated', { targetInfo: info });
  }
}

async function fixture(t) {
  const session = new Session(); let kills = 0;
  const host = await createBrowserWatchdog({ session, contextId, workerUrl,
    async terminateBrowser() { kills++; session.targets = []; } });
  t.after(() => host.close().catch(() => {}));
  return { session, host, kills: () => kills };
}

test('fixed independent browser context and worker URL reject malformed host inputs', async () => {
  for (const changes of [{ contextId: '' }, { workerUrl: 'https://127.0.0.1:1/examples/playground/restricted/worker.mjs' },
    { workerUrl: 'http://localhost:1/examples/playground/restricted/worker.mjs' },
    { workerUrl: workerUrl + '?source=guest' }, { workerUrl: workerUrl + '#secret' }]) {
    await assert.rejects(createBrowserWatchdog({ session: new Session(), contextId, workerUrl,
      terminateBrowser() {}, ...changes }), /PLAYGROUND-BROWSER-HOST/);
  }
});

test('pre-existing workers prevent startup and require independent browser termination', async () => {
  const session = new Session(); session.targets = [target()]; let killed = false;
  await assert.rejects(createBrowserWatchdog({ session, contextId, workerUrl,
    terminateBrowser() { killed = true; } }), /PLAYGROUND-BROWSER-TARGETS/);
  assert.equal(killed, true); assert.equal(session.listenerCount('Target.targetCreated'), 0);
});

test('one evaluation reserves admission until close acknowledgement and explicit zero-worker observation', async t => {
  const { session, host, kills } = await fixture(t);
  await host.begin(token, new AbortController().signal);
  session.create();
  await assert.rejects(host.begin('b'.repeat(64)), /PLAYGROUND-BROWSER-ADMISSION/);
  await assert.rejects(host.finish('b'.repeat(64)), /PLAYGROUND-BROWSER-JOB/);
  assert.equal(await host.finish(token), false);
  assert.deepEqual(session.targets, []);
  assert.ok(session.calls.some(([method]) => method === 'Target.closeTarget'));
  assert.equal(session.calls.at(-1)[0], 'Target.getTargets');
  assert.equal(kills(), 0);
  await host.begin('b'.repeat(64)); assert.equal(await host.finish('b'.repeat(64)), false);
});

test('cancellation drains the external worker before fresh admission', async t => {
  const { session, host } = await fixture(t); const signal = new AbortController();
  await host.begin(token, signal.signal); session.create(); signal.abort();
  assert.equal(await host.finish(token), false); assert.equal(session.targets.length, 0);
  await host.begin('b'.repeat(64)); await host.finish('b'.repeat(64));
});

test('external deadline remains observable until finish and permits recovery after proven empty teardown', async t => {
  t.mock.timers.enable({ apis: ['setTimeout'] });
  const { host } = await fixture(t);
  await host.begin(token); t.mock.timers.tick(5000);
  await Promise.resolve(); await Promise.resolve();
  await assert.rejects(host.begin('b'.repeat(64)), /PLAYGROUND-BROWSER-ADMISSION/);
  assert.equal(await host.finish(token), true);
  await host.begin('b'.repeat(64)); assert.equal(await host.finish('b'.repeat(64)), false);
});

test('worker close failure poisons admission and terminates the dedicated browser once', async t => {
  const { session, host, kills } = await fixture(t);
  await host.begin(token); session.create(); session.closeAcknowledgement = false;
  await assert.rejects(host.finish(token), /PLAYGROUND-BROWSER-TEARDOWN/);
  await assert.rejects(host.begin('b'.repeat(64)), /PLAYGROUND-BROWSER-ADMISSION/);
  assert.equal(kills(), 1);
});

test('unexpected worker context or extra worker requires independent terminal cleanup', async t => {
  const { session, host, kills } = await fixture(t);
  await host.begin(token); session.create(target({ browserContextId: 'foreign' }));
  await assert.rejects(host.finish(token), /PLAYGROUND-HOST-TEARDOWN/);
  assert.equal(kills(), 1);
});

test('worker creation after acknowledged teardown cannot become an unreserved operation', async t => {
  const { session, host, kills } = await fixture(t);
  await host.begin(token); await host.finish(token); session.create();
  await Promise.resolve(); await Promise.resolve();
  await assert.rejects(host.begin('b'.repeat(64)), /PLAYGROUND-BROWSER-ADMISSION/);
  assert.equal(kills(), 1);
});
