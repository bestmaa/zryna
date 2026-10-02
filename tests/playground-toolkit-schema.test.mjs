import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { test } from 'node:test';
import { validateToolkitEnvelope } from '../examples/playground/restricted/toolkit-schema.mjs';

// Synthetic publication metadata checks schema/policy only, never signatures or executable admission.
const digest = value => createHash('sha256').update(value).digest('hex');
const sourceCommit = 'a'.repeat(40);
const sourceTree = 'b'.repeat(40);
const archiveSha256 = 'c'.repeat(64);
const nestedCompiler = { version: '0.2.3', envelopeSha256: 'd'.repeat(64),
  archiveSha256: 'e'.repeat(64), sourceCommit: 'f'.repeat(40) };
const browser = { version: '153.0.8010.12',
  archiveSha256: '8aac35011c18f6e2d10696154af89a5728ac2ddd6dc6fad24ffdf243c3fcfd5a',
  inventorySha256: '110d5111c1a885596b2dba4fe5d04e570d26e1c57454d0a902f1115ce818635a' };
const captured = {
  'releases/compiler-envelope.json': [nestedCompiler.envelopeSha256, 262144],
  'releases/compiler-envelope.sigstore.json': [digest('synthetic:compiler-bundle'), 1048576],
  'releases/zryna-0.2.3-x86_64-unknown-linux-gnu.tar.gz': [nestedCompiler.archiveSha256, 268435456],
  'releases/chrome-linux64.zip': [browser.archiveSha256, 268435456],
  'releases/chrome-linux64.inventory.json': [browser.inventorySha256, 8388608],
};
const requiredGates = ['linux-contract', 'windows-contract'];
const root = 'zryna-playground-0.1.0-x86_64-unknown-linux-gnu';

function sorted(value) {
  if (Array.isArray(value)) return value.map(sorted);
  if (value && typeof value === 'object') return Object.fromEntries(Object.keys(value).sort()
    .map(key => [key, sorted(value[key])]));
  return value;
}
const encode = value => Buffer.from(JSON.stringify(sorted(value)) + '\n');
const file = (path, mode = 0o644) => ({ path, mode, bytes: 17, sha256: digest(`synthetic:${path}`) });

function fixture() {
  const files = [file('bin/zryna-playground-compiler', 0o755), file('policy.json'),
    file('resources/loader.js'), file('resources/browser.wit'), file('resources/wit-sources.json'),
    file('runtime/node/bin/node', 0o755),
    ...Object.entries(captured).map(([path, [sha256]]) => ({ ...file(path), sha256 })),
    ...requiredGates.map(name => file(`evidence/${name}.json`))].sort((a, b) => a.path < b.path ? -1 : 1);
  const document = { format: 'zryna.playground-toolkit.v1', version: 1, productVersion: '0.1.0',
    target: 'x86_64-unknown-linux-gnu',
    source: { repository: 'https://github.com/zryna/zryna', ref: 'refs/tags/playground-v0.1.0',
      commit: sourceCommit, tree: sourceTree, sourceDateEpoch: 1_700_000_000 },
    signing: { issuer: 'https://token.actions.githubusercontent.com', certificateIdentity:
      'https://github.com/zryna/zryna/.github/workflows/playground-release.yml@refs/tags/playground-v0.1.0' },
    archive: { path: root + '.tar.gz', bytes: 1024, sha256: archiveSha256 },
    nestedCompiler: structuredClone(nestedCompiler),
    browser: structuredClone(browser),
    files, materials: ['bin/zryna-playground-compiler', 'runtime/node/bin/node'].map((path, index) => {
      const descriptor = files.find(value => value.path === path);
      return { path, mount: index === 0 ? '/app/compiler' : '/materials/runtime/node/bin/node',
        bytes: descriptor.bytes, sha256: descriptor.sha256, executable: true };
    }), gates: requiredGates.map(name => ({ name, sourceCommit, status: 'passed',
      receiptPath: `evidence/${name}.json`, receiptSha256: files.find(value =>
        value.path === `evidence/${name}.json`).sha256 })) };
  const policy = { version: 1, sourceCommit, sourceTree, envelopeSha256: digest(encode(document)),
    archiveSha256, nestedCompiler: structuredClone(nestedCompiler), requiredGates: [...requiredGates] };
  return { document, policy };
}

