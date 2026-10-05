// External Node watchdog; page/worker timers cannot acknowledge browser teardown.
import { performance } from 'node:perf_hooks';
import { fail, limits } from './limits.mjs';

export async function createBrowserWatchdog({ session, contextId, workerUrl, terminateBrowser }) {
  if (!session || typeof session.send !== 'function' || typeof session.on !== 'function' ||
      typeof session.off !== 'function' || typeof contextId !== 'string' || !contextId ||
      typeof terminateBrowser !== 'function') fail('BROWSER-HOST');
  const url = new URL(workerUrl);
  if (url.protocol !== 'http:' || url.hostname !== '127.0.0.1' || url.pathname !== '/examples/playground/restricted/worker.mjs' ||
      url.search || url.hash || url.username || url.password) fail('BROWSER-HOST');
  let active;
  let poisoned = false;
  let closed = false;
  let termination;
  const worker = target => ['worker', 'shared_worker', 'service_worker'].includes(target.type);
  const owned = target => target.type === 'worker' && target.browserContextId === contextId && target.url === workerUrl;

  async function terminate(error) {
    poisoned = true;
    termination ??= Promise.resolve().then(terminateBrowser);
    try { await termination; }
    catch (cleanup) { throw new AggregateError([error, cleanup], 'PLAYGROUND-HOST-TEARDOWN'); }
    throw error;
  }

  async function command(method, input, deadline) {
    let timer;
    try {
      return await Promise.race([session.send(method, input), new Promise((_, reject) => {
        timer = setTimeout(() => reject(new Error('PLAYGROUND-HOST-TEARDOWN')),
          Math.max(0, deadline - performance.now()));
      })]);
    } finally { clearTimeout(timer); }
  }

  const targets = deadline => command('Target.getTargets', undefined, deadline);

  async function cleanup(job) {
    if (job.finishing) return job.finishing;
    job.finishing = (async () => {
      clearTimeout(job.timer);
      job.signal?.removeEventListener('abort', job.cancel);
      const deadline = Math.min(job.executionDeadline + limits.teardownMs, performance.now() + limits.teardownMs);
      try {
        while (true) {
          const result = await targets(deadline);
          if (!Array.isArray(result.targetInfos)) fail('BROWSER-TARGETS');
          const workers = result.targetInfos.filter(worker);
          if (workers.some(target => !owned(target))) fail('BROWSER-TARGETS');
          if (!workers.length) break;
          for (const target of workers) {
            // closeTarget can itself stall; observe it under the same absolute deadline.
            const result = await command('Target.closeTarget', { targetId: target.targetId }, deadline);
            if (result.success !== true) fail('BROWSER-TEARDOWN');
          }
          if (performance.now() >= deadline) fail('HOST-TEARDOWN');
          await new Promise(resolve => setTimeout(resolve, Math.min(10, Math.max(0, deadline - performance.now()))));
        }
        job.cleaned = true;
      } catch (error) { await terminate(error); }
    })();
    return job.finishing;
  }

  function created({ targetInfo }) {
    if (!targetInfo || !worker(targetInfo)) return;
    const job = active;
    if (!job || job.cleaned || !owned(targetInfo) || job.seen.has(targetInfo.targetId) || job.seen.size >= 1) {
      // The event handler's rejected promise is retained as a terminal state, never unhandled.
      const error = new Error('PLAYGROUND-BROWSER-TARGETS');
      if (job) job.failure = error;
      void terminate(error).catch(() => {});
      return;
    }
    job.seen.add(targetInfo.targetId);
    if (job.finishing || job.signal?.aborted) void cleanup(job).catch(() => {});
  }
  session.on('Target.targetCreated', created);
  try {
    const deadline = performance.now() + limits.teardownMs;
    await command('Target.setDiscoverTargets', { discover: true }, deadline);
    const baseline = await targets(deadline);
    if (!Array.isArray(baseline.targetInfos) || baseline.targetInfos.some(worker)) fail('BROWSER-TARGETS');
  } catch (error) {
    session.off('Target.targetCreated', created);
    await terminate(error);
  }

  return Object.freeze({
    async begin(token, signal) {
      if (poisoned || closed || active || typeof token !== 'string' || !/^[a-f0-9]{64}$/.test(token) || signal?.aborted) {
        fail('BROWSER-ADMISSION');
      }
      const job = { token, signal, seen: new Set(), executionDeadline: performance.now() + limits.evaluateMs };
      active = job;
      job.cancel = () => { void cleanup(job).catch(() => {}); };
      signal?.addEventListener('abort', job.cancel, { once: true });
      job.timer = setTimeout(() => { job.expired = true; void cleanup(job).catch(() => {}); }, limits.evaluateMs);
      let baseline;
      try { baseline = await targets(Math.min(job.executionDeadline, performance.now() + limits.teardownMs)); }
      catch (error) { await terminate(error); }
      if (!Array.isArray(baseline.targetInfos) || baseline.targetInfos.some(worker)) {
        await terminate(new Error('PLAYGROUND-BROWSER-TARGETS'));
      }
      if (job.finishing || signal?.aborted || poisoned) fail('BROWSER-ADMISSION');
    },
    async finish(token) {
      if (poisoned || closed) fail('HOST-TEARDOWN');
      if (!active) return;
      if (active.token !== token) fail('BROWSER-JOB');
      const job = active;
      await cleanup(job);
      if (termination) await termination;
      if (job.failure || poisoned) fail('HOST-TEARDOWN');
      if (active === job) active = undefined;
      return job.expired === true;
    },
    async close() {
      closed = true;
      session.off('Target.targetCreated', created);
      if (active) await cleanup(active);
      active = undefined;
      if (poisoned) fail('HOST-TEARDOWN');
    },
  });
}
