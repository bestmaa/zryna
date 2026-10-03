import assert from 'node:assert/strict';
import { mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { resolve } from 'node:path';
import test from 'node:test';
import { git, snapshot, unchanged } from '../scripts/stability-gates/source.mjs';
import { execute } from '../scripts/stability-gates/process.mjs';

function repository() {
  const root = mkdtempSync(resolve(tmpdir(), 'zryna-stability-source-'));
  git(root, ['init', '--quiet']);
  writeFileSync(resolve(root, 'input.zry'), 'export function answer(): i32 { return 42; }\n');
  git(root, ['add', 'input.zry']);
  git(root, ['-c', 'user.name=Stability test', '-c', 'user.email=stability@example.invalid',
    '-c', 'commit.gpgsign=false', 'commit', '--quiet', '-m', 'fixture']);
  return root;
}

test('source inventory binds a clean tree and detects hidden assume-unchanged mutations', () => {
  const root = repository();
  try {
    const first = snapshot(root);
    assert.equal(first.files, 1);
    unchanged(first, snapshot(root));
    git(root, ['update-index', '--assume-unchanged', 'input.zry']);
    writeFileSync(resolve(root, 'input.zry'), 'replaced source\n');
    assert.equal(git(root, ['status', '--porcelain=v1']), '');
    assert.throws(() => snapshot(root), /differs from Git blob/);
    assert.throws(() => unchanged(first, { ...first, tree: 'a'.repeat(40) }), /source changed/);
  } finally { rmSync(root, { recursive: true, force: true }); }
});

test('source inventory refuses untracked and staged input drift', () => {
  const root = repository();
  try {
    writeFileSync(resolve(root, 'extra.zry'), 'unexpected');
    assert.throws(() => snapshot(root), /clean source/);
    git(root, ['add', 'extra.zry']);
    assert.throws(() => snapshot(root), /clean source/);
  } finally { rmSync(root, { recursive: true, force: true }); }
});

test('bounded process preserves literal argument vectors and actual failure codes', async () => {
  const result = await execute(process.execPath, ['-e', 'console.log(process.argv[1]); process.exit(7)',
    '$(echo forbidden); literal'], { root: tmpdir(), timeoutMs: 5000 });
  assert.equal(result.stdout.toString(), '$(echo forbidden); literal\n');
  assert.equal(result.exitCode, 7);
  assert.equal(result.error, null);
  const missing = await execute('zryna-stability-missing-executable-418', [], { root: tmpdir(), timeoutMs: 5000 });
  assert.equal(missing.error, 'ENOENT');
  assert.equal(missing.exitCode, null);
});

test('deadline terminates a live process and output exhaustion fails closed', async () => {
  const timeout = await execute(process.execPath, ['-e', 'setInterval(() => {}, 1000)'],
    { root: tmpdir(), timeoutMs: 250 });
  assert(['ETIMEDOUT', 'CLEANUP'].includes(timeout.error));
  assert(timeout.elapsedMs < 15_000);
  const output = await execute(process.execPath, ['-e', 'process.stdout.write("x".repeat(17 * 1024 * 1024))'],
    { root: tmpdir(), timeoutMs: 5000 });
  assert.equal(output.error, 'MAXOUTPUT');
  assert(output.stdout.length <= 16 * 1024 * 1024);
});
