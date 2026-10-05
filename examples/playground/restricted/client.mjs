// Fixed same-origin routes; no target/path/command or caller-supplied executable authority.
import { exact, fail, isHash, limits, utf8 } from './limits.mjs';
import { decodeJson } from './json.mjs';
import { boundedResponse } from './protocol.mjs';

function base64(value, cap) {
  if (typeof value !== 'string' || value.length > Math.ceil(cap / 3) * 4 ||
      !/^(?:[A-Za-z0-9+/]{4})*(?:[A-Za-z0-9+/]{2}==|[A-Za-z0-9+/]{3}=)?$/.test(value)) fail('ENCODING');
  const bytes = Uint8Array.from(atob(value), char => char.charCodeAt(0));
  if (bytes.length > cap) fail('BYTES');
  return bytes;
}

export async function connect(capability) {
  if (typeof capability !== 'string' || !/^[a-f0-9]{64}$/.test(capability)) fail('CAPABILITY');
  let lastToken = null;
  async function request(path, value, signal) {
    const response = await fetch(path, { method: value === null ? 'GET' : 'POST',
      mode: 'same-origin', credentials: 'omit', cache: 'no-store', redirect: 'error', signal,
      headers: { authorization: `Bearer ${capability}`, ...(value === null ? {} : { 'content-type': 'application/json' }) },
      ...(value === null ? {} : { body: JSON.stringify(value) }) });
    if (!response.ok) {
      const error = decodeJson(await boundedResponse(new Response(response.body), signal), 1024);
      exact(error, ['policyError']);
      if (typeof error.policyError !== 'string' || !/^PLAYGROUND-[A-Z-]+$/.test(error.policyError)) fail('RESPONSE');
      throw new Error(error.policyError);
    }
    return response;
  }
  const startup = new AbortController();
  const timer = setTimeout(() => startup.abort(), limits.compileMs);
  let session;
  try { session = decodeJson(await boundedResponse(await request('/session', null, startup.signal), startup.signal), limits.header); }
  finally { clearTimeout(timer); }
  exact(session, ['version', 'authority', 'witSha256', 'template']);
  if (session.version !== 1 || !isHash(session.witSha256)) fail('SESSION');
  const templateBytes = base64(session.template, limits.binding);
  const host = Object.freeze({
    async compile(value, signal) {
      const response = await request('/compile', value, signal);
      const token = response.headers.get('x-zryna-job');
      if (!/^[a-f0-9]{64}$/.test(token ?? '')) fail('JOB');
      lastToken = token;
      const receipt = decodeJson(base64(response.headers.get('x-zryna-receipt'), 1024), 1024);
      return { token, receipt, bytes: await boundedResponse(response, signal) };
    },
    async beginEvaluation(value, signal) {
      const response = await request('/evaluate', value, signal);
      const reply = decodeJson(await boundedResponse(response, signal), 1024);
      exact(reply, ['token']);
      if (!/^[a-f0-9]{64}$/.test(reply.token ?? '')) fail('JOB');
      lastToken = reply.token; return reply.token;
    },
    async finish(token, cancelled) {
      const abort = new AbortController();
      const timer = setTimeout(() => abort.abort(), limits.teardownMs + 1000);
      try {
        const known = token ?? lastToken;
        const response = await request(known ? '/finish' : '/cancel', known ?
          { token: known, cancelled } : { cancelled: true }, abort.signal);
        const reply = decodeJson(await boundedResponse(response, abort.signal), 1024);
        exact(reply, reply.expired === true ? ['finished', 'expired'] : ['finished']);
        if (reply.finished !== true) fail('HOST-TEARDOWN');
        lastToken = null;
        return reply.expired === true;
      } finally { clearTimeout(timer); }
    },
  });
  return { host, authority: session.authority, witSha256: session.witSha256, templateBytes };
}
