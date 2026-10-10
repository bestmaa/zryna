import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { resolve } from 'node:path';
import { readFileSync } from 'node:fs';
import test from 'node:test';
import { gzipSync } from 'node:zlib';
import { captureTarGzipMembers } from '../scripts/distribution/npm-materials.mjs';
import { nodeExpandedArchive, nodeMaterialMembers }
  from '../scripts/distribution/node-materials.mjs';
import {
  checkQualificationRustClosure, rustMaterials, validateRustMaterials,
} from '../scripts/distribution/rust-materials.mjs';

const BLOCK = 512;

function sha256(input) {
  return createHash('sha256').update(input).digest('hex');
}

function octal(header, offset, width, value) {
  header.write(`${value.toString(8).padStart(width - 1, '0')}\0`, offset, width, 'ascii');
}

function header(path, mode, size, type = 48, link = '') {
  const value = Buffer.alloc(BLOCK);
  value.write(path, 0, 100, 'ascii');
  octal(value, 100, 8, mode);
  octal(value, 108, 8, 0);
  octal(value, 116, 8, 0);
  octal(value, 124, 12, size);
  octal(value, 136, 12, 1);
  value.fill(32, 148, 156);
  value[156] = type;
  value.write(link, 157, 100, 'ascii');
  value.write('ustar ', 257, 6, 'ascii');
  value.write('00', 263, 2, 'ascii');
  const checksum = value.reduce((sum, byte) => sum + byte, 0);
  value.write(`${checksum.toString(8).padStart(6, '0')}\0 `, 148, 8, 'ascii');
  return value;
}

function rewriteChecksum(value) {
  value.fill(32, 148, 156);
  const checksum = value.reduce((sum, byte) => sum + byte, 0);
  value.write(`${checksum.toString(8).padStart(6, '0')}\0 `, 148, 8, 'ascii');
}

function entry(path, data, type = 48, link = '') {
  const padding = (BLOCK - data.length % BLOCK) % BLOCK;
  return [header(path, 0o644, data.length, type, link), data, Buffer.alloc(padding)];
}

function namedGzip(input) {
  const gzip = gzipSync(input, { level: 9 });
  const prefix = Buffer.from(gzip.subarray(0, 10));
  prefix[3] |= 8;
  return Buffer.concat([prefix, Buffer.from('fixture.crate\0'), gzip.subarray(10)]);
}

function fixture({ corruptLinkField = false, corruptPadding = false, type = 48,
  trailing = false } = {}) {
  const root = 'example-1.0.0';
  const sourcePath = `licenses/${'long-'.repeat(18)}LICENSE`;
  const archivePath = `${root}/${sourcePath}`;
  const data = Buffer.from('license\n');
  const longName = Buffer.from(`${archivePath}\0`);
  const longHeader = header('././@LongLink', 0o644, longName.length, 76);
  if (corruptLinkField) {
    longHeader[158] = 65;
    rewriteChecksum(longHeader);
  }
  const raw = Buffer.concat([
    longHeader, longName, Buffer.alloc((BLOCK - longName.length % BLOCK) % BLOCK),
    ...entry('placeholder', data, type, type === 49 || type === 50 ? `${root}/target` : ''),
    ...entry(`${root}/EMPTY`, Buffer.alloc(0)),
    Buffer.alloc(2 * BLOCK),
  ]);
  if (corruptPadding) raw[raw.indexOf(data) + data.length] = 1;
  let archive = namedGzip(raw);
  if (trailing) archive = Buffer.concat([archive, Buffer.from([0])]);
  return { archive, descriptor: {
    root, size: archive.length, sha256: sha256(archive), files: [{ sourcePath,
      path: 'licenses/example-LICENSE', mode: 0o644, size: data.length,
      sha256: sha256(data) }],
  }, data };
}

test('captures an ordinary crate member through optional gzip and GNU long-name framing', () => {
  const value = fixture();
  assert.deepEqual(captureTarGzipMembers(value.archive, value.descriptor), [{
    path: 'licenses/example-LICENSE', mode: 0o644, data: value.data,
  }]);
});

test('rejects linked crate members even when their bytes match', () => {
  for (const type of [49, 50]) {
    const value = fixture({ type });
    assert.throws(() => captureTarGzipMembers(value.archive, value.descriptor),
      /npm tar ordinary file/);
  }
});

