import test from 'node:test';
import assert from 'node:assert/strict';
import { LIMITS, bytes, parseCanonical } from '../scripts/distribution/canonical.mjs';
import { LEGACY_PROVIDERS, PROVIDERS, providerPaths, validateTuples } from '../scripts/distribution/inventory.mjs';
import { validateDistribution } from '../scripts/distribution/prepare.mjs';
import { rustMaterials } from '../scripts/distribution/rust-materials.mjs';
import { verifyCompiledIdentity } from '../scripts/distribution/binary-identity.mjs';

const target = 'x86_64-unknown-linux-gnu';
const digest = 'a'.repeat(64);
const tuple = (path, role, size, mode = 0o644) => ({ path, role, size, mode,
  sha256: digest, material: 'source', licenses: ['LICENSE'] });
const license = tuple('LICENSE', 'license', 1);

test('native size and total expansion ceilings are inclusive without allocating native buffers', () => {
  const files = [license, tuple('bin/zryna', 'cli', LIMITS.binary, 0o755),
    tuple('runtime/node/bin/node', 'runtime', LIMITS.binary - 1, 0o755)];
  validateTuples(files, target);
  const tooLargeTotal = structuredClone(files);
  tooLargeTotal[2].size++;
  assert.throws(() => validateTuples(tooLargeTotal, target), /expansion budget/);
  const tooLargeBinary = structuredClone(files);
  tooLargeBinary[1].size++;
  assert.throws(() => validateTuples(tooLargeBinary, target), /role byte budget/);
});

test('provider/text role boundaries and first extra file reject before body allocation', () => {
  for (const [path, role, limit] of [['lib/zryna/bootstrap/worker.mjs', 'provider', LIMITS.provider],
    ['NOTICE', 'notice', LIMITS.text]]) {
    const files = [license, tuple(path, role, limit)];
    validateTuples(files, target);
    files[1].size++;
    assert.throws(() => validateTuples(files, target), /role byte budget/);
  }
  assert.throws(() => validateTuples(Array(513).fill(license), target), /file count budget/);
});

test('inventory rejects hidden paths, executable notices and dangling license references', () => {
  for (const file of [tuple('npm', 'notice', 1), tuple('NOTICE', 'notice', 1, 0o755),
    { ...tuple('NOTICE', 'notice', 1), licenses: ['licenses/missing'] }]) {
    assert.throws(() => validateTuples([license, file], target));
  }
});

test('canonical record byte ceiling rejects the first extra byte', () => {
  const valid = bytes({ a: 'x'.repeat(LIMITS.record - 9) });
  assert.equal(valid.length, LIMITS.record);
  parseCanonical(valid);
  assert.throws(() => parseCanonical(Buffer.concat([valid, Buffer.from(' ')])), /byte budget/);
});

test('reviewed Rust closure preserves exact target package and license counts', () => {
  for (const [triple, packages, files] of [[target, 161, 324], ['x86_64-pc-windows-msvc', 168, 331]]) {
    const selected = rustMaterials(triple);
    assert.equal(selected.length, packages);
    assert.equal(selected.flatMap(record => record.files).length, files);
    const notices = selected.flatMap(record => record.files).map(file => ({
      path: file.path, size: file.size, sha256: file.sha256, role: 'license', mode: 0o644,
      material: 'source', licenses: [file.path],
    })).sort((left, right) => left.path < right.path ? -1 : left.path > right.path ? 1 : 0);
    validateTuples(notices, triple);
    assert(selected.every(record => !record.name.startsWith('zryna')));
  }
});

test('compiled identity requires target machine and a unique complete build marker', () => {
  const header = Buffer.alloc(64);
  Buffer.from([127, 69, 76, 70, 2, 1, 1]).copy(header);
  header.writeUInt16LE(3, 16);
  header.writeUInt16LE(62, 18);
  const marker = Buffer.from(`ZRYNA-DISTRIBUTION-V1\0${digest}\0`);
  verifyCompiledIdentity(Buffer.concat([header, marker]), digest, target);
  assert.throws(() => verifyCompiledIdentity(Buffer.concat([header, marker, marker]), digest, target));
  assert.throws(() => verifyCompiledIdentity(Buffer.concat([header, marker]), 'b'.repeat(64), target));
  header.writeUInt16LE(183, 18);
  assert.throws(() => verifyCompiledIdentity(Buffer.concat([header, marker]), digest, target));
});

function completeInventory(providers) {
  const text = ['NOTICE', 'README.md', 'SUPPORT.md', 'VERSION'];
  const licenses = ['LICENSE', 'licenses/node-LICENSE', 'licenses/typescript-LICENSE.txt',
    'licenses/typescript-ThirdPartyNoticeText.txt', 'licenses/typescript6-LICENSE.txt',
    'licenses/rust/example-1.0.0/LICENSE'];
  return [
    ...providers.map(path => tuple(path, 'provider', 1)),
    ...text.map(path => tuple(path, 'notice', 1)),
    ...licenses.map(path => ({ ...tuple(path, 'license', 1), licenses: [path] })),
    tuple('runtime/node/bin/node', 'runtime', 1, 0o755),
    tuple('metadata/materials.json', 'metadata', 1),
    tuple('metadata/architecture-receipt.json', 'metadata', 1),
  ].sort((left, right) => left.path < right.path ? -1 : left.path > right.path ? 1 : 0);
}

test('tagged nine-file inventories remain exact while main requires every modular file', () => {
  const legacy = completeInventory(LEGACY_PROVIDERS);
  const main = completeInventory(PROVIDERS);
  assert.equal(LEGACY_PROVIDERS.length, 9);
  assert.equal(PROVIDERS.length, 28);
  validateTuples(legacy, target, { complete: true });
  for (const version of ['0.2.1', '0.2.2', '0.2.3']) {
    const sourceRef = `refs/tags/v${version}`;
    validateTuples(legacy, target, { complete: true, sourceRef });
    assert.throws(() => validateTuples(main, target, { complete: true, sourceRef }), /no distribution role/);
  }
  validateTuples(main, target, { complete: true, sourceRef: 'refs/heads/main' });
  assert.throws(() => validateTuples(main, target, { complete: true }), /no distribution role/);
  for (const path of PROVIDERS.filter(path => !LEGACY_PROVIDERS.includes(path))) {
    const missing = main.filter(file => file.path !== path);
    assert.throws(() => validateTuples(missing, target,
      { complete: true, sourceRef: 'refs/heads/main' }), /missing required payload/);
  }
  for (const ref of ['refs/heads/feature', 'refs/tags/v0.2.4', '', null]) {
    assert.throws(() => providerPaths(ref), /source identity/);
  }
});

test('distribution source identity is validated before selecting legacy or modular closure', () => {
  for (const [ref, productionCandidate] of [
    ['refs/heads/feature', true], ['refs/tags/v0.2.2', false],
    ['refs/tags/v0.2.3', true], ['refs/heads/main', false],
  ]) {
    const record = { format: 'zryna.distribution.v1', version: '0.2.3',
      source: { repository: 'https://github.com/zryna/zryna', ref,
        commit: digest.slice(0, 40), tree: digest.slice(0, 40), sourceDateEpoch: 0 },
      target: {}, recipe: {}, files: [],
    };
    assert.throws(() => validateDistribution(record, { productionCandidate }), /source identity/);
  }
});
