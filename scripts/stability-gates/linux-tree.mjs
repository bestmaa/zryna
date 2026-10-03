import { readFileSync, readdirSync } from 'node:fs';
import { performance } from 'node:perf_hooks';

export function linuxProcessIdentity(pid) {
  try {
    const text = readFileSync(`/proc/${pid}/stat`, 'utf8');
    const fields = text.slice(text.lastIndexOf(') ') + 2).trim().split(/\s+/u);
    if (!/^\d+$/u.test(fields[19])) throw new Error('invalid process identity');
    return { pid, parent: Number(fields[1]), group: Number(fields[2]), started: fields[19] };
  } catch (error) {
    if (['ENOENT', 'ESRCH'].includes(error.code)) return null;
    throw error;
  }
}

function same(expected) { return linuxProcessIdentity(expected.pid)?.started === expected.started; }
function send(pid, signal) {
  try { process.kill(pid, signal); }
  catch (error) { if (error.code !== 'ESRCH') throw error; }
}

// Freeze live ancestors before walking descendants, including separate descendant groups.
// /proc is a trusted OS input. PID/start-time checks reject reuse; this is not a native sandbox.
export function terminateLinuxTree(pid, expectedRoot) {
  const owned = new Map();
  const root = linuxProcessIdentity(pid);
  if (root && (!expectedRoot || root.started !== expectedRoot.started)) {
    throw new Error('root process identity changed');
  }
  if (root) owned.set(pid, root);
  send(-pid, 'SIGSTOP');
  const start = performance.now();
  let stable = false;
  try {
    for (let pass = 0; pass < 16 && performance.now() - start < 1000; pass += 1) {
      const names = readdirSync('/proc').filter(name => /^[1-9][0-9]*$/u.test(name));
      if (names.length > 8192) throw new Error('process inventory budget');
      const entries = names.map(name => linuxProcessIdentity(Number(name))).filter(Boolean);
      const before = owned.size;
      let added;
      do {
        added = false;
        for (const entry of entries) {
          if (owned.has(entry.pid)) continue;
          if (entry.group === pid || owned.has(entry.parent) && same(owned.get(entry.parent))) {
            if (!same(entry)) throw new Error('process identity changed');
            send(entry.pid, 'SIGSTOP');
            owned.set(entry.pid, entry);
            added = true;
          }
        }
      } while (added);
      if (owned.size === before) { stable = true; break; }
    }
  } finally {
    // Even failed capture must kill the known tree; failure never certifies cleanup.
    let failure;
    for (const entry of [...owned.values()].reverse()) {
      try { if (entry.group !== pid && same(entry)) send(entry.pid, 'SIGKILL'); }
      catch (error) { failure ??= error; }
    }
    try { send(-pid, 'SIGKILL'); } catch (error) { failure ??= error; }
    if (failure) throw failure;
  }
  if (!stable) throw new Error('process capture did not stabilize within its bound');
  return () => {
    if ([...owned.values()].some(same)) return false;
    try { process.kill(-pid, 0); return false; }
    catch (error) { if (error.code === 'ESRCH') return true; throw error; }
  };
}
