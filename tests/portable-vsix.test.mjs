import { test } from 'node:test';
import assert from 'node:assert/strict';
import { deflateRawSync } from 'node:zlib';
import { canonicalVsix, vsixEntries } from '../scripts/portable-setup/vsix.mjs';
import { verifyEditorManifest } from '../scripts/portable-setup/build.mjs';
import { crc32, encodeZip } from '../scripts/distribution/archive-zip.mjs';

function fixture() {
  return encodeZip('extension', [{ path: 'package.json', mode: 0o644,
    data: Buffer.from('{"name":"zryna","version":"0.4.0"}') }]);
}

function compressedFixture() {
  const local = [];
  const central = [];
  let offset = 0;
  for (const [name, value, deflated] of [
    ['extension/package.json', 'package bytes', true],
    ['extension/syntaxes/zryna.tmLanguage.json', 'syntax bytes', false],
  ]) {
    const path = Buffer.from(name);
    const data = Buffer.from(value);
    const payload = deflated ? deflateRawSync(data) : data;
    const checksum = crc32(data);
    const flags = deflated ? 0x0808 : 0x0800;
    const header = Buffer.alloc(30 + path.length);
    header.writeUInt32LE(0x04034b50, 0);
    header.writeUInt16LE(20, 4);
    header.writeUInt16LE(flags, 6);
    header.writeUInt16LE(deflated ? 8 : 0, 8);
    header.writeUInt16LE(33, 12);
    if (!deflated) {
      header.writeUInt32LE(checksum, 14);
      header.writeUInt32LE(data.length, 18);
      header.writeUInt32LE(data.length, 22);
    }
    header.writeUInt16LE(path.length, 26);
    path.copy(header, 30);
    const descriptor = Buffer.alloc(deflated ? 16 : 0);
    if (deflated) {
      descriptor.writeUInt32LE(0x08074b50, 0);
      descriptor.writeUInt32LE(checksum, 4);
      descriptor.writeUInt32LE(payload.length, 8);
      descriptor.writeUInt32LE(data.length, 12);
    }
    const record = Buffer.concat([header, payload, descriptor]);
    const directory = Buffer.alloc(46 + path.length);
    directory.writeUInt32LE(0x02014b50, 0);
    directory.writeUInt16LE(0x0314, 4);
    directory.writeUInt16LE(20, 6);
    directory.writeUInt16LE(flags, 8);
    directory.writeUInt16LE(deflated ? 8 : 0, 10);
    directory.writeUInt16LE(33, 14);
    directory.writeUInt32LE(checksum, 16);
    directory.writeUInt32LE(payload.length, 20);
    directory.writeUInt32LE(data.length, 24);
    directory.writeUInt16LE(path.length, 28);
    directory.writeUInt32LE(offset, 42);
    path.copy(directory, 46);
    local.push(record);
    central.push(directory);
    offset += record.length;
  }
  const end = Buffer.alloc(22);
  end.writeUInt32LE(0x06054b50, 0);
  end.writeUInt16LE(central.length, 8);
  end.writeUInt16LE(central.length, 10);
  end.writeUInt32LE(central.reduce((length, record) => length + record.length, 0), 12);
  end.writeUInt32LE(offset, 16);
  return Buffer.concat([...local, ...central, end]);
}

function reorderedFixture() {
  const input = compressedFixture();
  const entries = vsixEntries(input);
  const ordered = [...entries.slice(0, -2), entries.at(-1), entries.at(-2)];
  const directoryStart = input.readUInt32LE(input.length - 6);
  const output = Buffer.alloc(input.length);
  const offsets = new Map();
  let cursor = 0;
  for (const entry of ordered) {
    const index = entries.indexOf(entry);
    const end = entries[index + 1]?.local ?? directoryStart;
    input.copy(output, cursor, entry.local, end);
    offsets.set(entry.name, cursor);
    cursor += end - entry.local;
  }
  for (const entry of ordered) {
    const index = entries.indexOf(entry);
    const end = entries[index + 1]?.central ?? input.length - 22;
    input.copy(output, cursor, entry.central, end);
    output.writeUInt32LE(offsets.get(entry.name), cursor + 42);
    cursor += end - entry.central;
  }
  input.copy(output, cursor, input.length - 22);
  return [input, output];
}

