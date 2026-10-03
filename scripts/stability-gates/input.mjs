import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { closeSync, constants, fstatSync, lstatSync, openSync, readSync } from 'node:fs';
import { resolve } from 'node:path';
import { TextDecoder } from 'node:util';

export const MAX_DOCUMENT = 256 * 1024;
export const MAX_LOG = 16 * 1024 * 1024;
export const sha256 = bytes => createHash('sha256').update(bytes).digest('hex');
export const reject = message => { throw new Error(`S418: ${message}`); };

export function exact(value, keys, label) {
  assert(value && typeof value === 'object' && !Array.isArray(value), `S418: ${label} object`);
  assert.deepEqual(Object.keys(value).sort(), [...keys].sort(), `S418: ${label} fields`);
}

export function portable(path) {
  if (typeof path !== 'string' || path.length > 240) reject('bounded portable path required');
  for (const part of path.split('/')) {
    if (!/^[A-Za-z0-9_.][A-Za-z0-9_.-]*$/.test(part) || part === '.' || part === '..' || part.endsWith('.') ||
        /^(con|prn|aux|nul|com[0-9]|lpt[0-9])(?:\.|$)/i.test(part)) reject('unsafe path');
  }
  return path;
}

function identity(a, b) {
  return a.dev === b.dev && a.ino === b.ino && a.size === b.size &&
    a.mtimeMs === b.mtimeMs && a.ctimeMs === b.ctimeMs;
}

// Trusted private ancestors are required. This detects persistent links and final-file races,
// not arbitrary hostile ancestor swaps; it is an evidence reader, not an OS sandbox.
export function readSafe(root, path, limit = MAX_DOCUMENT) {
  portable(path);
  let current = resolve(root);
  const rootStat = lstatSync(current);
  if (!rootStat.isDirectory() || rootStat.isSymbolicLink()) reject('unsafe evidence root');
  const parts = path.split('/');
  for (const [index, part] of parts.entries()) {
    current = resolve(current, part);
    const stat = lstatSync(current);
    if (stat.isSymbolicLink() || (index < parts.length - 1 ? !stat.isDirectory() : !stat.isFile())) {
      reject('evidence must use regular files and directories');
    }
  }
  const before = lstatSync(current);
  if (before.size > limit) reject('file exceeds byte budget');
  const fd = openSync(current, constants.O_RDONLY | (constants.O_NOFOLLOW ?? 0) |
    (constants.O_NONBLOCK ?? 0));
  try {
    const opened = fstatSync(fd);
    if (!opened.isFile() || !identity(before, opened)) reject('file replaced before read');
    const bytes = Buffer.alloc(before.size + 1);
    let count = 0;
    while (count < bytes.length) {
      const received = readSync(fd, bytes, count, bytes.length - count, null);
      if (received === 0) break;
      count += received;
    }
    if (count !== before.size || !identity(opened, fstatSync(fd)) ||
        !identity(opened, lstatSync(current))) reject('unstable evidence file');
    return bytes.subarray(0, count);
  } finally { closeSync(fd); }
}

function bounded(value) {
  const pending = [{ value, depth: 0 }];
  const seen = new WeakSet();
  let count = 0;
  let stringBytes = 0;
  while (pending.length) {
    const entry = pending.pop();
    if (++count > 8192 || entry.depth > 24) reject('JSON resource budget');
    if (entry.value !== null && typeof entry.value === 'object') {
      if (seen.has(entry.value)) reject('cyclic JSON');
      seen.add(entry.value);
      for (const [key, child] of Object.entries(entry.value)) {
        stringBytes += Buffer.byteLength(key);
        pending.push({ value: child, depth: entry.depth + 1 });
      }
    } else if (typeof entry.value === 'string') {
      stringBytes += Buffer.byteLength(entry.value);
    } else if (typeof entry.value === 'number' && !Number.isFinite(entry.value)) reject('nonfinite JSON');
    else if (entry.value !== null && !['string', 'number', 'boolean'].includes(typeof entry.value)) {
      reject('non-JSON value');
    }
    if (stringBytes > MAX_DOCUMENT) reject('JSON string byte budget');
  }
}

function encode(value) {
  if (Array.isArray(value)) return `[${value.map(encode).join(',')}]`;
  if (value !== null && typeof value === 'object') {
    return `{${Object.keys(value).sort().map(key => `${JSON.stringify(key)}:${encode(value[key])}`).join(',')}}`;
  }
  return JSON.stringify(value);
}

export function canonical(value) {
  bounded(value);
  const text = `${encode(value)}\n`;
  if (Buffer.byteLength(text) > MAX_DOCUMENT) reject('document byte budget');
  return text;
}

export function parse(bytes) {
  if (bytes.length > MAX_DOCUMENT) reject('document byte budget');
  const text = new TextDecoder('utf-8', { fatal: true }).decode(bytes);
  const document = JSON.parse(text);
  if (canonical(document) !== text) reject('canonical JSON required (no duplicate keys)');
  return document;
}