test('rejects trailing bytes after a complete crate gzip stream', () => {
  const value = fixture({ trailing: true });
  assert.throws(() => captureTarGzipMembers(value.archive, value.descriptor), /npm gzip/);
});

test('rejects nonzero crate tar padding', () => {
  const value = fixture({ corruptPadding: true });
  assert.throws(() => captureTarGzipMembers(value.archive, value.descriptor),
    /npm tar padding/);
});

test('rejects hidden bytes after the GNU long-name linkfield terminator', () => {
  const value = fixture({ corruptLinkField: true });
  assert.throws(() => captureTarGzipMembers(value.archive, value.descriptor),
    /npm tar long name/);
});

test('publishes exact Node member and expanded Linux tar requirements', () => {
  assert.deepEqual(nodeExpandedArchive('x86_64-unknown-linux-gnu'), {
    size: 203161600,
    sha256: 'd9bd21210a6aaf6d02d35f739d02bb05ecda8c63deb1abb0150c288c3263fb48',
  });
  assert.equal(nodeExpandedArchive('x86_64-pc-windows-msvc'), null);
  for (const target of ['x86_64-unknown-linux-gnu', 'x86_64-pc-windows-msvc']) {
    const members = nodeMaterialMembers(target);
    assert.equal(members.length, 2);
    assert.ok(members.every(member => member.type === 'ordinary'));
  }
});

test('pins the complete target-specific Rust capture sets and Wasmtime license fanout', () => {
  const linux = rustMaterials('x86_64-unknown-linux-gnu');
  const windows = rustMaterials('x86_64-pc-windows-msvc');
  assert.equal(linux.length, 161);
  assert.equal(windows.length, 168);
  const upstream = 'https://raw.githubusercontent.com/bytecodealliance/wasmtime/'
    + '7bac2c2775808aaec5d4aa5627a5e447b51102cf/LICENSE';
  const destinations = new Set([...linux, ...windows].flatMap(record => record.files)
    .filter(file => file.origin === upstream).map(file => file.path));
  assert.equal(destinations.size, 14);
});

test('binds Rust material notices to the exact current and predecessor Cargo locks', () => {
  const target = 'x86_64-unknown-linux-gnu';
  const entries = rustMaterials(target).flatMap(record => record.files)
    .map(({ path, size, sha256: digest }) => ({ path, size, sha256: digest }));
  const receipt = digest => ({ inputs: [{ logicalPath: 'Cargo.lock', sha256: digest }] });
  assert.doesNotThrow(() => validateRustMaterials(entries, target,
    receipt('78733d507b74bd9a343c938285ede13c3a607cae36cdad6d116f6c2b572e14f1')));
  assert.doesNotThrow(() => validateRustMaterials(entries, target,
    receipt('fec1a746a6122a216255080e90ee39068578528feddec24c1e0d8bf9df3b8a38'), '0.2.1'));
  assert.doesNotThrow(() => validateRustMaterials(entries, target,
    receipt('baf9267bada161b9e2ddd6abddac4b0f029b5bb23177c11599c341a68b2130f9'), '0.2.2'));
  assert.throws(() => validateRustMaterials(entries, target,
    receipt('fec1a746a6122a216255080e90ee39068578528feddec24c1e0d8bf9df3b8a38')),
  /Rust material lockfile identity/);
  assert.throws(() => validateRustMaterials(entries, target,
    receipt('fec1a746a6122a216255080e90ee39068578528feddec24c1e0d8bf9df3b8a38'), '0.2.0'),
  /Rust material lockfile identity/);
});

test('qualification closure audit rejects an omitted normal registry dependency without network', () => {
  const cargo = resolve('pinned-cargo');
  const metadata = JSON.stringify({
    packages: [
      { id: 'zryna', name: 'zryna', source: null,
        targets: [{ name: 'zryna', kind: ['bin'] }] },
      { id: 'required', name: 'required-crate', version: '1.2.3',
        source: 'registry+https://github.com/rust-lang/crates.io-index', targets: [] },
    ],
    workspace_members: ['zryna'],
    resolve: { nodes: [
      { id: 'zryna', deps: [{ pkg: 'required', dep_kinds: [{ kind: null, target: null }] }] },
      { id: 'required', deps: [] },
    ] },
  });
  const spawn = (_executable, args) => ({ error: null, status: 0, signal: null,
    stdout: args[0] === 'which' ? `${cargo}\n`
      : args[0] === '--version' ? 'cargo 1.97.1 (c980f4866 2026-06-30)\n' : metadata,
    stderr: '' });
  assert.throws(() => checkQualificationRustClosure({ spawn, materialRecords: () => [] }),
    /material identities/);
});

