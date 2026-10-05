// The trusted launcher supplies fixed authenticated resources and supervised host operations.
import { randomBytes } from 'node:crypto';
import { createServer } from 'node:http';
import { exact, fail, isHash, limits, sha256, utf8 } from './limits.mjs';
import { decodeJson } from './json.mjs';
import { decodeCompilation, decodeSourceRequest } from './protocol.mjs';

const csp = "default-src 'none'; script-src 'self' blob: 'wasm-unsafe-eval'; worker-src 'self'; " +
  "connect-src 'self'; style-src 'self'; img-src 'none'; base-uri 'none'; frame-ancestors 'none'; form-action 'none'";

export function createService({ host, authority, witSha256, templateBytes, resources }) {
  exact(authority, ['compilerSha256', 'toolkitSha256', 'policySha256', 'bindingTemplateSha256']);
  if (Object.values(authority).some(value => !isHash(value)) || !isHash(witSha256) ||
      !(templateBytes instanceof Uint8Array) || templateBytes.length > limits.binding ||
      !(resources instanceof Map)) fail('HOST');
  const capability = randomBytes(32).toString('hex');
  let origin;
  let active = null;
  let latest = null;
  let poisoned = false;

  function headers(response, type) {
    response.setHeader('content-type', type);
    response.setHeader('cache-control', 'no-store');
    response.setHeader('content-security-policy', csp);
    response.setHeader('x-content-type-options', 'nosniff');
    response.setHeader('referrer-policy', 'no-referrer');
    response.setHeader('cross-origin-opener-policy', 'same-origin');
    response.setHeader('cross-origin-resource-policy', 'same-origin');
  }

  function send(response, status, bytes, type = 'application/json') {
    headers(response, type); response.writeHead(status); response.end(bytes);
  }

  async function body(request, cap) {
    const declared = request.headers['content-length'];
    if (request.headers['content-type'] !== 'application/json' || request.headers['content-encoding'] ||
        (declared !== undefined && (!/^(0|[1-9][0-9]*)$/.test(declared) || Number(declared) > cap))) fail('REQUEST');
    const chunks = []; let size = 0;
    for await (const bytes of request) {
      size += bytes.length;
      if (size > cap) fail('REQUEST-LIMIT');
      chunks.push(bytes);
    }
    if (declared !== undefined && Number(declared) !== size) fail('REQUEST');
    return new Uint8Array(Buffer.concat(chunks));
  }

  function reserve(kind) {
    if (poisoned) fail('HOST-TEARDOWN');
    if (active) fail('BUSY');
    const job = { kind, token: randomBytes(32).toString('hex'), abort: new AbortController(), timer: null };
    active = job;
    return job;
  }

  function current(job) {
    if (active !== job || job.abort.signal.aborted || job.finishing) fail('CANCELLED');
  }

  async function finish(job, cancelled) {
    if (active !== job) return;
    if (cancelled) job.abort.abort();
    if (!job.finishing) {
      clearTimeout(job.timer);
      job.finishing = Promise.resolve().then(async () => {
        try {
          const expired = await host.finish(job.token, cancelled, job.abort.signal);
          if (expired !== undefined && typeof expired !== 'boolean' || expired === true && job.kind !== 'evaluate') {
            fail('HOST-TEARDOWN');
          }
          return expired === true;
        }
        catch { poisoned = true; fail('HOST-TEARDOWN'); }
        finally { if (active === job) active = null; }
      });
    }
    return job.finishing;
  }

  const server = createServer(async (request, response) => {
    let job;
    try {
      if (request.headers.host !== new URL(origin).host || request.url.length > 256 ||
          request.url.includes('?') || request.url.includes('#')) fail('ORIGIN');
      const path = request.url;
      if (request.method === 'GET' && resources.has(path)) {
        const resource = resources.get(path);
        send(response, 200, resource.bytes, resource.type); return;
      }
      if (request.headers.authorization !== `Bearer ${capability}` ||
          request.headers['sec-fetch-site'] !== 'same-origin') fail('CAPABILITY');
      if (request.method === 'GET' && path === '/session') {
        send(response, 200, JSON.stringify({ version: 1, authority, witSha256,
          template: Buffer.from(templateBytes).toString('base64') })); return;
      }
      if (request.method !== 'POST' || request.headers.origin !== origin) fail('ORIGIN');
      if (path === '/compile') {
        job = reserve('compile'); latest = null;
        response.once('close', () => { if (!response.writableEnded) job.abort.abort(); });
        const bytes = await body(request, limits.request);
        const input = decodeSourceRequest(bytes);
        current(job);
        // A host deadline covers stalled pipes, descendants, and the complete compiler operation.
        const frame = await host.compile(bytes, job.abort.signal, job.token);
        current(job);
        const expected = { revision: input.revision, source: input.source, witSha256 };
        const compiled = await decodeCompilation(frame, expected);
        current(job);
        const receipt = { revision: input.revision, sourceSha256: compiled.metadata.sourceSha256,
          frameSha256: await sha256(frame), compilerSha256: authority.compilerSha256,
          toolkitSha256: authority.toolkitSha256, policySha256: authority.policySha256 };
        current(job);
        if (compiled.metadata.status === 'compiled') latest = compiled.metadata;
        response.setHeader('x-zryna-job', job.token);
        response.setHeader('x-zryna-receipt', Buffer.from(JSON.stringify(receipt)).toString('base64'));
        // The job stays reserved until an explicit teardown acknowledgement.
        job.timer = setTimeout(() => { void finish(job, true).catch(() => {}); }, limits.teardownMs);
        send(response, 200, frame, 'application/octet-stream'); return;
      }
      if (path === '/evaluate') {
        job = reserve('evaluate');
        const input = decodeJson(await body(request, 1024), 1024);
        exact(input, ['revision', 'sourceSha256', 'componentSha256']);
        current(job);
        if (!latest || input.revision !== latest.revision || input.sourceSha256 !== latest.sourceSha256 ||
            input.componentSha256 !== latest.identity.componentSha256) fail('STALE');
        // This call must arm an independent supervisor, not a timer inside this browser.
        await host.beginEvaluation(job.token, input, job.abort.signal);
        current(job);
        send(response, 200, JSON.stringify({ token: job.token })); return;
      }
      if (path === '/finish') {
        const input = decodeJson(await body(request, 1024), 1024);
        exact(input, ['token', 'cancelled']);
        if (typeof input.cancelled !== 'boolean' || !active || input.token !== active.token) fail('JOB');
        const expired = await finish(active, input.cancelled);
        send(response, 200, JSON.stringify(expired ? { finished: true, expired: true } : { finished: true })); return;
      }
      if (path === '/cancel') {
        const input = decodeJson(await body(request, 1024), 1024);
        exact(input, ['cancelled']);
        if (input.cancelled !== true) fail('JOB');
        if (active) await finish(active, true);
        send(response, 200, JSON.stringify({ finished: true })); return;
      }
      fail('ROUTE');
    } catch (error) {
      if (job) { try { await finish(job, true); } catch { poisoned = true; } }
      if (!response.destroyed) {
        const message = /^PLAYGROUND-[A-Z-]+$/.test(error?.message) ? error.message : 'PLAYGROUND-HOST';
        send(response, message === 'PLAYGROUND-BUSY' ? 409 : 400, JSON.stringify({ policyError: message }));
      }
    }
  });
  server.maxConnections = 4;
  server.requestTimeout = limits.compileMs;
  server.headersTimeout = 5000;
  server.timeout = limits.compileMs + limits.teardownMs;
  server.keepAliveTimeout = 1000;
  return Object.freeze({
    server,
    async listen() {
      if (await sha256(templateBytes) !== authority.bindingTemplateSha256) fail('BINDING-TEMPLATE');
      await new Promise((resolve, reject) => {
        server.once('error', reject);
        server.listen(0, '127.0.0.1', resolve);
      });
      origin = `http://127.0.0.1:${server.address().port}`;
      return { origin, url: `${origin}/examples/playground/restricted/#${capability}` };
    },
    async close() {
      latest = null;
      try { if (active) await finish(active, true); }
      finally {
        server.closeAllConnections();
        await new Promise((resolve, reject) => server.close(error => error ? reject(error) : resolve()));
      }
    },
  });
}
