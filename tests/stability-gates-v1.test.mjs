import assert from 'node:assert/strict';
import { mkdirSync, mkdtempSync, rmSync, symlinkSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { resolve } from 'node:path';
import test from 'node:test';
import { canonical, MAX_DOCUMENT, parse, portable, readSafe, sha256 } from '../scripts/stability-gates/input.mjs';
import { hostName, loadRegistry, REGISTRY_SHA256, select } from '../scripts/stability-gates/registry.mjs';
import { blockers, commandDigest, validateReceipt } from '../scripts/stability-gates/evidence.mjs';
import { assessGate, counts } from '../scripts/stability-gates/proof.mjs';
import { compare } from '../scripts/stability-gates/performance.mjs';

const registry = loadRegistry();
const host = 'linux-x86_64';
const source = { repository: 'https://github.com/zryna/zryna', commit: 'a'.repeat(40),
  tree: 'b'.repeat(40), inventorySha256: 'c'.repeat(64), files: 1, bytes: 10 };
const tap = '# tests 2\n# suites 0\n# pass 2\n# fail 0\n# cancelled 0\n# skipped 0\n# todo 0\n';
const rust = 'test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s\n';

// Synthetic transcripts test the independent validator. They are never published as runs.
function fixture(lane = 'compatibility') {
  const logs = new Map();
  const descriptor = (path, text) => {
    const bytes = Buffer.from(text);
    logs.set(path, bytes);
    return { path, size: bytes.length, sha256: sha256(bytes) };
  };
  const results = select(registry, lane, host).map(gate => {
    const stdout = gate.proof === 'node-tests' ? tap : rust +
      ({ m0: 'M0 conformance passed on linux: 18 commands, 13 registered proof suites.\n',
        m2: 'M2 conformance passed: 10 ordered gates.\n', m3: 'M3 full gate passed on linux/x64.\n' }[gate.proof] ?? '');
    const attempts = gate.lane === 'performance' ? [] : [{ exitCode: 0, signal: null, error: null,
      elapsedMs: 1, tests: counts(stdout, gate.proof),
      stdout: descriptor(`logs/${gate.id}-0.stdout`, stdout), stderr: descriptor(`logs/${gate.id}-0.stderr`, '') }];
    return { id: gate.id, commandSha256: commandDigest(gate), attempts,
      assessment: assessGate(gate, attempts, [{ stdout, stderr: '' }]) };
  });
  const tools = Object.entries(registry.toolchains).map(([name, version]) => ({ name, version, error: null }));
  const receipt = { format: 'zryna.stability-evidence.v1', registrySha256: REGISTRY_SHA256,
    source: structuredClone(source), host, lane, tools, results, closureStatus: 'blocked',
    blockers: blockers(registry, lane, host, tools, results) };
  const validate = () => validateReceipt(receipt, { registry, source, host, readLog: path => logs.get(path) });
  return { receipt, logs, validate };
}

test('freezes compatibility inventory, current exclusions and all prerequisite blockers', () => {
  assert.equal(registry.scope, 'current-support-preparation');
  assert.equal(registry.contracts.length, 7);
  for (const dependency of ['#414', '#416', '#417', 'M4-M6']) {
    assert(registry.blockers.some(reason => reason.includes(dependency)));
  }
  assert.equal(registry.gates.at(-1).thresholds, null);
  assert(!select(registry, 'all', 'windows-x86_64').some(gate => gate.id.includes('native')));
  assert.equal(hostName('win32', 'x64'), 'windows-x86_64');
  assert.throws(() => hostName('linux', 'arm64'), /unsupported host/);
  assert.throws(() => select(registry, 'waive', host), /unknown lane/);
});

test('accepts internally consistent receipts while retaining blocked closure', () => {
  for (const lane of ['compatibility', 'security', 'performance', 'all']) {
    const value = fixture(lane);
    assert.equal(value.validate(), value.receipt);
    assert.equal(value.receipt.closureStatus, 'blocked');
    assert(value.receipt.blockers.length >= registry.blockers.length);
  }
});

test('rejects stale sources, rewritten commands, missing/extra/duplicate gates and waived blockers', () => {
  for (const mutate of [
    value => { value.receipt.source.commit = 'd'.repeat(40); },
    value => { value.receipt.source.tree = 'd'.repeat(40); },
    value => { value.receipt.source.inventorySha256 = 'd'.repeat(64); },
    value => { value.receipt.registrySha256 = 'd'.repeat(64); },
    value => { value.receipt.results[0].commandSha256 = 'd'.repeat(64); },
    value => { value.receipt.results.pop(); },
    value => { value.receipt.results.push(value.receipt.results[0]); },
    value => { value.receipt.results[1] = value.receipt.results[0]; },
    value => { value.receipt.results.reverse(); },
    value => { value.receipt.blockers = []; },
    value => { value.receipt.closureStatus = 'passed'; },
    value => { value.receipt.exemption = 'approved'; },
    value => { value.receipt.tools.pop(); },
    value => { value.receipt.tools[0] = { name: 'node', version: null, error: 123 }; },
    value => { value.receipt.host = 'windows-x86_64'; },
  ]) {
    const value = fixture();
    mutate(value);
    assert.throws(value.validate);
  }
});

test('rejects log tampering, cross-attempt substitution, fabricated counts and false success', () => {
  for (const mutate of [
    value => { value.logs.set(value.receipt.results[0].attempts[0].stdout.path, Buffer.from('tampered')); },
    value => { value.receipt.results[0].attempts[0].stdout.path = 'logs/another-0.stdout'; },
    value => { value.receipt.results[0].attempts[0].stdout.size += 1; },
    value => { value.receipt.results[0].attempts[0].tests.passed += 100; },
    value => { value.receipt.results[0].attempts[0].exitCode = 1; },
    value => { value.receipt.results[0].attempts[0].error = 'ETIMEDOUT'; },
    value => { value.receipt.results[0].attempts[0].error = 123; },
    value => { value.receipt.results[0].attempts[0].signal = 'SIGKILL'; },
    value => { value.receipt.results[0].attempts[0].elapsedMs = Infinity; },
    value => { value.receipt.results[0].attempts[0].elapsedMs = 1e12; },
  ]) {
    const value = fixture();
    mutate(value);
    assert.throws(value.validate);
  }
});

test('empty, skipped, cancelled, ignored and unexecuted proof never qualifies', () => {
  assert.equal(counts('', 'node-tests').complete, false);
  assert.equal(counts('TAP version 13\n1..0\n', 'node-tests').complete, false);
  assert.equal(counts(tap.replace('# pass 2', '# pass 1').replace('# skipped 0', '# skipped 1'), 'node-tests').complete, false);
  assert.equal(counts(tap.replace('# pass 2', '# pass 1').replace('# cancelled 0', '# cancelled 1'), 'node-tests').complete, false);
  assert.equal(counts(rust.replace('0 ignored', '1 ignored'), 'rust-tests').complete, false);
  assert.equal(counts(rust.replace('1 passed', '0 passed'), 'rust-tests').complete, false);
  assert.deepEqual(counts('test result: FAILED. 322 passed; 1 failed; 2 ignored; 0 measured; 0 filtered out; finished in 1s\n', 'rust-tests'),
    { passed: 322, failed: 1, skipped: 2, complete: false });
  assert.equal(counts(rust, 'm2').complete, false);
  assert.throws(() => counts(tap.replace('# tests 2', '# tests 5'), 'node-tests'), /inconsistent TAP/);
});

test('performance rejects ceiling and variance regressions; no baseline never passes', () => {
  const samples = Array.from({ length: 5 }, () => ({ wallMs: 100, peakRssKiB: 1000 }));
  const thresholds = { wallMs: 100, peakRssKiB: 1000, maxSpread: 0.1 };
  assert.equal(compare(samples, null).status, 'blocked');
  assert.equal(compare(samples, thresholds).status, 'passed');
  assert.equal(compare(samples, { ...thresholds, peakRssKiB: 999 }).status, 'failed');
  samples[4].wallMs = 111;
  assert.equal(compare(samples, { ...thresholds, wallMs: 200 }).status, 'failed');
  assert.throws(() => compare(samples.slice(1), thresholds), /five measured/);
  assert.throws(() => compare([{ wallMs: NaN, peakRssKiB: 1 }, ...samples.slice(1)], thresholds), /positive finite/);
  assert.throws(() => compare(samples, { ...thresholds, waiver: true }), /fields/);
});

test('performance receipt cannot label missing measurements or an unreviewed baseline passed', () => {
  const value = fixture('performance');
  value.receipt.results[0].assessment = { status: 'passed', reason: 'fast enough' };
  assert.throws(value.validate, /false pass/);
});

test('measured performance receipts replay all six attempts and retain baseline blocker', () => {
  const value = fixture('performance');
  const gate = registry.gates.at(-1);
  const log = (path, text) => {
    const bytes = Buffer.from(text);
    value.logs.set(path, bytes);
    return { path, size: bytes.length, sha256: sha256(bytes) };
  };
  const result = value.receipt.results[0];
  result.attempts = Array.from({ length: 6 }, (_, index) => ({ exitCode: 0, signal: null,
    error: null, elapsedMs: 100, tests: counts(rust, gate.proof),
    stdout: log(`logs/${gate.id}-${index}.stdout`, rust),
    stderr: log(`logs/${gate.id}-${index}.stderr`, 'S418-MEMORY-KIB:1000\n') }));
  const logs = result.attempts.map(() => ({ stdout: rust, stderr: 'S418-MEMORY-KIB:1000\n' }));
  result.assessment = assessGate(gate, result.attempts, logs);
  value.receipt.blockers = blockers(registry, 'performance', host, value.receipt.tools, value.receipt.results);
  value.validate();
  assert.equal(result.assessment.status, 'blocked');
  assert.equal(result.assessment.summary.peakRssKiB.maximum, 1000);
  result.attempts.pop();
  assert.throws(value.validate, /complete performance attempt/);
  result.attempts.push(structuredClone(result.attempts[0]));
  assert.throws(value.validate, /attempt binding/);
});

test('canonical bounded JSON rejects duplicate keys, unknown representations and cycles', () => {
  assert.deepEqual(parse(Buffer.from(canonical({ z: 1, a: [] }))), { a: [], z: 1 });
  assert.throws(() => parse(Buffer.from('{"a":1,"a":1}\n')), /canonical/);
  assert.throws(() => parse(Buffer.alloc(MAX_DOCUMENT + 1)), /byte budget/);
  assert.throws(() => parse(Buffer.from([0xff])), /encoded data/);
  const cycle = {}; cycle.self = cycle;
  assert.throws(() => canonical(cycle), /cyclic/);
  assert.throws(() => canonical(Array.from({ length: 9000 }, () => 0)), /resource/);
  assert.throws(() => canonical({ oversized: 'x'.repeat(MAX_DOCUMENT + 1) }), /string byte budget/);
  let nested = []; for (let index = 0; index < 26; index += 1) nested = [nested];
  assert.throws(() => canonical(nested), /resource/);
});

test('evidence paths reject escapes, Windows devices and persistent directory links', () => {
  for (const path of ['../outside', '/absolute', 'logs/../outside', 'logs\\file', 'logs/NUL.txt', 'logs/x.', 'logs//file']) {
    assert.throws(() => portable(path), /unsafe path/);
  }
  const root = mkdtempSync(resolve(tmpdir(), 'zryna-stability-input-'));
  try {
    mkdirSync(resolve(root, 'real'));
    writeFileSync(resolve(root, 'real/data'), 'hello');
    assert.equal(readSafe(root, 'real/data').toString(), 'hello');
    assert.throws(() => readSafe(root, 'real/data', 4), /byte budget/);
    symlinkSync(resolve(root, 'real'), resolve(root, 'linked'), process.platform === 'win32' ? 'junction' : 'dir');
    assert.throws(() => readSafe(root, 'linked/data'), /regular files/);
    writeFileSync(resolve(root, 'registry.json'), '{}\n');
    mkdirSync(resolve(root, 'tests'));
    writeFileSync(resolve(root, 'tests/stability-gates-v1.json'), '{}\n');
    assert.throws(() => loadRegistry(root), /frozen authority/);
  } finally { rmSync(root, { recursive: true, force: true }); }
});
