import { inflateRawSync } from 'node:zlib';

// VSIX metadata timestamps and entry order are not part of the extension's behavior. Preserve
// each complete local record, including compressed bytes and data descriptors, while sorting it.
export function canonicalVsix(input) {
  const entries = vsixEntries(input);
  const byLocal = [...entries].sort((a, b) => a.local - b.local);
  const byName = [...entries].sort((a, b) => a.name < b.name ? -1 : a.name > b.name ? 1 : 0);
  const directoryStart = input.readUInt32LE(input.length - 6);
  if (!byLocal.length || byLocal[0].local !== 0
    || byLocal.some((entry, index) => entry.local >= (byLocal[index + 1]?.local ?? directoryStart))) {
    throw new Error('VSIX local records overlap or are missing.');
  }
  const output = Buffer.alloc(input.length);
  const offsets = new Map();
  let cursor = 0;
  for (const entry of byName) {
    const index = byLocal.indexOf(entry);
    const end = byLocal[index + 1]?.local ?? directoryStart;
    input.copy(output, cursor, entry.local, end);
    offsets.set(entry.name, cursor);
    output.writeUInt16LE(0, cursor + 10);
    output.writeUInt16LE(33, cursor + 12);
    cursor += end - entry.local;
  }
  if (cursor !== directoryStart) throw new Error('VSIX local records differ from the directory.');
  for (const entry of byName) {
    const index = entries.indexOf(entry);
    const end = entries[index + 1]?.central ?? input.length - 22;
    input.copy(output, cursor, entry.central, end);
    output.writeUInt16LE(0, cursor + 12);
    output.writeUInt16LE(33, cursor + 14);
    output.writeUInt32LE(offsets.get(entry.name), cursor + 42);
    cursor += end - entry.central;
  }
  if (cursor !== input.length - 22) throw new Error('VSIX directory records differ from the ZIP end.');
  input.copy(output, cursor, input.length - 22);
  return output;
}

export function vsixEntries(bytes) {
  if (!Buffer.isBuffer(bytes) || bytes.length < 22 || bytes.length > 4 * 1024 * 1024) throw new Error('VSIX size.');
  const end = bytes.length - 22;
  if (bytes.readUInt32LE(end) !== 0x06054b50 || bytes.readUInt16LE(end + 20) !== 0
    || bytes.readUInt16LE(end + 4) !== 0 || bytes.readUInt16LE(end + 6) !== 0) throw new Error('VSIX ZIP end.');
  let cursor = bytes.readUInt32LE(end + 16);
  const count = bytes.readUInt16LE(end + 10);
  if (count > 64 || count !== bytes.readUInt16LE(end + 8)) throw new Error('VSIX count.');
  const result = [];
  for (let i = 0; i < count; i++) {
    if (cursor + 46 > end || bytes.readUInt32LE(cursor) !== 0x02014b50) throw new Error('VSIX directory.');
    const nameSize = bytes.readUInt16LE(cursor + 28);
    const extra = bytes.readUInt16LE(cursor + 30);
    const comment = bytes.readUInt16LE(cursor + 32);
    const local = bytes.readUInt32LE(cursor + 42);
    const size = bytes.readUInt32LE(cursor + 20);
    const expanded = bytes.readUInt32LE(cursor + 24);
    const method = bytes.readUInt16LE(cursor + 10);
    if (expanded > 1024 * 1024 || local + 30 > cursor || bytes.readUInt32LE(local) !== 0x04034b50
      || ![0, 8].includes(method) || (bytes.readUInt16LE(cursor + 8) & 1)) throw new Error('VSIX entry.');
    const name = bytes.subarray(cursor + 46, cursor + 46 + nameSize).toString('utf8');
    const start = local + 30 + bytes.readUInt16LE(local + 26) + bytes.readUInt16LE(local + 28);
    if (start + size > cursor || !/^[A-Za-z0-9[\]_./-]+$/.test(name)
      || name.split('/').includes('..') || result.some(entry => entry.name === name)) throw new Error('VSIX path.');
    const compressed = bytes.subarray(start, start + size);
    const data = method === 8 ? inflateRawSync(compressed, { maxOutputLength: 1024 * 1024 }) : compressed;
    if (data.length !== expanded) throw new Error('VSIX expansion.');
    result.push({ name, data, local, central: cursor });
    cursor += 46 + nameSize + extra + comment;
  }
  if (cursor !== end) throw new Error('VSIX trailing directory bytes.');
  return result;
}
