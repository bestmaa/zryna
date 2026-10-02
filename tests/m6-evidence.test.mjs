import assert from 'node:assert/strict';
import test from 'node:test';
import { registry, requiredGates } from '../scripts/m6/registry.mjs';
import { registrySha256, validateM6Record } from '../scripts/m6/evidence.mjs';
import { fixture, integrationTree, hash, encode, retained, products, artifactVersions,
  selectedGateArtifacts, gateMaterials, selectedFamilies, integrationCommit } from './m6/fixture.mjs';

function reject(change, pattern = /./) {
  const { record, policy } = fixture();
  change(record, policy);
  const input = encode(record);
  // Re-selecting the digest must not bypass the remaining fixed policy checks.
  policy.recordSha256 = hash(input);
  assert.throws(() => validateM6Record(input, policy), pattern);
}

test('version one obligations independently select products, artifacts and every platform case', () => {
  assert.equal(registry.format, 'zryna.m6-registry.v1');
  assert.equal(registry.version, 1);
  assert.deepEqual(registry.products, products);
  assert.deepEqual([...registry.requiredArtifacts].sort(), Object.keys(artifactVersions).sort());
  assert.deepEqual(Object.fromEntries(registry.gates.map(row =>
    [row.id, [row.platforms.join(' '), row.cases.join(' ')]])), selectedFamilies);
  const expected = Object.entries(selectedFamilies).flatMap(([id, [platforms]]) =>
    platforms.split(' ').map(platform => `${id}-${platform}`));
  assert.deepEqual([...requiredGates], expected);
  assert.equal(expected.length, 23);
  assert.equal(expected.filter(id => id.endsWith('-linux')).length, 14);
  assert.equal(expected.filter(id => id.endsWith('-windows')).length, 9);
  assert.equal(registrySha256, hash(encode(registry)));
});

test('a complete synthetic record round trips without asserting executed or authenticated results', () => {
  const { record, policy } = fixture();
  assert.deepEqual(JSON.parse(JSON.stringify(validateM6Record(encode(record), policy))), record);
});

test('record digest, integration commit and integration tree are independently selected', () => {
  const { record, policy } = fixture();
  policy.recordSha256 = hash('other-record');
  assert.throws(() => validateM6Record(encode(record), policy), /M6-RECORD/);
  for (const key of ['commit', 'tree']) reject(r => { r.integration[key] = '3'.repeat(40); }, /M6-INTEGRATION/);
  reject(r => { r.integration.clean = false; }, /M6-INTEGRATION/);
  reject(r => { r.integration.repository += '/fork'; }, /M6-INTEGRATION/);
});

test('closed versioned record and policy reject missing, unknown and altered authority fields', () => {
  for (const field of ['format', 'version', 'registrySha256']) {
    reject(r => { delete r[field]; });
    reject(r => { r[field] = field === 'version' ? 2 : 'unselected'; });
  }
  reject(r => { r.support = true; }, /SHAPE/);
  reject((r, p) => { p.integrationCommit = 'A'.repeat(40); }, /M6-POLICY/);
  reject((r, p) => { p.version = 2; }, /M6-POLICY/);
  reject((r, p) => { p.extra = true; }, /SHAPE/);
});

test('every platform gate is mandatory and duplicate, unknown and case-altered identifiers fail', () => {
  reject(r => { r.gates.pop(); }, /M6-GATES/);
  reject(r => { r.gates[1] = structuredClone(r.gates[0]); }, /M6-GATE/);
  for (const id of ['PROJECT-linux', 'project-macos', 'unknown-linux']) {
    reject(r => { r.gates[0].id = id; }, /M6-GATE/);
  }
  reject(r => { r.gates[0].environment.platform = 'windows'; }, /M6-ENVIRONMENT/);
  reject(r => { r.gates[0].environment.architecture = 'aarch64'; }, /M6-ENVIRONMENT/);
  reject(r => { r.gates[0].integrationCommit = '3'.repeat(40); }, /M6-GATE/);
});

