// Test-only raw differential. Production typed ingestion is exercised independently.
import { spawn } from 'node:child_process';
import path from 'node:path';
import { isDeepStrictEqual } from 'node:util';

if (process.version !== 'v22.22.1') throw new Error('pinned Node required');

const chunks = [];
let bytes = 0;
for await (const chunk of process.stdin) {
  bytes += chunk.length;
  if (bytes > 8 * 1024 * 1024) throw new Error('test input budget');
  chunks.push(chunk);
}
const requests = JSON.parse(Buffer.concat(chunks).toString('utf8'));
if (!Array.isArray(requests) || requests.length > 256) throw new Error('test request budget');
const root = process.argv[2];
const env = {};
for (const key of ['SystemRoot', 'WINDIR']) {
  if (process.platform === 'win32' && process.env[key]) env[key] = process.env[key];
}
const result = await new Promise((resolve, reject) => {
  const child = spawn(process.execPath, [path.join(root, 'adapters/typescript-6/src/worker-v5.mjs')], {
    cwd: root, env, stdio: ['pipe', 'pipe', 'pipe'],
  });
  const responses = [];
  let pending = '';
  let authenticated = false;
  let stdoutBytes = 0;
  let stderrBytes = 0;
  const decoder = new TextDecoder('utf-8', { fatal: true });
  const fail = error => { child.kill('SIGKILL'); reject(error); };
  const timer = setTimeout(() => fail(new Error('test worker hard deadline')), 20000);
  child.on('error', fail);
  child.stdin.on('error', fail);
  child.stdout.on('data', chunk => {
    try {
      stdoutBytes += chunk.length;
      if (stdoutBytes > 64 * 1024 * 1024) throw new Error('worker stdout budget');
      pending += decoder.decode(chunk, { stream: true });
      for (let newline; (newline = pending.indexOf('\n')) >= 0;) {
        const line = pending.slice(0, newline);
        pending = pending.slice(newline + 1);
        const response = JSON.parse(line);
        if (!authenticated) {
          const expected = {
            provider: 'typescript-6', provider_version: '6.0.3', protocol_version: 5,
            capabilities: {
              control_flow_v1: true,
              data_ownership_syntax_v1: true, bounded_generics_syntax_v1: true,
              module_resolution: false, semantic_diagnostics: false,
            },
          };
          if (!isDeepStrictEqual(response, { id: 0, result: expected })) {
            throw new Error('pinned v5 handshake differs');
          }
          authenticated = true;
          child.stdin.end(requests.map((files, index) => JSON.stringify({
            id: index + 1, method: 'analyze', params: { schema_version: 5, files },
          })).join('\n') + '\n');
        } else {
          if (response.id !== responses.length + 1 || responses.length >= requests.length) throw new Error('response framing');
          responses.push(response);
        }
      }
    } catch (error) { fail(error); }
  });
  child.stderr.on('data', chunk => {
    stderrBytes += chunk.length;
    if (stderrBytes > 64 * 1024) fail(new Error('worker stderr budget'));
  });
  child.on('close', code => {
    clearTimeout(timer);
    try { pending += decoder.decode(); } catch (error) { reject(error); return; }
    if (code !== 0 || stderrBytes !== 0 || pending !== '' || !authenticated || responses.length !== requests.length) {
      reject(new Error('worker did not finish exact clean framed responses'));
    } else resolve(responses);
  });
  child.stdin.write(JSON.stringify({ id: 0, method: 'handshake' }) + '\n');
});
process.stdout.write(JSON.stringify(result));
