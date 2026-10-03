import assert from 'node:assert/strict';
import { fork } from 'node:child_process';
import { createHash } from 'node:crypto';
import { EventEmitter, once } from 'node:events';
import { cpSync, existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { resolve } from 'node:path';
import { pathToFileURL } from 'node:url';
import test from 'node:test';
import { setTimeout as delay } from 'node:timers/promises';
import { createInterruptionScope } from '../scripts/stability-gates/interruption.mjs';
import { execute } from '../scripts/stability-gates/process.mjs';
import { assessAttempt } from '../scripts/stability-gates/proof.mjs';
import { git } from '../scripts/stability-gates/source.mjs';

const root = resolve(import.meta.dirname, '..');
const fixture = resolve(import.meta.dirname, 'stability-gates-fixtures/owned-tree.mjs');
const implementation = pathToFileURL(resolve(root, 'scripts/stability-gates/process.mjs')).href;

async function ready(path, child, diagnostic = () => '') {
  for (let index = 0; index < 250; index += 1) {
    if (existsSync(path)) return JSON.parse(readFileSync(path, 'utf8'));
    assert.equal(child.exitCode, null, `supervisor exited before readiness: ${diagnostic()}`);
    await delay(20);
  }
  throw new Error('owned fixture did not become ready within 5 seconds');
}
function gone(pid) {
  try { process.kill(pid, 0); return false; }
  catch (error) { if (error.code === 'ESRCH') return true; throw error; }
}
async function exercise(signal, detached = false, repeated = false) {
  const directory = mkdtempSync(resolve(tmpdir(), 'zryna-stability-interruption-'));
  const readyPath = resolve(directory, 'ready.json');
  const output = resolve(directory, 'result.json');
  const child = fork(fixture, ['supervisor', readyPath, output, implementation, String(detached)], {
    stdio: ['ignore', 'pipe', 'pipe', 'ipc'], windowsHide: true,
  });
  const closed = once(child, 'close');
  let owned;
  try {
    owned = await ready(readyPath, child);
    if (repeated) child.send({ signals: [signal, 'SIGTERM', signal] });
    else if (process.platform === 'win32') child.send({ signal });
    else process.kill(child.pid, signal);
    const [code, terminalSignal] = await closed;
    assert.equal(code, signal === 'SIGINT' ? 130 : 143);
    assert.equal(terminalSignal, null);
    const result = JSON.parse(readFileSync(output, 'utf8'));
    assert.equal(result.exitCode, null);
    assert.equal(result.signal, signal);
    assert.equal(result.error, 'ECANCELED');
    assert(result.elapsedMs > 0 && result.elapsedMs < 15_000);
    assert.deepEqual(result.after, result.before);
    assert(gone(owned.child), 'owned child survives cleanup');
    assert(gone(owned.grandchild), 'owned grandchild survives cleanup');
  } finally {
    // Only PIDs created by this private fixture are eligible for failure cleanup.
    for (const pid of owned ? Object.values(owned) : []) {
      if (!gone(pid)) try { process.kill(pid, 'SIGKILL'); } catch {}
    }
    if (child.exitCode === null && child.signalCode === null) child.kill('SIGKILL');
    rmSync(directory, { recursive: true, force: true });
  }
}

test('interruption scope retains the first signal and removes handlers idempotently', () => {
  const signals = new EventEmitter();
  const scope = createInterruptionScope(signals);
  const observed = [];
  const unsubscribe = scope.subscribe(signal => observed.push(signal));
  signals.emit('SIGINT'); signals.emit('SIGTERM'); signals.emit('SIGINT');
  assert.deepEqual(observed, ['SIGINT', 'SIGINT', 'SIGINT']);
  unsubscribe(); signals.emit('SIGTERM');
  scope.dispose(); scope.dispose();
  assert.equal(signals.listenerCount('SIGINT'), 0);
  assert.equal(signals.listenerCount('SIGTERM'), 0);
  assert.throws(() => scope.subscribe(() => {}));
});

test('SIGINT terminates the owned child and grandchild with failed terminal evidence', async () => {
  await exercise('SIGINT');
});

test('SIGTERM terminates descendants even in a separate Linux process group', async () => {
  await exercise('SIGTERM', true);
});

test('repeated interruptions preserve the first signal and do not leak cleanup handlers', async () => {
  await exercise('SIGINT', true, true);
  await exercise('SIGINT', false, true);
});

test('prior cancellation prevents process start and cannot qualify successful proof', async () => {
  const signals = new EventEmitter();
  const scope = createInterruptionScope(signals);
  signals.emit('SIGTERM');
  const result = await execute('zryna-stability-must-never-start-418', [],
    { root: tmpdir(), timeoutMs: 250, interruption: scope });
  assert.equal(result.error, 'ECANCELED');
  assert.equal(result.signal, 'SIGTERM');
  assert.equal(result.exitCode, null);
  assert.equal(assessAttempt(result, '# tests 1\n# suites 0\n# pass 1\n# fail 0\n# cancelled 0\n# skipped 0\n# todo 0\n', 'node-tests').status, 'failed');
  scope.dispose();
});

test('collector interruption preserves actual logs and never starts the later gates', async () => {
  // Synthetic repository/execution only: never published as candidate conformance evidence.
  const directory = mkdtempSync(resolve(tmpdir(), 'zryna-stability-collector-interruption-'));
  const repository = resolve(directory, 'source');
  const output = resolve(directory, 'evidence');
  const readyPath = resolve(directory, 'ready.json');
  mkdirSync(repository);
  cpSync(resolve(root, 'scripts/stability-gates'), resolve(repository, 'scripts/stability-gates'), { recursive: true });
  mkdirSync(resolve(repository, 'tests'));
  cpSync(resolve(root, 'tests/stability-gates-v1.json'), resolve(repository, 'tests/stability-gates-v1.json'));
  const registry = JSON.parse(readFileSync(resolve(root, 'tests/stability-gates-v1.json'), 'utf8'));
  for (const { authority } of registry.contracts) {
    mkdirSync(resolve(repository, authority, '..'), { recursive: true });
    cpSync(resolve(root, authority), resolve(repository, authority));
  }
  const contractFiles = ['diagnostics-protocol-v2.test.mjs', 'm2-manifest-contract.test.mjs',
    'm3-public-docs.test.mjs', 'provider-conformance-v4.test.mjs', 'distribution-receipt-compatibility.test.mjs'];
  for (const name of contractFiles) writeFileSync(resolve(repository, 'tests', name),
    name === contractFiles[0] ? `import { spawn } from 'node:child_process';
spawn(process.execPath, [${JSON.stringify(fixture)}, 'child', ${JSON.stringify(readyPath)}, '', '', 'true'], { stdio: 'inherit' });
setInterval(() => {}, 1000);
` : "import test from 'node:test'; test('synthetic fixture', () => {});\n");
  git(repository, ['init', '--quiet']); git(repository, ['add', '.']);
  git(repository, ['-c', 'user.name=Stability test', '-c', 'user.email=stability@example.invalid',
    '-c', 'commit.gpgsign=false', 'commit', '--quiet', '-m', 'synthetic interruption fixture']);
  const child = fork(fixture, ['collector', readyPath, output,
    pathToFileURL(resolve(repository, 'scripts/stability-gates/run.mjs')).href],
  { stdio: ['ignore', 'pipe', 'pipe', 'ipc'], windowsHide: true,
    env: { ...process.env, NODE_TEST_CONTEXT: undefined } });
  const closed = once(child, 'close');
  let stdout = ''; let stderr = ''; let owned;
  child.stdout.on('data', data => { stdout += data; });
  child.stderr.on('data', data => { stderr += data; });
  try {
    owned = await ready(readyPath, child, () => stderr);
    if (process.platform === 'win32') child.send({ signal: 'SIGTERM' });
    else process.kill(child.pid, 'SIGTERM');
    const [code] = await closed;
    assert.equal(code, 143, stderr);
    const record = JSON.parse(readFileSync(resolve(output, 'interruption.json'), 'utf8'));
    assert.equal(record.format, 'zryna.stability-interruption.v1');
    assert.equal(record.status, 'interrupted'); assert.equal(record.closureStatus, 'blocked');
    assert.equal(record.signal, 'SIGTERM');
    assert.deepEqual(record.sourceAfter, record.source); assert.equal(record.sourceError, null);
    assert.deepEqual(record.completedResults, []);
    assert.equal(record.current.id, 'compatibility-contracts');
    assert.equal(record.current.observations.length, 1);
    const observation = record.current.observations[0];
    assert.equal(observation.error, 'ECANCELED'); assert.equal(observation.signal, 'SIGTERM');
    assert.equal(observation.exitCode, null);
    assert(record.uncollectedGates.includes('compatibility-m1-portable'));
    assert(!existsSync(resolve(output, 'receipt.json')));
    assert(!stdout.includes('[stability] compatibility-m1-portable'));
    for (const log of [observation.stdout, observation.stderr]) {
      const bytes = readFileSync(resolve(output, log.path));
      assert.equal(bytes.length, log.size);
      assert.equal(createHash('sha256').update(bytes).digest('hex'), log.sha256);
    }
    assert(gone(owned.child)); assert(gone(owned.grandchild));
  } finally {
    for (const pid of owned ? Object.values(owned) : []) if (!gone(pid)) try { process.kill(pid, 'SIGKILL'); } catch {}
    if (child.exitCode === null && child.signalCode === null) child.kill('SIGKILL');
    rmSync(directory, { recursive: true, force: true });
  }
});