function commandCandidate(target = 'x86_64-unknown-linux-gnu') {
  const binding = JSON.parse(readFileSync(new URL(
    '../scripts/distribution/command-h1-rust-binding-v1.json', import.meta.url)));
  const source = { repository: 'https://github.com/zryna/zryna',
    commit: '1'.repeat(40), tree: '2'.repeat(40) };
  return { entries: rustMaterials(target).flatMap(record => record.files)
    .map(({ path, size, sha256: digest }) => ({ path, size, sha256: digest })),
  target, receipt: { source, inputs: binding.inputs },
  distribution: { version: '0.2.3', source: { ...source, ref: 'refs/heads/main' },
    target: { triple: target }, recipe: { format: 'zryna.distribution-recipe.v1',
      sha256: '0f522abd343ba4e3c07a38d93c778fa7a0e30aac2295abe1f075091f6b6d7db1' } } };
}

function validateCommandCandidate(value, options = {}) {
  return validateRustMaterials(value.entries, value.target, value.receipt, '0.2.3',
    { productionCandidate: true, distribution: value.distribution, ...options });
}

test('exact H1 source binding admits both candidate targets with unchanged material notices', () => {
  for (const target of ['x86_64-unknown-linux-gnu', 'x86_64-pc-windows-msvc']) {
    assert.doesNotThrow(() => validateCommandCandidate(commandCandidate(target)));
  }
});

test('H1 source binding cannot replace tagged or predecessor release lock pins', () => {
  assert.throws(() => validateCommandCandidate(commandCandidate(), { productionCandidate: false }),
    /Rust material lockfile identity/);
  assert.throws(() => validateCommandCandidate(commandCandidate(), { productionCandidate: 'true' }),
    /Rust material lockfile identity/);
  for (const version of ['0.2.1', '0.2.2', '0.2.3']) {
    const value = commandCandidate();
    value.distribution.source.ref = `refs/tags/v${version}`;
    assert.throws(() => validateCommandCandidate(value), /Rust material lockfile identity/);
    assert.throws(() => validateRustMaterials(value.entries, value.target, value.receipt, version,
      { productionCandidate: true, distribution: value.distribution }), /Rust material lockfile identity/);
  }
});

test('candidate material binding rejects source, recipe, version and target substitutions', () => {
  for (const mutate of [
    value => { value.distribution.source.repository = 'https://example.invalid/repository'; },
    value => { value.distribution.source.ref = 'refs/heads/other'; },
    value => { value.distribution.source.commit = '3'.repeat(40); },
    value => { value.distribution.source.tree = '3'.repeat(40); },
    value => { value.distribution.recipe.sha256 = '0'.repeat(64); },
    value => { value.distribution.recipe.format = 'unreviewed'; },
    value => { value.distribution.version = '0.2.4'; },
    value => { value.distribution.target.triple = 'x86_64-pc-windows-msvc'; },
  ]) {
    const value = commandCandidate(); mutate(value);
    assert.throws(() => validateCommandCandidate(value), /Rust material lockfile identity/);
  }
});

test('candidate binding rejects drift in every exact source input and inventory shape', () => {
  for (let index = 0; index < 4; index++) {
    for (const field of ['size', 'sha256', 'logicalPath']) {
      const value = commandCandidate();
      value.receipt.inputs[index][field] = field === 'size' ? 1 : field === 'sha256'
        ? '0'.repeat(64) : 'other';
      assert.throws(() => validateCommandCandidate(value), /Rust material lockfile identity/);
    }
  }
  for (const mutate of [
    inputs => inputs.pop(), inputs => inputs.push({ ...inputs[0] }), inputs => inputs.reverse(),
  ]) {
    const value = commandCandidate(); mutate(value.receipt.inputs);
    assert.throws(() => validateCommandCandidate(value), /Rust material lockfile identity/);
  }
});

test('candidate binding retains exact notice hashes and full material coverage', () => {
  for (const mutate of [
    entries => entries.pop(), entries => { entries[0].sha256 = '0'.repeat(64); },
    entries => entries.push({ ...entries[0] }),
  ]) {
    const value = commandCandidate(); mutate(value.entries);
    assert.throws(() => validateCommandCandidate(value), /Rust notice inventory/);
  }
});
