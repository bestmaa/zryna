import { spawnSync } from 'node:child_process';
import { lstatSync, mkdirSync, writeFileSync } from 'node:fs';
import { isAbsolute, relative, resolve, sep } from 'node:path';
import { fileURLToPath } from 'node:url';
import { canonical, MAX_LOG, reject, sha256 } from './input.mjs';
import { ROOT, REGISTRY_SHA256, hostName, loadRegistry, select } from './registry.mjs';
import { snapshot, unchanged } from './source.mjs';
import { assessGate, counts } from './proof.mjs';
import { blockers, commandDigest, validateReceipt } from './evidence.mjs';
import { execute } from './process.mjs';
import { createInterruptionScope } from './interruption.mjs';

function toolVersions(registry) {
  return Object.keys(registry.toolchains).map(name => {
    const result = spawnSync(name, ['--version'], { cwd: ROOT, shell: false,
      encoding: 'utf8', timeout: 30_000, maxBuffer: 64 * 1024 });
    if (result.error || result.signal || result.status !== 0) {
      return { name, version: null, error: result.error?.code ?? 'VERSION' };
    }
    const output = result.stdout.trim();
    const version = ['cargo', 'rustc'].includes(name) ? output.split(' ')[1] : output;
    if (!version || version.length > 128) return { name, version: null, error: 'VERSION' };
    return { name, version, error: null };
  });
}

function hasMemoryTool() {
  if (process.platform !== 'linux') return false;
  try {
    const stat = lstatSync('/usr/bin/time');
    if (!stat.isFile() || stat.isSymbolicLink()) return false;
    const result = spawnSync('/usr/bin/time', ['--version'], { shell: false,
      encoding: 'utf8', timeout: 5000, maxBuffer: 64 * 1024 });
    return !result.error && result.status === 0 && /^time \(GNU Time\)/m.test(result.stdout);
  } catch { return false; }
}

function prepareOutput(output) {
  if (!isAbsolute(output)) reject('absolute new private evidence directory required');
  const rel = relative(ROOT, output);
  if (rel === '' || !rel.startsWith(`..${sep}`) && !isAbsolute(rel)) reject('evidence output must be outside source checkout');
  // Walk the existing parent chain; refuse persistent links and reparse-point symlinks.
  const ancestors = [];
  for (let current = resolve(output, '..'); ; current = resolve(current, '..')) {
    ancestors.push(current);
    if (resolve(current, '..') === current) break;
  }
  for (const ancestor of ancestors) {
    const stat = lstatSync(ancestor);
    if (!stat.isDirectory() || stat.isSymbolicLink()) reject('unsafe evidence parent');
  }
  mkdirSync(output, { mode: 0o700 });
  mkdirSync(resolve(output, 'logs'), { mode: 0o700 });
}

function persist(bytes, output, path) {
  if (bytes.length > MAX_LOG) reject('log byte budget');
  writeFileSync(resolve(output, path), bytes, { flag: 'wx', mode: 0o600 });
  return { path, size: bytes.length, sha256: sha256(bytes) };
}

async function collectWithInterruption(lane, output, interruption) {
  const registry = loadRegistry();
  const host = hostName();
  const gates = select(registry, lane, host);
  const source = snapshot(ROOT);
  const tools = toolVersions(registry);
  prepareOutput(output);
  const results = [];
  let current = null;
  const interrupted = () => {
    let sourceAfter = null;
    let sourceError = null;
    try { sourceAfter = snapshot(ROOT); unchanged(source, sourceAfter); }
    catch { sourceError = 'SOURCE_CHANGED_OR_UNAVAILABLE'; }
    const record = { format: 'zryna.stability-interruption.v1', registrySha256: REGISTRY_SHA256,
      source, sourceAfter, sourceError, host, lane, tools, signal: interruption.signal,
      status: 'interrupted', closureStatus: 'blocked', completedResults: results, current,
      uncollectedGates: gates.slice(results.length + (current ? 1 : 0)).map(gate => gate.id),
      blockers: [...registry.blockers, 'interrupted collection is incomplete and cannot qualify a lane'] };
    persist(Buffer.from(canonical(record)), output, 'interruption.json');
    console.log('Interrupted evidence preserved. No later gate launched; M7 closure remains blocked.');
    return record;
  };
  for (const gate of gates) {
    current = { id: gate.id, commandSha256: commandDigest(gate), observations: [] };
    if (interruption.signal) return interrupted();
    console.log(`[stability] ${gate.id}`);
    const attempts = [];
    const logs = [];
    const measured = gate.lane === 'performance';
    const count = measured ? (hasMemoryTool() ? gate.warmups + gate.samples : 0) : 1;
    for (let index = 0; index < count; index += 1) {
      if (interruption.signal) return interrupted();
      const executed = await execute(measured ? '/usr/bin/time' : gate.executable,
        measured ? ['-f', 'S418-MEMORY-KIB:%M', '--', gate.executable, ...gate.args] : gate.args,
        { root: ROOT, timeoutMs: gate.timeoutMs, interruption });
      const { stdout, stderr, ...attempt } = executed;
      const observation = { ...attempt,
        stdout: persist(stdout, output, `logs/${gate.id}-${index}.stdout`),
        stderr: persist(stderr, output, `logs/${gate.id}-${index}.stderr`) };
      current.observations.push(observation);
      if (interruption.signal) return interrupted();
      logs.push({ stdout: stdout.toString('utf8'), stderr: stderr.toString('utf8') });
      attempts.push({ ...observation, tests: counts(stdout.toString('utf8'), gate.proof) });
    }
    results.push({ id: gate.id, commandSha256: commandDigest(gate), attempts,
      assessment: assessGate(gate, attempts, logs) });
    console.log(`[stability] ${gate.id}: ${results.at(-1).assessment.status}`);
    current = null;
    if (interruption.signal) return interrupted();
    unchanged(source, snapshot(ROOT));
  }
  const receipt = { format: 'zryna.stability-evidence.v1', registrySha256: REGISTRY_SHA256,
    source, host, lane, tools, results, closureStatus: 'blocked',
    blockers: blockers(registry, lane, host, tools, results) };
  unchanged(source, snapshot(ROOT));
  validateReceipt(receipt, { registry, source, host, root: output });
  persist(Buffer.from(canonical(receipt)), output, 'receipt.json');
  console.log('Evidence preserved. M7 closure remains blocked; this is current-support preparation.');
  return receipt;
}

export async function collect(lane, output) {
  const interruption = createInterruptionScope();
  try { return await collectWithInterruption(lane, output, interruption); }
  finally { interruption.dispose(); }
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  try {
    if (process.argv.length !== 4) reject('usage: run.mjs compatibility|security|performance|all /absolute/new-output');
    const receipt = await collect(process.argv[2], process.argv[3]);
    if (receipt.format === 'zryna.stability-interruption.v1') {
      process.exitCode = receipt.signal === 'SIGINT' ? 130 : 143;
    } else if (receipt.results.length === 0 || receipt.results.some(result => result.assessment.status !== 'passed')) process.exitCode = 1;
  } catch (error) {
    console.error(error.message);
    process.exitCode = 1;
  }
}
