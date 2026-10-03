import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { canonical, portable, readSafe, reject, sha256 } from './input.mjs';

export function git(root, args) {
  const result = spawnSync('git', args, {
    cwd: root, shell: false, encoding: 'utf8', timeout: 30_000, maxBuffer: 16 * 1024 * 1024,
  });
  if (result.error || result.status !== 0 || result.signal) reject('Git source inspection failed');
  return result.stdout;
}

export function snapshot(root) {
  const commit = git(root, ['rev-parse', 'HEAD']).trim();
  const tree = git(root, ['rev-parse', 'HEAD^{tree}']).trim();
  assert.match(commit, /^[a-f0-9]{40}$/, 'S418: full source commit');
  assert.match(tree, /^[a-f0-9]{40}$/, 'S418: full source tree');
  if (git(root, ['status', '--porcelain=v1', '--untracked-files=all']).length) reject('clean source required');
  const entries = git(root, ['ls-files', '--stage', '-z']).split('\0').filter(Boolean);
  if (entries.length === 0 || entries.length > 50_000) reject('source file budget');
  const paths = new Set();
  const inventory = [];
  let total = 0;
  for (const entry of entries) {
    const match = /^(100644|100755) ([a-f0-9]{40}) 0\t(.+)$/.exec(entry);
    if (!match) reject('regular stage-zero source required');
    const [, mode, blob, path] = match;
    portable(path);
    if (paths.has(path.toLowerCase())) reject('source path collision');
    paths.add(path.toLowerCase());
    const bytes = readSafe(root, path, 2 * 1024 * 1024);
    total += bytes.length;
    if (total > 64 * 1024 * 1024) reject('aggregate source byte budget');
    const actual = createHash('sha1').update(`blob ${bytes.length}\0`).update(bytes).digest('hex');
    if (actual !== blob) reject(`source differs from Git blob: ${path}`);
    inventory.push(`${mode} ${blob} ${sha256(bytes)} ${path}\n`);
  }
  // Also defeats staged source/index drift hidden by assume-unchanged or skip-worktree flags.
  if (git(root, ['write-tree']).trim() !== tree) reject('index differs from candidate tree');
  if (git(root, ['rev-parse', 'HEAD']).trim() !== commit) reject('candidate moved during inspection');
  return { repository: 'https://github.com/zryna/zryna', commit, tree,
    inventorySha256: sha256(inventory.join('')), files: entries.length, bytes: total };
}

export function unchanged(before, after) {
  if (canonical(before) !== canonical(after)) reject('source changed during evidence collection');
}
