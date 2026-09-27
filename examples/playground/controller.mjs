const DEADLINE_MS = 5000;

export function createRunner({ WorkerClass = Worker, schedule = setTimeout,
  unschedule = clearTimeout } = {}) {
  let active;
  let nextId = 0;

  function stop(reason = 'Cancelled') {
    if (!active) return false;
    const run = active;
    active = undefined;
    unschedule(run.timer);
    run.worker.terminate();
    run.reject(new Error(reason));
    return true;
  }

  function run(args) {
    if (active) return Promise.reject(new Error('One run at a time'));
    if (!Array.isArray(args) || args.length !== 2 || args.some(value =>
      typeof value !== 'number' || !Number.isInteger(value) ||
      value < -2147483648 || value > 2147483647 || Object.is(value, -0))) {
      return Promise.reject(new Error('Invalid signed i32 arguments'));
    }
    return new Promise((resolve, reject) => {
      const id = ++nextId;
      let worker;
      try {
        worker = new WorkerClass('/examples/playground/worker.mjs', { type: 'module' });
      } catch (error) {
        reject(error);
        return;
      }
      const finish = (error, result) => {
        if (!active || active.id !== id) return;
        unschedule(active.timer);
        active = undefined;
        worker.terminate();
        if (error) reject(error);
        else resolve(result);
      };
      const timer = schedule(() => finish(new Error('Run expired; worker terminated')),
        DEADLINE_MS);
      active = { id, worker, timer, reject };
      worker.onmessage = event => {
        const message = event.data;
        if (!message || message.id !== id || typeof message !== 'object') {
          finish(new Error('Invalid worker response'));
        } else if (message.ok === true &&
            Number.isInteger(message.value) &&
            message.value >= -2147483648 && message.value <= 2147483647 &&
            typeof message.componentSha256 === 'string' &&
            /^[0-9a-f]{64}$/.test(message.componentSha256)) {
          finish(undefined, { value: message.value,
            componentSha256: message.componentSha256 });
        } else if (message.ok === false && typeof message.error === 'string' &&
            message.error.length <= 100) {
          finish(new Error(message.error));
        } else {
          finish(new Error('Invalid worker response'));
        }
      };
      worker.onerror = () => finish(new Error('Worker failed'));
      worker.onmessageerror = () => finish(new Error('Invalid worker message'));
      try { worker.postMessage({ id, args }); }
      catch { finish(new Error('Worker request failed')); }
    });
  }

  return { run, stop };
}
