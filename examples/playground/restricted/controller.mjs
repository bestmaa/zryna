// One source revision and one supervised operation own all asynchronous work.
import { exact, fail, isHash, isI32, isRevision, limits, sha256, utf8 } from './limits.mjs';
import { decodeCompilation } from './protocol.mjs';
import { verifyReceipt } from './executable.mjs';

export function createController({ host, witSha256, authority, templateBytes, WorkerClass = Worker,
  schedule = setTimeout, unschedule = clearTimeout }) {
  if (!host || !isHash(witSha256)) fail('HOST');
  exact(authority, ['compilerSha256', 'toolkitSha256', 'policySha256', 'bindingTemplateSha256']);
  if (Object.values(authority).some(value => !isHash(value)) ||
      !(templateBytes instanceof Uint8Array) || templateBytes.length > limits.binding) fail('HOST');
  let revision = 1;
  let source = '';
  let compiled = null;
  let active = null;
  let poisoned = false;

  function current(job) {
    if (active !== job || job.abort.signal.aborted || job.revision !== revision) fail('STALE');
  }

  function start() {
    if (poisoned) fail('HOST-TEARDOWN');
    if (active) fail('BUSY');
    const job = { revision, source, abort: new AbortController(), worker: null,
      timer: null, token: null, reject: null };
    active = job;
    return job;
  }

  async function finish(job) {
    job.worker?.terminate();
    if (job.timer !== null) unschedule(job.timer);
    let expired;
    try { expired = await host.finish(job.token, job.abort.signal.aborted); }
    catch { poisoned = true; fail('HOST-TEARDOWN'); }
    finally { if (active === job) active = null; }
    if (expired === true) fail('EVALUATION-DEADLINE');
  }

  function cancel() {
    const job = active;
    if (!job) return false;
    job.reason = 'PLAYGROUND-CANCELLED';
    job.abort.abort();
    job.worker?.terminate();
    job.reject?.(new Error('PLAYGROUND-CANCELLED'));
    return true;
  }

  async function interrupted(job, task) {
    let abort;
    const cancelled = new Promise((_, reject) => {
      abort = () => reject(new Error(job.reason ?? 'PLAYGROUND-CANCELLED'));
      job.abort.signal.addEventListener('abort', abort, { once: true });
      if (job.abort.signal.aborted) abort();
    });
    try { return await Promise.race([task, cancelled]); }
    finally { job.abort.signal.removeEventListener('abort', abort); }
  }

  return Object.freeze({
    get revision() { return revision; },
    get busy() { return active !== null; },
    edit(value) {
      utf8(value);
      if (!isRevision(revision + 1)) fail('REVISION');
      cancel();
      source = value;
      revision++;
      compiled = null;
      return revision;
    },
    cancel,
    async compile() {
      if (utf8(source).length > limits.source) fail('SOURCE-LIMIT');
      const job = start();
      compiled = null;
      job.timer = schedule(() => {
        job.reason = 'PLAYGROUND-COMPILER-DEADLINE'; job.abort.abort();
      }, limits.compileMs);
      try {
        const response = await interrupted(job, host.compile({ version: 1, revision: job.revision,
          source: job.source }, job.abort.signal));
        job.token = response.token;
        current(job);
        const expected = { revision: job.revision, source: job.source, witSha256 };
        const result = await decodeCompilation(response.bytes, expected);
        verifyReceipt(response.receipt, result.metadata, authority);
        if (await sha256(response.bytes) !== response.receipt.frameSha256) fail('COMPILER-AUTHORITY');
        current(job);
        if (result.metadata.status === 'compiled') {
          compiled = { bytes: response.bytes.slice(), expected, metadata: result.metadata,
            receipt: response.receipt };
        }
        return result.metadata;
      } finally { await finish(job); }
    },
    async evaluate(logical, args) {
      if (!compiled || compiled.expected.revision !== revision) fail('COMPILE-FIRST');
      const output = compiled;
      const entry = output.metadata.exports.find(entry => entry.logical === logical);
      if (!entry || !Array.isArray(args) || args.length !== entry.arity || args.some(value => !isI32(value))) {
        fail('ARGUMENTS');
      }
      const job = start();
      job.timer = schedule(() => {
        job.reason = 'PLAYGROUND-EVALUATION-DEADLINE';
        job.abort.abort(); job.worker?.terminate();
        job.reject?.(new Error(job.reason));
      }, limits.evaluateMs);
      try {
        // The independent host watchdog must be armed before creating a worker.
        job.token = await interrupted(job, host.beginEvaluation({ revision: job.revision,
          sourceSha256: output.metadata.sourceSha256,
          componentSha256: output.metadata.identity.componentSha256 }, job.abort.signal));
        current(job);
        return await new Promise((resolve, reject) => {
          job.reject = reject;
          try {
            job.worker = new WorkerClass('/examples/playground/restricted/worker.mjs', { type: 'module' });
            job.worker.onerror = () => reject(new Error('PLAYGROUND-WORKER'));
            job.worker.onmessage = event => {
              try {
                current(job);
                const reply = event.data;
                exact(reply, ['revision', 'componentSha256', 'logical', 'ok', 'value', 'error']);
                if (reply.revision !== job.revision || reply.logical !== logical ||
                    reply.componentSha256 !== output.metadata.identity.componentSha256 ||
                    typeof reply.ok !== 'boolean') fail('WORKER-RESPONSE');
                if (!reply.ok) {
                  if (reply.value !== null || typeof reply.error !== 'string' ||
                      !/^(?:PLAYGROUND-[A-Z-]+|ZRYNA-B[0-9]{4})$/.test(reply.error)) fail('WORKER-RESPONSE');
                  reject(new Error(reply.error));
                } else {
                  if (reply.error !== null || !isI32(reply.value)) fail('WORKER-RESPONSE');
                  resolve({ revision: job.revision, logical, value: reply.value,
                    componentSha256: reply.componentSha256 });
                }
              } catch (error) { reject(error); }
            };
            job.worker.postMessage({ revision: job.revision, logical, args: [...args],
              bytes: output.bytes.slice(), expected: output.expected, receipt: output.receipt,
              authority, templateBytes: templateBytes.slice() });
          } catch (error) { reject(error); }
        });
      } finally { await finish(job); }
    },
  });
}
