import { createHash } from 'node:crypto';
import { closeSync, constants, fstatSync, mkdirSync, openSync, readSync, writeFileSync } from 'node:fs';
import { isAbsolute, posix, relative } from 'node:path';

const flags = constants.O_RDONLY | constants.O_DIRECTORY | constants.O_NOFOLLOW;
const fileKeys = ['dev', 'ino', 'size', 'mode', 'uid', 'gid', 'nlink', 'mtimeNs', 'ctimeNs'];
const fail = () => { throw new Error('protected documentation output identity or inventory rejected'); };
const digest = data => createHash('sha256').update(data).digest('hex');

function canonical(path) {
  if (typeof path !== 'string' || !isAbsolute(path) || posix.normalize(path) !== path ||
      !/^\/[A-Za-z0-9._/-]+$/.test(path)) fail();
}

function identity(descriptor) {
  const info = fstatSync(descriptor, { bigint: true });
  if (!info.isDirectory()) fail();
  return `${info.dev}:${info.ino}`;
}

function walk(path, owned) {
  const descriptor = openSync('/', flags);
  owned.push(descriptor);
  const rows = [{ descriptor, expected: identity(descriptor) }];
  for (const name of path.slice(1).split('/').filter(Boolean)) {
    const parent = rows.at(-1).descriptor;
    const child = openSync(`/proc/self/fd/${parent}/${name}`, flags);
    owned.push(child);
    rows.push({ descriptor: child, parent, name, expected: identity(child) });
  }
  return rows;
}

function revalidate(rows) {
  for (const row of rows) {
    if (identity(row.descriptor) !== row.expected) fail();
    const named = openSync(row.parent === undefined ? '/' : `/proc/self/fd/${row.parent}/${row.name}`, flags);
    let primary;
    try { if (identity(named) !== row.expected) fail(); }
    catch (error) { primary = error; }
    try { closeSync(named); }
    catch (error) {
      if (primary) throw new AggregateError([primary, error], 'protected documentation close failure');
      throw error;
    }
    if (primary) throw primary;
  }
}

function readback(files) {
  const chunk = Buffer.alloc(65536);
  for (const file of files) {
    const before = fstatSync(file.descriptor, { bigint: true });
    if (!before.isFile() || before.nlink !== 1n || before.size !== BigInt(file.bytes) ||
        fileKeys.some(key => before[key] !== file.observed[key])) fail();
    const hash = createHash('sha256');
    let offset = 0;
    while (offset < file.bytes) {
      const count = readSync(file.descriptor, chunk, 0, Math.min(chunk.length, file.bytes - offset), offset);
      if (!count) fail();
      hash.update(chunk.subarray(0, count)); offset += count;
    }
    const after = fstatSync(file.descriptor, { bigint: true });
    if (hash.digest('hex') !== file.sha256 || fileKeys.some(key => before[key] !== after[key])) fail();
    const named = openSync(`/proc/self/fd/${file.parent}/${file.name}`,
      constants.O_RDONLY | constants.O_NOFOLLOW | constants.O_NONBLOCK);
    let primary;
    try {
      const info = fstatSync(named, { bigint: true });
      if (fileKeys.some(key => before[key] !== info[key])) fail();
    } catch (error) { primary = error; }
    try { closeSync(named); }
    catch (error) {
      if (primary) throw new AggregateError([primary, error], 'protected documentation close failure');
      throw error;
    }
    if (primary) throw primary;
  }
}

// Failed partials remain review evidence. Every write uses retained no-follow directory handles.
export function retainProtectedDocsOutput(root, sourceRoot, outputs, beforeWrite = () => {}) {
  if (process.platform !== 'linux') throw new Error('protected documentation output requires Linux');
  canonical(root); canonical(sourceRoot);
  const fromSource = relative(sourceRoot, root);
  if (!fromSource.startsWith('../') || !Array.isArray(outputs) || !outputs.length || outputs.length > 514) fail();
  const names = new Set();
  let total = 0;
  for (const [name, data] of outputs) {
    if (typeof name !== 'string' || !/^[A-Za-z0-9._/-]+$/.test(name) ||
        name.split('/').some(part => !part || part === '.' || part === '..') || name.length > 240 ||
        names.has(name) || !(data instanceof Uint8Array) || data.length > 2097152 ||
        (total += data.length) > 34603136) fail();
    names.add(name);
  }
  for (const name of names) {
    if (name.split('/').slice(0, -1).some((_part, index, parts) => names.has(parts.slice(0, index + 1).join('/')))) fail();
  }
  const owned = [];
  let primary;
  try {
    const source = walk(sourceRoot, owned), parent = walk(posix.dirname(root), owned);
    if (parent.some(row => row.expected === source.at(-1).expected)) fail();
    revalidate(source); revalidate(parent);
    const name = posix.basename(root), parentDescriptor = parent.at(-1).descriptor;
    mkdirSync(`/proc/self/fd/${parentDescriptor}/${name}`, { mode: 0o700 });
    const descriptor = openSync(`/proc/self/fd/${parentDescriptor}/${name}`, flags);
    owned.push(descriptor);
    const output = [...parent, { descriptor, parent: parentDescriptor, name, expected: identity(descriptor) }];
    const observed = fstatSync(descriptor);
    if (observed.uid !== process.getuid() || (observed.mode & 0o7777) !== 0o700) fail();
    const directories = new Map([['', descriptor]]), files = [];
    for (const [index, [portable, data]] of outputs.entries()) {
      beforeWrite(index);
      revalidate(source); revalidate(output); readback(files);
      const parts = portable.split('/'), leaf = parts.pop();
      let prefix = '', container = descriptor;
      for (const part of parts) {
        prefix = prefix ? `${prefix}/${part}` : part;
        if (!directories.has(prefix)) {
          if (directories.size >= 4096) fail();
          mkdirSync(`/proc/self/fd/${container}/${part}`, { mode: 0o700 });
          const child = openSync(`/proc/self/fd/${container}/${part}`, flags);
          owned.push(child);
          output.push({ descriptor: child, parent: container, name: part, expected: identity(child) });
          directories.set(prefix, child);
        }
        container = directories.get(prefix);
      }
      revalidate(source); revalidate(output);
      const file = openSync(`/proc/self/fd/${container}/${leaf}`,
        constants.O_RDWR | constants.O_CREAT | constants.O_EXCL | constants.O_NOFOLLOW, 0o600);
      owned.push(file);
      writeFileSync(file, data);
      files.push({ descriptor: file, parent: container, name: leaf, bytes: data.length,
        sha256: digest(data), observed: fstatSync(file, { bigint: true }) });
      readback(files); revalidate(source); revalidate(output);
    }
  } catch (error) { primary = error; }
  const errors = primary ? [primary] : [];
  for (const descriptor of owned.reverse()) {
    try { closeSync(descriptor); } catch (error) { errors.push(error); }
  }
  if (errors.length === 1) throw errors[0];
  if (errors.length) throw new AggregateError(errors, 'protected documentation output cleanup failure');
}