function reseal(document, policy) {
  const input = encode(document);
  return { input, policy: { ...policy, envelopeSha256: digest(input) } };
}

function rejects(mutate, pattern) {
  const { document, policy } = fixture();
  mutate(document, policy);
  const sealed = reseal(document, policy);
  // Recomputing the independently expected envelope digest cannot waive fixed policy checks.
  assert.throws(() => validateToolkitEnvelope(sealed.input, sealed.policy), pattern);
}

function syncMaterial(document, path) {
  const descriptor = document.files.find(value => value.path === path);
  for (const material of document.materials.filter(value => value.path === path)) {
    material.bytes = descriptor.bytes;
    material.sha256 = descriptor.sha256;
    material.executable = descriptor.mode === 0o755;
  }
}

test('explicit synthetic canonical envelope matches independently selected policy fields', () => {
  const { document, policy } = fixture();
  const result = validateToolkitEnvelope(encode(document), policy);
  assert.deepEqual(JSON.parse(JSON.stringify(result)), document);
  assert.equal(result.source.commit, sourceCommit);
  assert.equal(result.gates.length, 2);
});

test('independently supplied policy pins reject invalid versions hashes and source claims', () => {
  for (const mutation of [{ version: 2 }, { sourceCommit: 'invalid' }, { sourceTree: 'invalid' },
    { archiveSha256: 'invalid' }]) {
    rejects((_document, policy) => Object.assign(policy, mutation), /TOOLKIT-POLICY/);
  }
  rejects((_document, policy) => { policy.archiveSha256 = '0'.repeat(64); }, /TOOLKIT-ARCHIVE/);
  rejects((_document, policy) => { policy.sourceCommit = '0'.repeat(40); }, /TOOLKIT-SOURCE/);
  rejects((_document, policy) => { policy.sourceTree = '0'.repeat(40); }, /TOOLKIT-SOURCE/);
  const { document, policy } = fixture();
  assert.throws(() => validateToolkitEnvelope(encode(document), { ...policy, envelopeSha256: 'invalid' }),
    /TOOLKIT-POLICY/);
});

test('every envelope and nested metadata object rejects extra or omitted fields', () => {
  for (const access of [value => value, value => value.source, value => value.signing,
    value => value.archive, value => value.nestedCompiler, value => value.browser,
    value => value.files[0], value => value.materials[0], value => value.gates[0]]) {
    rejects(document => { access(document).extra = true; }, /SHAPE/);
    rejects(document => { const item = access(document); delete item[Object.keys(item)[0]]; }, /SHAPE/);
  }
  rejects((_document, policy) => { policy.extra = true; }, /SHAPE/);
  rejects((_document, policy) => { delete policy.archiveSha256; }, /SHAPE/);
});

test('source repository tag exact commit tree and epoch remain fixed after resealing', () => {
  for (const mutation of [{ repository: 'https://github.com/other/other' }, { ref: 'refs/heads/main' },
    { commit: '1'.repeat(40) }, { tree: '2'.repeat(40) }, { commit: 'A'.repeat(40) },
    { sourceDateEpoch: -1 }, { sourceDateEpoch: 0x100000000 }]) {
    rejects(document => Object.assign(document.source, mutation), /TOOLKIT-SOURCE/);
  }
  for (const mutation of [{ format: 'other' }, { version: 2 }, { productVersion: '0.2.0' },
    { target: 'x86_64-pc-windows-msvc' }]) {
    rejects(document => Object.assign(document, mutation), /TOOLKIT-VERSION/);
  }
});