test('all cases must be distinct completed positive observations with no failures or skips', () => {
  reject(r => { r.gates[0].cases.pop(); }, /M6-CASES/);
  reject(r => { r.gates[0].cases[1] = { ...r.gates[0].cases[0] }; }, /M6-CASES/);
  reject(r => { r.gates[0].cases[0].id = 'Creation'; }, /M6-CASES/);
  for (const [field, value] of [['passed', 0], ['passed', -1], ['passed', 1000001], ['failed', 1], ['skipped', 1]]) {
    reject(r => { r.gates[0].cases[0][field] = value; }, /M6-CASES/);
  }
  reject(r => { r.gates[0].exitCode = 1; }, /M6-GATE/);
});

test('artifact roles, versions and digests must match the independently selected full inventory', () => {
  reject(r => { r.artifacts.pop(); }, /M6-ARTIFACTS/);
  reject(r => { r.artifacts[1] = structuredClone(r.artifacts[0]); }, /M6-ARTIFACTS/);
  reject(r => { r.artifacts[0].role = 'Compiler-release'; });
  reject(r => { r.artifacts[0].sha256 = hash('substituted'); }, /M6-ARTIFACT-PIN/);
  reject(r => { r.artifacts[0].version = '0.2.4'; }, /M6-ARTIFACT-PIN/);
  reject((r, p) => { p.artifactPins[1] = { ...p.artifactPins[0] }; }, /M6-ARTIFACT-PIN/);
});

test('fixed product versions still reject when adjacent artifact policy pins are resealed', () => {
  for (const role of ['compiler-release', 'server-linux', 'server-windows', 'extension',
    'setup-linux', 'setup-windows', 'playground-toolkit', 'playground-compiler', 'chrome-linux']) {
    reject((r, p) => {
      r.artifacts.find(row => row.role === role).version = '9.9.9';
      p.artifactPins.find(row => row.role === role).version = '9.9.9';
    }, /M6-VERSION/);
  }
});

test('artifact paths cannot alias by duplicate, case or file-directory substitution', () => {
  for (const path of ['artifacts/compiler-release.bin', 'Artifacts/compiler-release.bin', 'artifacts/compiler-release.bin/child']) {
    reject(r => { r.artifacts[1].path = path; });
  }
  for (const path of ['../escape', '/absolute', 'artifacts/NUL.bin', 'artifacts\\escape']) {
    reject(r => { r.artifacts[0].path = path; });
  }
});

test('source provenance must use a recognized kind and a bounded valid original identity', () => {
  reject(r => { r.artifacts[0].source.kind = 'unchecked-local'; }, /M6-SOURCE/);
  for (const revision of ['', 'main', `${integrationCommit}:main`, 'x'.repeat(1025)]) {
    reject(r => { r.artifacts[0].source.revision = revision; }, /M6-SOURCE/);
  }
  reject(r => { r.artifacts[0].source.sha256 = 'A'.repeat(64); }, /M6-SOURCE/);
  reject(r => { r.artifacts[0].source.verified = true; }, /SHAPE/);
});

test('resealing the record cannot change independently selected artifact provenance or retention identity', () => {
  reject(r => { r.artifacts[0].source.revision = `${'3'.repeat(40)}:${integrationTree}`; }, /M6-ARTIFACT-PIN/);
  reject(r => { r.artifacts[0].source.sha256 = hash('other-source'); }, /M6-ARTIFACT-PIN/);
  reject(r => { r.artifacts[0].source.kind = 'upstream-archive'; }, /M6-ARTIFACT-PIN/);
  reject(r => { r.artifacts[0].path = 'artifacts/substituted.bin'; }, /M6-ARTIFACT-PIN/);
  reject(r => { r.artifacts[0].url = retained('substituted.bin'); }, /M6-ARTIFACT-PIN/);
  reject(r => { r.artifacts[0].bytes = 2; }, /M6-ARTIFACT-PIN/);
});

test('gate execution declares existing distinct materials and a finite recorded command', () => {
  for (const roles of [[], ['missing'], ['compiler-release', 'compiler-release']]) {
    reject(r => { r.gates[0].artifactRoles = roles; }, /M6-EXECUTION/);
  }
  for (const command of [[], [''], Array(129).fill('x'), ['x'.repeat(1025)]]) {
    reject(r => { r.gates[0].command = command; }, /M6-EXECUTION/);
  }
  reject(r => { r.gates[0].environment.runtime = ''; }, /M6-ENVIRONMENT/);
  reject(r => { r.gates[0].environment.osVersion = 'x'.repeat(1025); }, /M6-ENVIRONMENT/);
});

