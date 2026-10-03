import { spawn, spawnSync } from 'node:child_process';
import { resolve } from 'node:path';
import { performance } from 'node:perf_hooks';
import { MAX_LOG } from './input.mjs';

// Only trusted registered commands run here. Process-tree termination is a separate
// bounded operation; this is not an execution sandbox for arbitrary commands.
export function execute(executable, args, { root, timeoutMs, env = process.env } = {}) {
  return new Promise(resolveResult => {
    const start = performance.now();
    const output = { stdout: [], stderr: [] };
    let total = 0;
    let error = null;
    let done = false;
    let stopping = false;
    let stopTimer;
    const child = spawn(executable, args, { cwd: root, env, shell: false, windowsHide: true,
      detached: process.platform !== 'win32', stdio: ['ignore', 'pipe', 'pipe'] });
    const finish = (exitCode, signal) => {
      if (done) return;
      done = true;
      clearTimeout(timer);
      clearTimeout(stopTimer);
      child.stdout.destroy();
      child.stderr.destroy();
      child.unref();
      resolveResult({ exitCode, signal, error, elapsedMs: performance.now() - start,
        stdout: Buffer.concat(output.stdout), stderr: Buffer.concat(output.stderr) });
    };
    const stop = code => {
      error ??= code;
      if (stopping) return;
      stopping = true;
      if (child.pid) {
        if (process.platform === 'win32') {
          const killed = spawnSync(resolve(process.env.SystemRoot ?? 'C:/Windows', 'System32/taskkill.exe'),
            ['/PID', String(child.pid), '/T', '/F'], { shell: false, windowsHide: true,
              timeout: 10_000, maxBuffer: 64 * 1024 });
          if (killed.error || killed.status !== 0) error = 'CLEANUP';
        } else {
          try { process.kill(-child.pid, 'SIGKILL'); }
          catch (cause) { if (cause.code !== 'ESRCH') error = 'CLEANUP'; }
        }
      }
      stopTimer ??= setTimeout(() => finish(null, 'SIGKILL'), 10_000);
    };
    const timer = setTimeout(() => stop('ETIMEDOUT'), timeoutMs);
    for (const name of ['stdout', 'stderr']) child[name].on('data', chunk => {
      if (done) return;
      total += chunk.length;
      if (total > MAX_LOG) stop('MAXOUTPUT');
      else output[name].push(chunk);
    });
    child.on('error', cause => { error = cause.code ?? 'SPAWN'; });
    child.on('close', (exitCode, signal) => {
      if (process.platform !== 'win32' && child.pid) {
        try {
          process.kill(-child.pid, 0);
          stop('CLEANUP');
        } catch (cause) { if (cause.code !== 'ESRCH') error = 'CLEANUP'; }
      }
      finish(exitCode, signal);
    });
  });
}
