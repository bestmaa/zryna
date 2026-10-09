import { spawnSync } from 'node:child_process';
import { lstatSync, readFileSync, readdirSync } from 'node:fs';
import { join, posix } from 'node:path';
import { fail, portablePath, LEGACY_SOURCE_EXTENSIONS, SOURCE_EXTENSIONS } from './policy.mjs';

export function git(root, args, input) {
  const result = spawnSync('git', args, { cwd: root, input, encoding: 'utf8',
    maxBuffer: 128 * 1024 * 1024, timeout: 30000, windowsHide: true,
    env: { ...process.env, GIT_OPTIONAL_LOCKS: '0', GIT_NO_REPLACE_OBJECTS: '1' } });
  if (result.error || result.status !== 0) fail(`Git comparison authority unavailable (${args[0]}): supply the required base/anchor history explicitly; ${result.error?.message ?? result.stderr.trim()}`);
  return result.stdout;
}

export function commit(root, ref) {
  if (!/^(HEAD|[a-f0-9]{40})$/.test(ref)) fail('base must be HEAD or one full immutable commit SHA');
  return git(root, ['rev-parse', '--verify', `${ref}^{commit}`]).trim();
}

export function tree(root, revision) {
  const result = new Map();
  for (const entry of git(root, ['ls-tree', '-rz', '--full-tree', revision]).split('\0').filter(Boolean)) {
    const [metadata, path] = entry.split('\t');
    portablePath(path);
    const [mode, type, hash] = metadata.split(' ');
    result.set(path, { mode, type, hash });
  }
  return result;
}

export function blobs(root, entries) {
  const hashes = [...new Set(entries.map(entry => entry.hash))];
  if (!hashes.length) return new Map();
  // Git's byte-size framing, not newline splitting, authenticates arbitrary UTF-8 source.
  const raw = spawnSync('git', ['cat-file', '--batch'], { cwd: root, input: `${hashes.join('\n')}\n`,
    maxBuffer: 128 * 1024 * 1024, timeout: 30000, windowsHide: true,
    env: { ...process.env, GIT_OPTIONAL_LOCKS: '0', GIT_NO_REPLACE_OBJECTS: '1' } });
  if (raw.error || raw.status !== 0) fail('cannot read trusted source blobs; supply complete base history');
  const result = new Map();
  let offset = 0;
  for (const hash of hashes) {
    const end = raw.stdout.indexOf(10, offset);
    const [actual, type, size] = raw.stdout.subarray(offset, end).toString().split(' ');
    if (actual !== hash || type !== 'blob' || !/^\d+$/.test(size)) fail('invalid trusted blob framing');
    offset = end + 1;
    const bytes = raw.stdout.subarray(offset, offset + Number(size));
    result.set(hash, new TextDecoder('utf-8', { fatal: true }).decode(bytes));
    offset += Number(size) + 1;
  }
  return result;
}

export function source(path) { return SOURCE_EXTENSIONS.some(extension => path.toLowerCase().endsWith(extension)); }
export function legacySource(path) { return LEGACY_SOURCE_EXTENSIONS.some(extension => path.toLowerCase().endsWith(extension)); }

export function readSafe(root, path, missing = false) {
  portablePath(path);
  let current = root;
  const parts = path.split('/');
  for (const [index, part] of parts.entries()) {
    current = join(current, part);
    const stat = lstatSync(current, { throwIfNoEntry: false });
    if (!stat && missing) return undefined;
    if (!stat || stat.isSymbolicLink() || (index === parts.length - 1 ? !stat.isFile() : !stat.isDirectory())) fail(`not a regular no-symlink path: ${path}`);
    if (index === parts.length - 1 && stat.size > 2 * 1024 * 1024) fail(`file exceeds inspection budget: ${path}`);
  }
  return new TextDecoder('utf-8', { fatal: true }).decode(readFileSync(current));
}

export function workingPaths(root) {
  // Ignore only declared build/dependency outputs, not .gitignore patterns or test directory names.
  const excluded = new Set(['.git', 'target', 'node_modules', 'adapters/typescript-6/node_modules', '.zryna/cache', '.zryna/out']);
  const paths = new Set(git(root, ['ls-files', '-z', '--cached']).split('\0').filter(Boolean));
  const pending = [''];
  let entries = 0;
  while (pending.length) {
    const directory = pending.pop();
    for (const entry of readdirSync(join(root, directory), { withFileTypes: true })) {
      if (++entries > 50000) fail('filesystem entry budget exceeded');
      const path = directory ? `${directory}/${entry.name}` : entry.name;
      portablePath(path);
      if (entry.isSymbolicLink()) fail(`not a regular no-symlink path: ${path}`);
      if (excluded.has(path)) {
        if (!entry.isDirectory() && !(path === '.git' && entry.isFile())) fail(`invalid excluded output: ${path}`);
        continue;
      }
      if (entry.isDirectory()) {
        if (path.split('/').length > 32) fail('filesystem depth budget exceeded');
        pending.push(path);
      } else if (entry.isFile()) paths.add(path);
      else fail(`not a regular path: ${path}`);
    }
  }
  const sorted = [...paths].sort();
  const seen = new Map();
  for (const path of sorted) {
    portablePath(path);
    const parts = path.split('/');
    for (let length = 1; length <= parts.length; length++) {
      const prefix = parts.slice(0, length).join('/');
      const folded = prefix.toLowerCase();
      if (seen.has(folded) && seen.get(folded) !== prefix) fail(`case-colliding path: ${path}`);
      seen.set(folded, prefix);
    }
  }
  return sorted;
}

export function renames(root, from, to) {
  const args = ['diff', '--no-ext-diff', '--no-textconv', '--name-status', '-z', '--find-renames=50%', from];
  if (to) args.push(to);
  args.push('--');
  const fields = git(root, args).split('\0').filter(Boolean);
  const result = new Map();
  for (let index = 0; index < fields.length;) {
    const status = fields[index++];
    const path = fields[index++];
    if (status.startsWith('R')) result.set(fields[index++], path);
  }
  return result;
}

export function navigation(root) {
  const markdown = readSafe(root, 'docs/CODE_NAVIGATION.md');
  const links = [...markdown.matchAll(/\[[^\]\n]+\]\(([^)\n]+)\)/g)];
  if (!links.length) fail('navigation has no declared targets');
  for (const [, target] of links) {
    if (!/^[A-Za-z0-9_.\/-]+$/.test(target) || target.startsWith('/')) fail(`unsupported navigation target: ${target}`);
    readSafe(root, portablePath(posix.normalize(posix.join('docs', target))));
  }
}