test('each gate family independently selects the relevant product materials for both platforms', () => {
  assert.deepEqual(Object.fromEntries(registry.gates.map(row =>
    [row.id, row.artifactRoles?.join(' ')])), selectedGateArtifacts);
  const { record, policy } = fixture();
  for (const gate of record.gates) gate.artifactRoles = gateMaterials(gate.id);
  policy.recordSha256 = hash(encode(record));
  assert.doesNotThrow(() => validateM6Record(encode(record), policy));
});

test('resealed complete receipts reject omission of every required gate material', () => {
  for (const id of requiredGates) {
    for (const missing of gateMaterials(id)) {
      const { record, policy } = fixture();
      const gate = record.gates.find(row => row.id === id);
      // All other known materials remain present, so only relevance coverage is absent.
      gate.artifactRoles = gate.artifactRoles.filter(role => role !== missing);
      const input = encode(record);
      policy.recordSha256 = hash(input);
      assert.throws(() => validateM6Record(input, policy), /M6-/, `${id} omitted ${missing}`);
    }
  }
});

test('the other platform server or setup cannot replace the required platform artifact', () => {
  for (const id of requiredGates) {
    for (const role of gateMaterials(id).filter(role => /^(server|setup)-/.test(role))) {
      reject(r => {
        const gate = r.gates.find(row => row.id === id);
        gate.artifactRoles = gateMaterials(id).map(value => value === role ?
          role.replace(/-(linux|windows)$/, suffix => suffix === '-linux' ? '-windows' : '-linux') : value);
      }, /M6-/);
    }
  }
});

test('timestamps reject invalid, noncanonical and backwards observations', () => {
  for (const time of ['2026-02-30T00:00:00.000Z', '2026-10-02T00:00:00Z', 'invalid']) {
    reject(r => { r.gates[0].finishedAt = time; }, /M6-TIME/);
  }
  reject(r => { r.gates[0].finishedAt = '2026-10-01T23:59:59.000Z'; }, /M6-TIME/);
});

test('receipt identity binds gate-specific path with bounded bytes and digest', () => {
  reject(r => { r.gates[0].receipt.path = r.gates[1].receipt.path; }, /M6-RECEIPT/);
  reject(r => { r.gates[0].receipt.sha256 = '0'.repeat(63); }, /M6-RECEIPT/);
  for (const bytes of [0, -1, 262145]) reject(r => { r.gates[0].receipt.bytes = bytes; }, /M6-RECEIPT/);
});

test('retention rejects expiring Actions artifacts, credentials, queries and non-asset URLs', () => {
  const urls = ['https://github.com/zryna/zryna/actions/runs/1/artifacts/2',
    retained('file') + '?token=temporary', retained('file') + '#fragment',
    'https://user:secret@github.com/zryna/zryna/releases/download/tag/file',
    'http://github.com/zryna/zryna/releases/download/tag/file',
    'https://example.org/releases/file', 'https://github.com/zryna/zryna/releases/download/',
    'https://open-vsx.org/', 'https://open-vsx.org/extension/zryna/zryna/0.5.0',
    'https://marketplace.visualstudio.com/items?itemName=zryna.zryna'];
  for (const url of urls) {
    reject(r => { r.artifacts[0].url = url; }, /M6-RETENTION/);
    reject(r => { r.gates[0].receipt.url = url; }, /M6-RETENTION/);
    reject(r => {
      r.publication.verificationReceipt.url = url;
      r.gates.find(row => row.id === 'publication-linux').receipt.url = url;
    }, /M6-RETENTION/);
    reject((r, p) => {
      r.artifacts.find(row => row.role === 'extension').url = url;
      p.artifactPins.find(row => row.role === 'extension').url = url;
    }, /M6-RETENTION/);
  }
});

