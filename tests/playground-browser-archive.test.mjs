import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { deflateRawSync } from 'node:zlib';
import test from 'node:test';
import { encodeZip, crc32 } from '../scripts/distribution/archive-zip.mjs';
import { verifyBrowserMembers } from '../examples/playground/restricted/browser-archive.mjs';

// Tiny synthetic archives exercise byte validation; none contains a browser executable.
const root = 'chrome-linux64';
const hash = data => createHash('sha256').update(data).digest('hex');
const json = value => Buffer.from(JSON.stringify(value) + '\n');
function fixture(overrides = {}) {
  const files = [{ path: 'chrome', mode: 0o755, data: Buffer.from('synthetic chrome\n') },
    { path: 'data00', mode: 0o644, data: Buffer.alloc(0) },
    { path: 'folder/a.txt', mode: 0o644, data: Buffer.from('synthetic data\n') }]
    .map(file => ({ ...file, data: overrides[file.path] ?? file.data }));
  return { archive: encodeZip(root, files), inventory: files.map(file => ({
    path: `${root}/${file.path}`, bytes: file.data.length, sha256: hash(file.data),
  })) };
}

function entries(archive) {
  const eocd = archive.length - 22;
  let cursor = archive.readUInt32LE(eocd + 16);
  return Array.from({ length: archive.readUInt16LE(eocd + 10) }, () => {
    const central = cursor, local = archive.readUInt32LE(cursor + 42);
    const length = archive.readUInt16LE(cursor + 28);
    const name = archive.subarray(cursor + 46, cursor + 46 + length).toString('ascii');
    const dataStart = local + 30 + archive.readUInt16LE(local + 26) + archive.readUInt16LE(local + 28);
    const dataEnd = dataStart + archive.readUInt32LE(cursor + 20);
    cursor += 46 + length + archive.readUInt16LE(cursor + 30) + archive.readUInt16LE(cursor + 32);
    return { central, local, name, dataStart, dataEnd };
  });
}
const member = (archive, path) => entries(archive).find(entry => entry.name === `${root}/${path}`);
function rename(archive, entry, name) {
  assert.equal(name.length, entry.name.length);
  archive.write(name, entry.central + 46, 'ascii');
  archive.write(name, entry.local + 30, 'ascii');
}

function insert(archive, position, data) {
  const rows = entries(archive), eocd = archive.length - 22;
  const output = Buffer.concat([archive.subarray(0, position), data, archive.subarray(position)]);
  for (const row of rows) output.writeUInt32LE(row.local + (row.local >= position ? data.length : 0),
    row.central + data.length + 42);
  output.writeUInt32LE(archive.readUInt32LE(eocd + 16) + data.length, eocd + data.length + 16);
  return output;
}

function compressedFixture(trailing = false) {
  const { archive, inventory } = fixture();
  const row = member(archive, 'chrome'), rows = entries(archive), eocd = archive.length - 22;
  let wire = deflateRawSync(archive.subarray(row.dataStart, row.dataEnd));
  if (trailing) wire = Buffer.concat([wire, Buffer.from([0])]);
  const delta = wire.length - (row.dataEnd - row.dataStart);
  const output = Buffer.concat([archive.subarray(0, row.dataStart), wire, archive.subarray(row.dataEnd)]);
  for (const entry of rows) output.writeUInt32LE(entry.local + (entry.local >= row.dataEnd ? delta : 0),
    entry.central + delta + 42);
  output.writeUInt32LE(archive.readUInt32LE(eocd + 16) + delta, eocd + delta + 16);
  output.writeUInt16LE(8, row.local + 8);
  output.writeUInt16LE(8, row.central + delta + 10);
  output.writeUInt32LE(wire.length, row.local + 18);
  output.writeUInt32LE(wire.length, row.central + delta + 20);
  return { archive: output, inventory };
}

function rejects(mutate, pattern) {
  const value = fixture();
  mutate(value);
  assert.throws(() => verifyBrowserMembers(value.archive, json(value.inventory)), pattern);
}

test('tiny stored archive and empty ordinary file match the complete selected inventory', () => {
  const { archive, inventory } = fixture();
  assert.ok(archive.length < 2048);
  assert.deepEqual(JSON.parse(JSON.stringify(verifyBrowserMembers(archive, json(inventory)))), inventory);
  assert.equal(crc32(Buffer.from('123456789')), 0xcbf43926);
});

