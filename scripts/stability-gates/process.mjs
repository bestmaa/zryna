import { spawn, spawnSync } from 'node:child_process';
import { resolve } from 'node:path';
import { performance } from 'node:perf_hooks';
import { MAX_LOG } from './input.mjs';
import { createInterruptionScope } from './interruption.mjs';
import { linuxProcessIdentity, terminateLinuxTree } from './linux-tree.mjs';

// Only trusted registered commands run here. Process-tree termination is a separate
// bounded operation; this is not an execution sandbox for arbitrary commands.
export function execute(executable, args, { root, timeoutMs, env = process.env, interruption } = {}) {
  return new Promise(resolveResult => {
    const start = performance.now();
    const scope = interruption ?? createInterruptionScope();
    const ownScope = interruption === undefined;
    const output = { stdout: [], stderr: [] };
    let total = 0;
    let error = null;
    let done = false;
    let stopping = false;
    let stopTimer;
    let pollTimer;
    let closed = false;
    let closedCode = null;
    let closedSignal = null;
    let treeGone = () => true;
    let unsubscribe = () => {};
    if (scope.signal) {
      if (ownScope) scope.dispose();
      resolveResult({ exitCode: null, signal: scope.signal, error: 'ECANCELED',
        elapsedMs: performance.now() - start, stdout: Buffer.alloc(0), stderr: Buffer.alloc(0) });
      return;
    }
    const child = spawn(executable, args, { cwd: root, env, shell: false, windowsHide: true,
      detached: process.platform !== 'win32', stdio: ['ignore', 'pipe', 'pipe'] });
    let identity = null;
    if (process.platform === 'linux' && child.pid) {
      try { identity = linuxProcessIdentity(child.pid); } catch { /* Cleanup cannot certify unreadable identities. */ }
    }
    const finish = (exitCode, signal) => {
      if (done) return;
      done = true;
      clearTimeout(timer);
      clearTimeout(stopTimer);
      clearTimeout(pollTimer);
      unsubscribe();
      if (ownScope) scope.dispose();
      child.stdout.destroy();
      child.stderr.destroy();
      child.unref();
      resolveResult({ exitCode: scope.signal || exitCode < 0 ? null : exitCode, signal: scope.signal ?? signal,
        error, elapsedMs: performance.now() - start,
        stdout: Buffer.concat(output.stdout), stderr: Buffer.concat(output.stderr) });
    };
    const confirm = () => {
      if (done) return;
      try {
        if (closed && treeGone()) { finish(closedCode, closedSignal); return; }
      } catch { error = 'CLEANUP'; }
      pollTimer = setTimeout(confirm, 25);
    };
    const stop = code => {
      error ??= code;
      if (stopping) return;
      stopping = true;
      stopTimer = setTimeout(() => {
        error = 'CLEANUP';
        finish(null, 'SIGKILL');
      }, 10_000);
      if (child.pid) {
        if (process.platform === 'win32') {
          const killed = spawnSync(resolve(process.env.SystemRoot ?? 'C:/Windows', 'System32/taskkill.exe'),
            ['/PID', String(child.pid), '/T', '/F'], { shell: false, windowsHide: true,
              timeout: 10_000, maxBuffer: 64 * 1024 });
          if (killed.error || killed.status !== 0) { error = 'CLEANUP'; treeGone = () => false; }
        } else {
          try { treeGone = terminateLinuxTree(child.pid, identity); }
          catch { error = 'CLEANUP'; treeGone = () => false; }
        }
      }
      confirm();
    };
    const timer = setTimeout(() => stop('ETIMEDOUT'), timeoutMs);
    unsubscribe = scope.subscribe(() => stop('ECANCELED'));
    for (const name of ['stdout', 'stderr']) child[name].on('data', chunk => {
      if (done) return;
      total += chunk.length;
      if (total > MAX_LOG) stop('MAXOUTPUT');
      else output[name].push(chunk);
    });
    child.on('error', cause => { error = cause.code ?? 'SPAWN'; });
    child.on('close', (exitCode, signal) => {
      closed = true;
      closedCode = exitCode;
      closedSignal = signal;
      if (stopping) return;
      if (process.platform !== 'win32' && child.pid) {
        try {
          process.kill(-child.pid, 0);
          stop('CLEANUP');
          return;
        } catch (cause) { if (cause.code !== 'ESRCH') error = 'CLEANUP'; }
      }
      finish(exitCode, signal);
    });
  });
}