test('signer issuer identity and browser pins cannot be replaced by adjacent metadata', () => {
  for (const mutation of [{ issuer: 'https://foreign.invalid' }, { certificateIdentity:
    'https://github.com/zryna/zryna/.github/workflows/playground-release.yml@refs/heads/main' }]) {
    rejects(document => Object.assign(document.signing, mutation), /TOOLKIT-SIGNER/);
  }
  for (const mutation of [{ version: '153.0.8010.13' }, { archiveSha256: '0'.repeat(64) },
    { inventorySha256: '1'.repeat(64) }]) {
    rejects(document => Object.assign(document.browser, mutation), /TOOLKIT-BROWSER/);
  }
});

test('archive and nested compiler descriptors must match separately supplied pins', () => {
  for (const mutation of [{ path: '../toolkit.tar.gz' }, { sha256: '0'.repeat(64) },
    { bytes: 0 }, { bytes: 537919489 }]) {
    rejects(document => Object.assign(document.archive, mutation), /TOOLKIT-ARCHIVE/);
  }
  for (const mutation of [{ version: '0.2.4' }, { envelopeSha256: '0'.repeat(64) },
    { archiveSha256: '0'.repeat(64) }, { sourceCommit: '0'.repeat(40) }]) {
    rejects(document => Object.assign(document.nestedCompiler, mutation), /TOOLKIT-NESTED-COMPILER/);
  }
  rejects((_document, policy) => { policy.nestedCompiler.version = 'other'; }, /TOOLKIT-NESTED-COMPILER/);
});

test('captured compiler and browser release files are mandatory after metadata resealing', () => {
  for (const path of Object.keys(captured)) {
    rejects(document => { document.files = document.files.filter(value => value.path !== path); }, /TOOLKIT-INVENTORY/);
    rejects(document => { document.files.find(value => value.path === path).path += '-substitute';
      document.files.sort((a, b) => a.path < b.path ? -1 : 1); }, /TOOLKIT-INVENTORY/);
  }
});

test('captured nested and browser descriptors retain exact separately selected archive and inventory hashes', () => {
  for (const path of Object.keys(captured).filter(path => !path.endsWith('.sigstore.json'))) {
    rejects(document => { document.files.find(value => value.path === path).sha256 = digest('substituted'); },
      /TOOLKIT-CAPTURED-PIN/);
  }
  for (const [path, field] of [['releases/compiler-envelope.json', 'envelopeSha256'],
    ['releases/zryna-0.2.3-x86_64-unknown-linux-gnu.tar.gz', 'archiveSha256']]) {
    rejects(document => {
      const hash = digest('adjacent-substitution');
      document.files.find(value => value.path === path).sha256 = hash;
      document.nestedCompiler[field] = hash;
    }, /TOOLKIT-NESTED-COMPILER/);
  }
});

test('captured file-specific byte ceilings admit each endpoint and reject its first extra byte', () => {
  for (const [path, [, maximum]] of Object.entries(captured)) {
    const { document, policy } = fixture();
    document.files.find(value => value.path === path).bytes = maximum;
    let sealed = reseal(document, policy);
    assert.doesNotThrow(() => validateToolkitEnvelope(sealed.input, sealed.policy));
    document.files.find(value => value.path === path).bytes++;
    sealed = reseal(document, policy);
    assert.throws(() => validateToolkitEnvelope(sealed.input, sealed.policy));
  }
});

test('captured release and signature files must retain a nonexecutable immutable inventory mode', () => {
  for (const path of Object.keys(captured)) {
    rejects(document => { document.files.find(value => value.path === path).mode = 0o755; }, /TOOLKIT-CAPTURED-PIN/);
  }
});

test('captured compiler signature bundle bytes remain pinned by the selected outer envelope digest', () => {
  const { document, policy } = fixture();
  document.files.find(value => value.path === 'releases/compiler-envelope.sigstore.json').sha256 = digest('different-bundle');
  assert.throws(() => validateToolkitEnvelope(encode(document), policy), /TOOLKIT-ENVELOPE/);
});