test('hosted jobs require successful exact integration source and finite run/job identifiers', () => {
  const hosted = r => r.gates.find(row => row.id === 'hosted-linux');
  reject(r => { hosted(r).hosted.sourceCommit = '3'.repeat(40); }, /M6-HOSTED/);
  for (const conclusion of ['skipped', 'failure', 'cancelled', 'pending']) {
    reject(r => { hosted(r).hosted.conclusion = conclusion; }, /M6-HOSTED/);
  }
  for (const value of [0, -1, Number.MAX_SAFE_INTEGER + 1]) {
    reject(r => { hosted(r).hosted.jobId = value; });
  }
  reject(r => { hosted(r).hosted = null; }, /SHAPE/);
  reject(r => { r.gates[0].hosted = { ...hosted(r).hosted }; }, /M6-HOSTED/);
});

test('publication record cannot substitute toolkit or documentation identities or claim a review phase', () => {
  for (const status of ['draft', 'under-verification', 'published']) {
    reject(r => { r.publication.status = status; }, /M6-PUBLICATION/);
  }
  for (const field of ['toolkitSha256', 'docsManifestSha256']) {
    reject(r => { r.publication[field] = hash('unselected'); }, /M6-PUBLICATION/);
  }
  reject(r => { r.publication.verificationReceipt.sha256 = ''; });
});

test('publication verification descriptor binds the publication gate digest and retained location', () => {
  reject(r => { r.publication.verificationReceipt.sha256 = hash('other-verification'); }, /M6-PUBLICATION/);
  reject(r => { r.gates.find(row => row.id === 'publication-linux').receipt.sha256 = hash('other-gate'); }, /M6-PUBLICATION/);
  reject(r => { r.publication.verificationReceipt.path = 'evidence/other.json'; }, /M6-PUBLICATION/);
  reject(r => { r.publication.verificationReceipt.bytes = 2; }, /M6-PUBLICATION/);
  reject(r => { r.publication.verificationReceipt.url = retained('other.json'); }, /M6-PUBLICATION/);
  for (const bytes of [0, -1, 262145]) reject(r => { r.publication.verificationReceipt.bytes = bytes; });
  reject(r => { delete r.publication.verificationReceipt.url; }, /SHAPE/);
  reject(r => { r.publication.verificationReceipt.extra = true; }, /SHAPE/);
  reject(r => {
    r.publication.verificationReceiptSha256 = r.publication.verificationReceipt.sha256;
    delete r.publication.verificationReceipt;
  }, /SHAPE/);
});

test('finite artifact and case bounds admit their endpoints but reject overflow', () => {
  const { record, policy } = fixture();
  record.artifacts[0].bytes = 2147483648;
  policy.artifactPins[0].bytes = 2147483648;
  record.gates[0].receipt.bytes = 262144;
  record.gates[0].cases[0].passed = 1000000;
  record.publication.verificationReceipt.bytes = 262144;
  record.gates.find(row => row.id === 'publication-linux').receipt.bytes = 262144;
  policy.recordSha256 = hash(encode(record));
  assert.doesNotThrow(() => validateM6Record(encode(record), policy));
  for (const bytes of [0, -1, 2147483649]) reject(r => { r.artifacts[0].bytes = bytes; }, /M6-ARTIFACT/);
  reject((r, p) => { p.artifactPins = Array(129).fill(p.artifactPins[0]); }, /M6-POLICY/);
});

test('duplicate keys, alternate ordering, whitespace and trailing bytes reject despite selected digest', () => {
  const { record, policy } = fixture();
  const canonical = encode(record);
  const text = canonical.toString();
  const carriers = [Buffer.from(text.replace('"version":1}', '"version":1,"version":1}')),
    Buffer.from(JSON.stringify(record) + '\n'), Buffer.from(' ' + text), Buffer.from(text + '{}'),
    canonical.subarray(0, canonical.length - 1), Buffer.from(text.replace('"passed":1', '"passed":1e0')),
    Buffer.from(text.replace('"passed":1', '"passed":-0')), Buffer.from([0xff]), Buffer.alloc(262145, 32)];
  for (const input of carriers) {
    policy.recordSha256 = hash(input);
    assert.throws(() => validateM6Record(input, policy));
  }
});
