import assert from 'node:assert/strict';
import { spawn } from 'node:child_process';
import { rmSync } from 'node:fs';
import { resolve } from 'node:path';
import { setTimeout as delay } from 'node:timers/promises';
import { linuxProcessIdentity, terminateLinuxTree } from '../../scripts/stability-gates/linux-tree.mjs';

function gone(pid) {
  try { process.kill(pid, 0); return false; }
  catch (error) { if (error.code === 'ESRCH') return true; throw error; }
}

// Own the fork and its close promise before readiness can fail. Deleting a live
// process's cwd is forbidden on Windows; sending a kill is not a close barrier.
export function ownFixture(child) {
  let closed = false;
  const completion = new Promise((resolveClose, reject) => {
    child.once('error', reject);
    child.once('close', (...result) => { closed = true; resolveClose(result); });
  });
  let identity = null;
  if (process.platform === 'linux') identity = linuxProcessIdentity(child.pid);
  let cleanup;
  return {
    completion,
    get closed() { return closed; },
    remove(directory, pids = []) {
      cleanup ??= (async () => {
        const deadline = Date.now() + 10_000;
        let treeGone = () => true;
        if (!closed) {
          if (process.platform === 'win32') {
            const killer = spawn(resolve(process.env.SystemRoot ?? 'C:/Windows', 'System32/taskkill.exe'),
              ['/PID', String(child.pid), '/T', '/F'],
              { shell: false, windowsHide: true, stdio: ['ignore', 'pipe', 'pipe'] });
            let transcript = '';
            for (const stream of [killer.stdout, killer.stderr]) stream.on('data', data => {
              transcript += data.toString();
              if (transcript.length > 64 * 1024) killer.kill('SIGKILL');
            });
            const killed = new Promise((resolveKill, reject) => {
              killer.once('error', reject);
              killer.once('close', (code, signal) => {
                if (code === 0 && signal === null) resolveKill();
                else reject(new Error(`owned fixture taskkill failed: ${code}/${signal}: ${transcript}`));
              });
            });
            const timer = setTimeout(() => killer.kill('SIGKILL'), 10_000);
            try { await killed; } finally { clearTimeout(timer); }
          } else treeGone = terminateLinuxTree(child.pid, identity);
        }
        while (!closed || !treeGone() || pids.some(pid => !gone(pid))) {
          assert(Date.now() < deadline, 'owned fixture cleanup exceeded ten seconds; source retained');
          await delay(20);
        }
        await completion;
        assert(closed, 'fixture close barrier must precede directory removal');
        rmSync(directory, { recursive: true, force: true });
      })();
      return cleanup;
    },
  };
}
