import assert from 'node:assert/strict';
import { exact, canonical, MAX_LOG, readSafe, reject, sha256 } from './input.mjs';
import { REGISTRY_SHA256, select } from './registry.mjs';
import { assessGate, counts } from './proof.mjs';

export function commandDigest(gate) { return sha256(canonical(gate)); }

export function blockers(registry, lane, host, tools, results) {
  const reasons = [...registry.blockers];
  if (lane !== 'all') reasons.push(`uncollected lanes outside ${lane}`);
  reasons.push(`other supported host evidence required beyond ${host}`);
  if (host === 'windows-x86_64') reasons.push('Linux native proof and memory baseline required');
  for (const [name, version] of Object.entries(registry.toolchains)) {
    const tool = tools.find(entry => entry.name === name);
    if (!tool || tool.version !== version) reasons.push(`pinned ${name} ${version} unavailable`);
  }
  for (const result of results) {
    if (result.assessment.status !== 'passed') reasons.push(`${result.id}: ${result.assessment.reason}`);
  }
  return reasons;
}

function logDescriptor(descriptor, expectedPath, root, readLog) {
  exact(descriptor, ['path', 'size', 'sha256'], 'log descriptor');
  assert.equal(descriptor.path, expectedPath, 'S418: log name/attempt binding');
  assert.match(descriptor.sha256, /^[a-f0-9]{64}$/, 'S418: log SHA-256');
  if (!Number.isSafeInteger(descriptor.size) || descriptor.size < 0 || descriptor.size > MAX_LOG) reject('log size budget');
  const bytes = readLog ? readLog(descriptor.path) : readSafe(root, descriptor.path, MAX_LOG);
  assert.equal(bytes.length, descriptor.size, 'S418: log size mismatch');
  assert.equal(sha256(bytes), descriptor.sha256, 'S418: log digest mismatch');
  return bytes.toString('utf8');
}

export function validateReceipt(receipt, { registry, source, host, root, readLog } = {}) {
  canonical(receipt);
  exact(receipt, ['format', 'registrySha256', 'source', 'host', 'lane', 'tools', 'results',
    'closureStatus', 'blockers'], 'receipt');
  assert.equal(receipt.format, 'zryna.stability-evidence.v1');
  assert.equal(receipt.registrySha256, REGISTRY_SHA256, 'S418: registry binding');
  assert.deepEqual(receipt.source, source, 'S418: exact candidate source binding');
  assert.equal(receipt.host, host, 'S418: evidence host binding');
  assert.equal(receipt.closureStatus, 'blocked', 'S418: preparation cannot close M7');
  const gates = select(registry, receipt.lane, host);
  assert(Array.isArray(receipt.tools), 'S418: tool observations required');
  assert.deepEqual(receipt.tools.map(tool => tool.name), Object.keys(registry.toolchains), 'S418: tool inventory');
  for (const tool of receipt.tools) {
    exact(tool, ['name', 'version', 'error'], 'tool');
    assert(tool.version === null || typeof tool.version === 'string' && tool.version.length > 0 && tool.version.length <= 128, 'S418: tool version');
    assert(tool.error === null || typeof tool.error === 'string' && /^[A-Z0-9_]{1,32}$/.test(tool.error), 'S418: tool error');
    assert(tool.version === null ? tool.error !== null : tool.error === null, 'S418: tool observation consistency');
  }
  assert(Array.isArray(receipt.results), 'S418: result inventory required');
  assert.deepEqual(receipt.results.map(result => result.id), gates.map(gate => gate.id), 'S418: complete ordered gate inventory');
  for (const [index, result] of receipt.results.entries()) {
    const gate = gates[index];
    exact(result, ['id', 'commandSha256', 'attempts', 'assessment'], 'gate result');
    assert.equal(result.commandSha256, commandDigest(gate), 'S418: exact command binding');
    assert(Array.isArray(result.attempts) && result.attempts.length <= 6, 'S418: bounded attempts');
    const logs = result.attempts.map((attempt, attemptIndex) => {
      exact(attempt, ['exitCode', 'signal', 'error', 'elapsedMs', 'tests', 'stdout', 'stderr'], 'attempt');
      assert(attempt.exitCode === null || Number.isSafeInteger(attempt.exitCode) && attempt.exitCode >= 0, 'S418: exit code');
      assert(attempt.signal === null || /^SIG[A-Z0-9]{1,20}$/.test(attempt.signal), 'S418: signal');
      assert(attempt.error === null || typeof attempt.error === 'string' && /^[A-Z0-9_]{1,32}$/.test(attempt.error), 'S418: process error');
      assert(Number.isFinite(attempt.elapsedMs) && attempt.elapsedMs > 0, 'S418: elapsed time');
      // A process may exceed its deadline slightly while termination is observed.
      assert(attempt.elapsedMs <= gate.timeoutMs + 60_000, 'S418: elapsed-time ceiling');
      const prefix = `logs/${gate.id}-${attemptIndex}`;
      const stdout = logDescriptor(attempt.stdout, `${prefix}.stdout`, root, readLog);
      const stderr = logDescriptor(attempt.stderr, `${prefix}.stderr`, root, readLog);
      assert.deepEqual(attempt.tests, counts(stdout, gate.proof), 'S418: independently replayed test counts');
      return { stdout, stderr };
    });
    assert.deepEqual(result.assessment, assessGate(gate, result.attempts, logs), 'S418: evidence cannot claim false pass');
  }
  assert.deepEqual(receipt.blockers, blockers(registry, receipt.lane, host, receipt.tools, receipt.results), 'S418: blockers cannot be waived');
  return receipt;
}