test('VSIX canonicalization retains every content byte and normalizes both timestamp fields', () => {
  const input = fixture();
  for (const entry of vsixEntries(input)) {
    input.writeUInt16LE(123, entry.local + 10);
    input.writeUInt16LE(456, entry.central + 12);
  }
  const actual = canonicalVsix(input);
  assert.deepEqual(vsixEntries(actual).map(entry => [entry.name, entry.data]),
    vsixEntries(input).map(entry => [entry.name, entry.data]));
  assert.deepEqual(canonicalVsix(actual), actual);
  for (const entry of vsixEntries(actual)) {
    assert.equal(actual.readUInt16LE(entry.local + 10), 0);
    assert.equal(actual.readUInt16LE(entry.central + 12), 0);
  }
});

test('VSIX member order does not change canonical bytes or extracted contents', () => {
  const [first, second] = reorderedFixture();
  assert.notDeepEqual(first, second);
  const contents = bytes => Object.fromEntries(vsixEntries(bytes).map(entry => [entry.name, entry.data]));
  const compressed = bytes => Object.fromEntries(vsixEntries(bytes).map(entry => {
    const start = entry.local + 30 + bytes.readUInt16LE(entry.local + 26)
      + bytes.readUInt16LE(entry.local + 28);
    return [entry.name, bytes.subarray(start, start + bytes.readUInt32LE(entry.central + 20))];
  }));
  assert.deepEqual(contents(first), contents(second));
  assert.deepEqual(compressed(first), compressed(second));
  const canonical = canonicalVsix(first);
  assert.deepEqual(canonicalVsix(second), canonical);
  assert.deepEqual(canonicalVsix(canonical), canonical);
  assert.deepEqual(contents(canonical), contents(first));
  assert.deepEqual(compressed(canonical), compressed(first));
  assert.deepEqual(vsixEntries(canonical).map(entry => entry.name),
    Object.keys(contents(first)).sort());
});

test('VSIX parser rejects missing end, foreign method, encryption and excessive expansion', () => {
  assert.throws(() => vsixEntries(Buffer.alloc(4)));
  assert.throws(() => vsixEntries(fixture().subarray(0, -1)));
  for (const mutate of [
    (bytes, entry) => bytes.writeUInt16LE(99, entry.central + 10),
    (bytes, entry) => bytes.writeUInt16LE(1, entry.central + 8),
    (bytes, entry) => bytes.writeUInt32LE(2 * 1024 * 1024, entry.central + 24),
    (bytes, entry) => bytes.writeUInt32LE(0xffffffff, entry.central + 42),
  ]) {
    const bytes = fixture();
    mutate(bytes, vsixEntries(bytes)[1]);
    assert.throws(() => vsixEntries(bytes));
  }
});

test('candidate assembly rejects a VSIX with mismatched server, editor or profile metadata', () => {
  const metadata = {
    name: 'zryna', version: '0.4.0', zrynaCompatibility: {
      compilerVersion: '0.2.3', serverVersion: '0.4.0', installationCapability: 'portable-setup-v1',
      requiredCapabilities: { 'scalar-v2': 'scalar-format-v1', 'control-flow-v1': 'control-flow-format-v1' },
      profiles: ['scalar-v2', 'control-flow-v1'], sourceBuildRequired: false,
      releasedCompilerCompatible: true,
    },
  };
  const entries = [{ name: 'extension/package.json', data: Buffer.from(JSON.stringify(metadata)) }];
  assert.doesNotThrow(() => verifyEditorManifest(entries));
  for (const [field, value] of [['version', '0.3.0'], ['version', '0.5.0']]) {
    const original = metadata[field];
    metadata[field] = value;
    entries[0].data = Buffer.from(JSON.stringify(metadata));
    assert.throws(() => verifyEditorManifest(entries), /compatibility/);
    metadata[field] = original;
  }
  for (const [field, value] of [['compilerVersion', '0.3.0'], ['serverVersion', '0.3.0'],
    ['installationCapability', 'portable-setup-v2']]) {
    const original = metadata.zrynaCompatibility[field];
    metadata.zrynaCompatibility[field] = value;
    entries[0].data = Buffer.from(JSON.stringify(metadata));
    assert.throws(() => verifyEditorManifest(entries), /compatibility/);
    metadata.zrynaCompatibility[field] = original;
  }
  metadata.zrynaCompatibility.requiredCapabilities['control-flow-v1'] = 'scalar-format-v1';
  entries[0].data = Buffer.from(JSON.stringify(metadata));
  assert.throws(() => verifyEditorManifest(entries), /compatibility/);
});
