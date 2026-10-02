// Output authority retains each no-follow directory and checks named reachability before writes.
import { createHash } from 'node:crypto';
import { closeSync, constants, fstatSync, mkdirSync, openSync, readSync, writeFileSync } from 'node:fs';
import { isAbsolute, posix, relative } from 'node:path';
import { fail } from '../limits.mjs';

const directoryFlags = constants.O_RDONLY | constants.O_DIRECTORY | constants.O_NOFOLLOW;

function canonical(path) {
  if (typeof path !== 'string' || !isAbsolute(path) || posix.normalize(path) !== path ||
      !/^\/[A-Za-z0-9._/-]+$/.test(path)) fail('BUILD-OUTPUT');
}

function identity(descriptor) {
  const value = fstatSync(descriptor, { bigint: true });
  if (!value.isDirectory()) fail('BUILD-OUTPUT');
  return `${value.dev}:${value.ino}`;
}

function walk(path, owned) {
  const root = openSync('/', directoryFlags);
  owned.push(root);
  const rows = [{ descriptor: root, expected: identity(root) }];
  for (const name of path.slice(1).split('/').filter(Boolean)) {
    const parent = rows.at(-1).descriptor;
    const descriptor = openSync(`/proc/self/fd/${parent}/${name}`, directoryFlags);
    owned.push(descriptor);
    rows.push({ descriptor, parent, name, expected: identity(descriptor) });
  }
  return rows;
}

function revalidate(rows) {
  for (const row of rows) {
    if (identity(row.descriptor) !== row.expected) fail('BUILD-OUTPUT-SUBSTITUTION');
    const descriptor = openSync(row.parent === undefined ? '/' :
      `/proc/self/fd/${row.parent}/${row.name}`, directoryFlags);
    let primary;
    try { if (identity(descriptor) !== row.expected) fail('BUILD-OUTPUT-SUBSTITUTION'); }
    catch (error) { primary = error; }
    try { closeSync(descriptor); } catch (error) {
      if (primary) throw new AggregateError([primary, error], 'PLAYGROUND-BUILD-CLEANUP');
      throw error;
    }
    if (primary) throw primary;
  }
}

const fileKeys = ['dev', 'ino', 'size', 'mode', 'uid', 'gid', 'nlink', 'mtimeNs', 'ctimeNs'];

function fileReadback(files, directory) {
  const chunk = Buffer.alloc(65536);
  for (const file of files) {
    const before = fstatSync(file.descriptor, { bigint: true });
    if (!before.isFile() || before.nlink !== 1n || before.size !== BigInt(file.bytes) ||
        fileKeys.some(key => before[key] !== file.observed[key])) fail('BUILD-OUTPUT-SUBSTITUTION');
    const hash = createHash('sha256');
    let offset = 0;
    while (offset < file.bytes) {
      const count = readSync(file.descriptor, chunk, 0, Math.min(chunk.length, file.bytes - offset), offset);
      if (!count) fail('BUILD-OUTPUT-SUBSTITUTION');
      hash.update(chunk.subarray(0, count)); offset += count;
    }
    const after = fstatSync(file.descriptor, { bigint: true });
    if (hash.digest('hex') !== file.sha256 || fileKeys.some(key => after[key] !== before[key])) {
      fail('BUILD-OUTPUT-SUBSTITUTION');
    }
    const named = openSync(`/proc/self/fd/${directory}/${file.name}`,
      constants.O_RDONLY | constants.O_NOFOLLOW | constants.O_NONBLOCK);
    let primary;
    try {
      const actual = fstatSync(named, { bigint: true });
      if (fileKeys.some(key => actual[key] !== before[key])) fail('BUILD-OUTPUT-SUBSTITUTION');
    } catch (error) { primary = error; }
    try { closeSync(named); } catch (error) {
      if (primary) throw new AggregateError([primary, error], 'PLAYGROUND-BUILD-CLEANUP');
      throw error;
    }
    if (primary) throw primary;
  }
}

function write(directory, name, data, owned) {
  const descriptor = openSync(`/proc/self/fd/${directory}/${name}`,
    constants.O_RDWR | constants.O_CREAT | constants.O_EXCL | constants.O_NOFOLLOW, 0o600);
  owned.push(descriptor);
  const expected = createHash('sha256').update(data).digest('hex');
  writeFileSync(descriptor, data);
  return { descriptor, name, bytes: data.length, sha256: expected,
    observed: fstatSync(descriptor, { bigint: true }) };
}

export function retainOutput(root, sourceRoot, outputs, completion, beforeWrite = () => {}) {
  if (process.platform !== 'linux') fail('BUILD-OUTPUT-HOST');
  canonical(root); canonical(sourceRoot);
  const fromSource = relative(sourceRoot, root);
  if (fromSource === '' || !fromSource.startsWith('../')) fail('BUILD-OUTPUT');
  const owned = [];
  let primary;
  try {
    const source = walk(sourceRoot, owned);
    const parent = walk(posix.dirname(root), owned);
    const sourceIdentity = source.at(-1).expected;
    if (parent.some(row => row.expected === sourceIdentity)) fail('BUILD-OUTPUT');
    revalidate(source); revalidate(parent);
    const name = posix.basename(root);
    const parentDescriptor = parent.at(-1).descriptor;
    mkdirSync(`/proc/self/fd/${parentDescriptor}/${name}`, { mode: 0o700 });
    const descriptor = openSync(`/proc/self/fd/${parentDescriptor}/${name}`, directoryFlags);
    owned.push(descriptor);
    const row = { descriptor, parent: parentDescriptor, name, expected: identity(descriptor) };
    const output = [...parent, row];
    const mode = fstatSync(descriptor);
    if (mode.uid !== process.getuid() || (mode.mode & 0o7777) !== 0o700 ||
        row.expected === sourceIdentity) fail('BUILD-OUTPUT');
    const files = [];
    for (const [index, [file, data]] of [...outputs, ['assembly-receipt.json', completion]].entries()) {
      if (!/^[A-Za-z0-9._-]+$/.test(file)) fail('BUILD-OUTPUT');
      beforeWrite(index);
      revalidate(source); revalidate(output);
      fileReadback(files, descriptor);
      files.push(write(descriptor, file, data, owned));
      fileReadback(files, descriptor);
      revalidate(source); revalidate(output);
    }
  } catch (error) { primary = error; }
  const errors = primary ? [primary] : [];
  for (const descriptor of owned.reverse()) {
    try { closeSync(descriptor); } catch (error) { errors.push(error); }
  }
  // Failed directories are retained; never delete, reopen, overwrite or fall back to a new location.
  if (errors.length === 1) throw errors[0];
  if (errors.length) throw new AggregateError(errors, 'PLAYGROUND-BUILD-CLEANUP');
}