test('bounded deflate and signed or unsigned data descriptors preserve actual member bytes', () => {
  const compressed = compressedFixture();
  assert.doesNotThrow(() => verifyBrowserMembers(compressed.archive, json(compressed.inventory)));
  for (const signed of [false, true]) {
    const { archive, inventory } = fixture();
    const row = member(archive, 'chrome'), descriptor = Buffer.alloc(signed ? 16 : 12);
    const offset = signed ? 4 : 0;
    if (signed) descriptor.writeUInt32LE(0x08074b50, 0);
    descriptor.writeUInt32LE(archive.readUInt32LE(row.central + 16), offset);
    descriptor.writeUInt32LE(row.dataEnd - row.dataStart, offset + 4);
    descriptor.writeUInt32LE(row.dataEnd - row.dataStart, offset + 8);
    archive.writeUInt16LE(8, row.local + 6);
    archive.writeUInt16LE(8, row.central + 8);
    archive.fill(0, row.local + 14, row.local + 26);
    const output = insert(archive, row.dataEnd, descriptor);
    assert.doesNotThrow(() => verifyBrowserMembers(output, json(inventory)));
  }
});

test('inventory requires exact ordinary-file paths, byte counts and SHA-256 values', () => {
  for (const change of [i => { i[0].sha256 = hash('substitution'); }, i => { i[0].bytes++; },
    i => { i[0].sha256 = 'A'.repeat(64); }, i => { i[0].extra = true; },
    i => { delete i[0].path; }, i => { i.pop(); }, i => { i.shift(); }, i => { i.push({ ...i[0] }); },
    i => { i.reverse(); }, i => { i[0].path = 'foreign/chrome'; }]) rejects(v => change(v.inventory));
});

test('inventory parser rejects malformed duplicate keys, trailing bytes and unsupported carriers', () => {
  const { archive, inventory } = fixture(), valid = json(inventory);
  for (const input of [Buffer.alloc(0), Buffer.from([0xff]), Buffer.from(valid.toString() + '{}'),
    Buffer.from(valid.toString().replace(/"bytes":([0-9]+)/, '"bytes":$1,"bytes":$1')),
    Buffer.from('[[]]'), json([]), Buffer.alloc(8388609, 32)]) {
    assert.throws(() => verifyBrowserMembers(archive, input));
  }
  assert.throws(() => verifyBrowserMembers(archive, valid.toString()));
});

test('archive carriers require a full buffer and terminal single-disk EOCD', () => {
  const { archive, inventory } = fixture();
  for (const input of [new Uint8Array(archive), Buffer.alloc(21), archive.subarray(0, -1),
    Buffer.concat([archive, Buffer.from([0])])]) assert.throws(() => verifyBrowserMembers(input, json(inventory)));
  for (const offset of [4, 6, 20]) rejects(v => { v.archive.writeUInt16LE(1, v.archive.length - 22 + offset); }, /BROWSER-ARCHIVE/);
  rejects(v => { v.archive.writeUInt32LE(0, v.archive.length - 22); }, /BROWSER-ARCHIVE/);
});

test('central directory count, extent and member expansion bounds are closed', () => {
  for (const count of [0, 1025]) rejects(v => {
    const eocd = v.archive.length - 22;
    v.archive.writeUInt16LE(count, eocd + 8); v.archive.writeUInt16LE(count, eocd + 10);
  }, /BROWSER-ARCHIVE/);
  rejects(v => { v.archive.writeUInt16LE(1, v.archive.length - 22 + 8); }, /BROWSER-ARCHIVE/);
  for (const offset of [12, 16]) rejects(v => {
    const at = v.archive.length - 22 + offset;
    v.archive.writeUInt32LE(v.archive.readUInt32LE(at) + 1, at);
  }, /BROWSER-ARCHIVE/);
  rejects(v => { v.archive.writeUInt32LE(536870913, member(v.archive, 'chrome').central + 24); }, /BROWSER-ARCHIVE/);
  rejects(v => { v.inventory[0].bytes = 536870913; }, /BROWSER-ARCHIVE/);
  rejects(v => { v.inventory.forEach(file => { file.bytes = 536870912; }); }, /BROWSER-ARCHIVE/);
  rejects(v => { v.inventory = Array(513).fill(v.inventory[0]); });
});

test('duplicate names and case-folded file or directory aliases cannot replace members', () => {
  for (const path of ['chrome', 'Chrome', 'folder']) rejects(v => {
    rename(v.archive, member(v.archive, 'data00'), `${root}/${path}`);
  }, /BROWSER-ARCHIVE/);
  rejects(v => { rename(v.archive, member(v.archive, 'folder/a.txt'), `${root}/Folder/a.txt`); });
});

test('explicit directory spellings cannot case-alias a verified ordinary-file ancestor', () => {
  rejects(v => { rename(v.archive, member(v.archive, 'folder/'), `${root}/Folder/`); });
});

test('foreign namespace, reserved paths and non-ASCII archive names reject', () => {
  for (const name of ['foreign-linux6/chrome', `${root}/../xxx`, `${root}/NUL.xx`, `${root}/chr\\me`]) {
    rejects(v => { rename(v.archive, member(v.archive, 'chrome'), name); });
  }
  rejects(v => {
    const row = member(v.archive, 'chrome');
    v.archive[row.central + 46] = 0xff; v.archive[row.local + 30] = 0xff;
  }, /BROWSER-ARCHIVE/);
});