test('finite material inventory rejects malformed hash mode bounds and missing roles', () => {
  for (const mutation of [{ sha256: 'invalid' }, { mode: 0o777 }, { bytes: 0 }, { bytes: 268435457 }]) {
    rejects(document => Object.assign(document.files[0], mutation), /TOOLKIT-INVENTORY/);
  }
  for (const path of ['bin/zryna-playground-compiler', 'policy.json', 'resources/loader.js', 'resources/browser.wit']) {
    rejects(document => { document.files = document.files.filter(file => file.path !== path); }, /TOOLKIT-INVENTORY/);
  }
  rejects(document => { document.files = []; }, /TOOLKIT-INVENTORY/);
  rejects(document => { document.files.find(file => file.path === 'bin/zryna-playground-compiler').mode = 0o644;
    syncMaterial(document, 'bin/zryna-playground-compiler'); }, /TOOLKIT-INVENTORY/);
  rejects(document => { document.files.push(...Array.from({ length: 122 }, (_, index) => file(`zz/${index}`))); },
    /TOOLKIT-INVENTORY/);
});

test('portable inventory paths reject traversal duplicates case aliases and file-directory overlap', () => {
  for (const path of ['../escape', '/absolute', 'dir\\file', 'dir/CON.txt', 'dir/end.', 'dir//file', 'é/file']) {
    rejects(document => { document.files[0].path = path; });
  }
  rejects(document => { document.files.push({ ...document.files[0] }); });
  rejects(document => { document.files.reverse(); });
  for (const path of ['Policy.json', 'resources/LOADER.js', 'bin']) {
    rejects(document => { document.files.push(file(path)); document.files.sort((a, b) => a.path < b.path ? -1 : 1); });
  }
  rejects(document => { document.files.push(file('Resources/extra.js'));
    document.files.sort((a, b) => a.path < b.path ? -1 : 1); });
});

test('materials match file identity bytes executable mode and fixed mount domains', () => {
  for (const mutation of [{ path: 'missing' }, { bytes: 18 }, { sha256: '0'.repeat(64) },
    { executable: false }, { executable: 1 }, { mount: 'relative' }, { mount: '/etc/passwd' },
    { mount: '/materials/../escape' }, { mount: '/materials//node' }]) {
    rejects(document => Object.assign(document.materials[1], mutation));
  }
  rejects(document => { document.materials[1].mount = '/app/compiler'; }, /TOOLKIT-MATERIALS/);
  rejects(document => { document.materials = []; }, /TOOLKIT-MATERIALS/);
  rejects(document => { document.materials.splice(1); }, /TOOLKIT-MATERIALS/);
  rejects(document => { document.materials[0].path = document.materials[1].path; }, /TOOLKIT-MATERIALS/);
});

test('required node mount must retain an executable material after recomputing adjacent mode claims', () => {
  rejects(document => { document.files.find(file => file.path === 'runtime/node/bin/node').mode = 0o644;
    syncMaterial(document, 'runtime/node/bin/node'); }, /TOOLKIT-MATERIALS/);
});

test('mount paths cannot describe both a file and a descendant file', () => {
  rejects(document => { const descriptor = file('runtime/extra'); document.files.push(descriptor);
    document.files.sort((a, b) => a.path < b.path ? -1 : 1);
    document.materials.push({ path: descriptor.path, bytes: descriptor.bytes, sha256: descriptor.sha256,
      executable: false, mount: '/materials/runtime/node/bin/node/extra' }); });
});

test('required gates need exact passed current-source receipts present in the inventory', () => {
  for (const mutation of [{ name: 'foreign' }, { sourceCommit: '0'.repeat(40) }, { status: 'skipped' },
    { receiptPath: 'other.json' }, { receiptSha256: '0'.repeat(64) }]) {
    rejects(document => Object.assign(document.gates[0], mutation), /TOOLKIT-GATES/);
  }
  rejects(document => { document.gates = []; }, /TOOLKIT-GATES/);
  rejects(document => { document.gates[1] = { ...document.gates[0] }; }, /TOOLKIT-GATES/);
  rejects(document => { document.files = document.files.filter(file => file.path !== document.gates[0].receiptPath); },
    /TOOLKIT-GATES/);
  for (const gates of [[], ['a', 'a'], ['Bad-name'], ['a'.repeat(65)], Array(129).fill('a')]) {
    rejects((_document, policy) => { policy.requiredGates = gates; }, /TOOLKIT-GATES/);
  }
});

