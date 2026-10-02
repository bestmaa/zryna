// Finite build inputs are captured without executing any staged material.
import { closeSync, constants, fstatSync, openSync, readSync } from 'node:fs';
import { isAbsolute, posix } from 'node:path';
import { orderedPaths, portablePath, sha256 } from '../../../../scripts/distribution/canonical.mjs';
import { exact, fail, isHash } from '../limits.mjs';

function release(descriptors, primary) {
  const errors = primary ? [primary] : [];
  for (const descriptor of descriptors.reverse()) {
    try { closeSync(descriptor); } catch (error) { errors.push(error); }
  }
  if (errors.length === 1) throw errors[0];
  if (errors.length) throw new AggregateError(errors, 'PLAYGROUND-BUILD-CLEANUP');
}

export function captureBuildInput(root, input, size, digest) {
  if (process.platform !== 'linux' || !isAbsolute(root) || posix.normalize(root) !== root ||
      !/^\/[A-Za-z0-9._/-]+$/.test(root) || !Number.isSafeInteger(size) ||
      size < 1 || size > 268435456 || !isHash(digest)) fail('BUILD-INPUT');
  portablePath(input);
  const descriptors = [];
  let data, primary;
  try {
    let parent = openSync('/', constants.O_RDONLY | constants.O_DIRECTORY | constants.O_NOFOLLOW);
    descriptors.push(parent);
    const parts = [...root.slice(1).split('/').filter(Boolean), ...input.split('/')];
    for (const part of parts.slice(0, -1)) {
      parent = openSync(`/proc/self/fd/${parent}/${part}`,
        constants.O_RDONLY | constants.O_DIRECTORY | constants.O_NOFOLLOW);
      descriptors.push(parent);
    }
    const descriptor = openSync(`/proc/self/fd/${parent}/${parts.at(-1)}`,
      constants.O_RDONLY | constants.O_NOFOLLOW | constants.O_NONBLOCK);
    descriptors.push(descriptor);
    const before = fstatSync(descriptor, { bigint: true });
    if (!before.isFile() || before.size !== BigInt(size) || before.mode & 0o6000n) fail('BUILD-INPUT');
    data = Buffer.alloc(size);
    let offset = 0;
    while (offset < size) {
      const count = readSync(descriptor, data, offset, Math.min(65536, size - offset), offset);
      if (!count) fail('BUILD-INPUT');
      offset += count;
    }
    const after = fstatSync(descriptor, { bigint: true });
    if (['dev', 'ino', 'size', 'mode', 'uid', 'mtimeNs', 'ctimeNs'].some(key => before[key] !== after[key]) ||
        sha256(data) !== digest) fail('BUILD-INPUT');
  } catch (error) { primary = error; }
  release(descriptors, primary);
  return data;
}

export function captureBuildFiles(root, files) {
  if (!Array.isArray(files) || !files.length || files.length > 128) fail('BUILD-INVENTORY');
  let total = 0;
  const inputs = new Set();
  for (const file of files) {
    exact(file, ['path', 'input', 'mode', 'bytes', 'sha256']);
    portablePath(file.path); portablePath(file.input);
    if (!Number.isSafeInteger(file.bytes) || file.bytes < 1 || file.bytes > 268435456 ||
        !isHash(file.sha256) || ![0o644, 0o755].includes(file.mode) || inputs.has(file.input) ||
        (total += file.bytes) > 536870912) fail('BUILD-INVENTORY');
    inputs.add(file.input);
  }
  orderedPaths(files.map(file => file.path));
  orderedPaths([...inputs].sort());
  return files.map(file => ({ path: file.path, mode: file.mode,
    data: captureBuildInput(root, file.input, file.bytes, file.sha256) }));
}