test('file content substitution must satisfy CRC and independently selected member SHA', () => {
  rejects(v => { v.archive[member(v.archive, 'chrome').dataStart] ^= 1; }, /BROWSER-ARCHIVE/);
  rejects(v => {
    const row = member(v.archive, 'chrome');
    v.archive[row.dataStart] ^= 1;
    const checksum = crc32(v.archive.subarray(row.dataStart, row.dataEnd));
    v.archive.writeUInt32LE(checksum, row.local + 14); v.archive.writeUInt32LE(checksum, row.central + 16);
  }, /BROWSER-ARCHIVE/);
});

test('local and central checksum, method, flags, size and name must agree', () => {
  for (const [offset, width] of [[6, 2], [8, 2], [14, 4], [18, 4], [22, 4], [26, 2]]) rejects(v => {
    const at = member(v.archive, 'chrome').local + offset;
    if (width === 2) v.archive.writeUInt16LE(v.archive.readUInt16LE(at) + 1, at);
    else v.archive.writeUInt32LE(v.archive.readUInt32LE(at) + 1, at);
  }, /BROWSER-ARCHIVE/);
  rejects(v => { v.archive[member(v.archive, 'chrome').local + 30] ^= 1; }, /BROWSER-ARCHIVE/);
  rejects(v => { v.archive.writeUInt32LE(0, member(v.archive, 'chrome').local); }, /BROWSER-ARCHIVE/);
});

test('encrypted, unsupported, multi-disk and linked archive members reject', () => {
  for (const [offset, value] of [[8, 1], [10, 99], [34, 1], [28, 0], [28, 201]]) rejects(v => {
    v.archive.writeUInt16LE(value, member(v.archive, 'chrome').central + offset);
  }, /BROWSER-ARCHIVE/);
  for (const mode of [0o120777, 0o106755, 0o020644]) rejects(v => {
    v.archive.writeUInt32LE((mode << 16) >>> 0, member(v.archive, 'chrome').central + 38);
  }, /BROWSER-ARCHIVE/);
  rejects(v => { const row = member(v.archive, 'chrome');
    v.archive.writeUInt32LE((v.archive.readUInt32LE(row.central + 38) | 16) >>> 0, row.central + 38);
  }, /BROWSER-ARCHIVE/);
});

test('local-record spans cover the whole archive prefix without unclaimed bytes', () => {
  for (const position of [0, -1]) {
    const { archive, inventory } = fixture();
    const at = position === 0 ? 0 : archive.readUInt32LE(archive.length - 22 + 16);
    assert.throws(() => verifyBrowserMembers(insert(archive, at, Buffer.from([0])), json(inventory)), /BROWSER-ARCHIVE/);
  }
});

test('a valid nested local header cannot let two central members overlap data spans', () => {
  const original = fixture(), inner = member(original.archive, 'folder/a.txt');
  const nested = original.archive.subarray(inner.local, inner.dataEnd);
  const { archive, inventory } = fixture({ data00: nested });
  const outer = member(archive, 'data00'), child = member(archive, 'folder/a.txt');
  archive.writeUInt32LE(outer.dataStart, child.central + 42);
  assert.throws(() => verifyBrowserMembers(archive, json(inventory)), /BROWSER-ARCHIVE/);
});

test('central offsets and local extra lengths cannot reach outside the member prefix', () => {
  for (const offset of [0xffffffff, 1]) rejects(v => {
    v.archive.writeUInt32LE(offset, member(v.archive, 'chrome').central + 42);
  }, /BROWSER-ARCHIVE/);
  rejects(v => { v.archive.writeUInt16LE(65535, member(v.archive, 'chrome').local + 28); }, /BROWSER-ARCHIVE/);
  rejects(v => { v.archive.writeUInt16LE(65535, member(v.archive, 'chrome').central + 30); }, /BROWSER-ARCHIVE/);
});

test('compressed member trailing bytes and expansion beyond declared size reject', () => {
  const trailing = compressedFixture(true);
  assert.throws(() => verifyBrowserMembers(trailing.archive, json(trailing.inventory)), /BROWSER-ARCHIVE/);
  const bounded = compressedFixture(), row = member(bounded.archive, 'chrome');
  bounded.archive.writeUInt32LE(1, row.local + 22);
  bounded.archive.writeUInt32LE(1, row.central + 24);
  assert.throws(() => verifyBrowserMembers(bounded.archive, json(bounded.inventory)), /BROWSER-ARCHIVE/);
});

test('flagged descriptor records require exact checksums and lengths rather than following headers', () => {
  rejects(v => { const row = member(v.archive, 'chrome');
    v.archive.writeUInt16LE(8, row.local + 6); v.archive.writeUInt16LE(8, row.central + 8);
  }, /BROWSER-ARCHIVE/);
});