test('inclusive declared file and archive bounds accept while their first extra bytes reject', () => {
  const { document, policy } = fixture();
  document.archive.bytes = 537919488;
  document.files.find(file => file.path === 'bin/zryna-playground-compiler').bytes = 268435456;
  document.files.find(file => file.path === 'runtime/node/bin/node').bytes = 268435456 - (document.files.length - 2) * 17;
  for (const path of ['bin/zryna-playground-compiler', 'runtime/node/bin/node']) syncMaterial(document, path);
  let sealed = reseal(document, policy);
  assert.doesNotThrow(() => validateToolkitEnvelope(sealed.input, sealed.policy));
  document.files.find(file => file.path === 'runtime/node/bin/node').bytes++;
  syncMaterial(document, 'runtime/node/bin/node');
  sealed = reseal(document, policy);
  assert.throws(() => validateToolkitEnvelope(sealed.input, sealed.policy), /TOOLKIT-INVENTORY/);
});

test('finite files and material counts admit exactly 128 and reject the first extra entry', () => {
  const { document, policy } = fixture();
  document.files.push(...Array.from({ length: 128 - document.files.length }, (_, index) => file(`extra/item-${String(index).padStart(3, '0')}`)));
  document.files.sort((a, b) => a.path < b.path ? -1 : 1);
  document.materials = document.files.map(descriptor => ({ path: descriptor.path, bytes: descriptor.bytes,
    sha256: descriptor.sha256, executable: descriptor.mode === 0o755,
    mount: descriptor.path === 'bin/zryna-playground-compiler' ? '/app/compiler' : `/materials/${descriptor.path}` }));
  let sealed = reseal(document, policy);
  assert.equal(document.files.length, 128);
  assert.equal(document.materials.length, 128);
  assert.doesNotThrow(() => validateToolkitEnvelope(sealed.input, sealed.policy));
  document.materials.push({ ...document.materials[1], mount: '/materials/extra/first-extra' });
  sealed = reseal(document, policy);
  assert.throws(() => validateToolkitEnvelope(sealed.input, sealed.policy), /TOOLKIT-MATERIALS/);
  document.materials.pop();
  document.files.push(file('zz/first-extra'));
  sealed = reseal(document, policy);
  assert.throws(() => validateToolkitEnvelope(sealed.input, sealed.policy), /TOOLKIT-INVENTORY/);
});

test('canonical framing rejects duplicate keys trailing bytes noncanonical numbers and carrier overflow', () => {
  const { document, policy } = fixture();
  const canonical = encode(document);
  for (const input of [Buffer.concat([canonical, Buffer.from(' ')]), Buffer.concat([canonical, Buffer.from('{}')]),
    Buffer.from(canonical.toString().replace('"version":1', '"version":1,"version":1')),
    Buffer.from(canonical.toString().replace('"version":1', '"version":1,"vers\\u0069on":1')),
    Buffer.from(canonical.toString().replace('"version":1', '"version":1.0')),
    Buffer.from(canonical.toString().replace('"version":1', '"version":1e0')),
    Buffer.concat([Buffer.from([0xef, 0xbb, 0xbf]), canonical]), Buffer.from([255]), Buffer.alloc(262145, 32)]) {
    assert.throws(() => validateToolkitEnvelope(input, { ...policy, envelopeSha256: digest(input) }));
  }
  assert.throws(() => validateToolkitEnvelope(canonical, { ...policy, envelopeSha256: '0'.repeat(64) }),
    /TOOLKIT-ENVELOPE/);
});
