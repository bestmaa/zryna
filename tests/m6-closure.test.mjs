import assert from 'node:assert/strict';
import test from 'node:test';
import { assembleM6Record, authenticateM6Evidence, m6Evidence,
  validateToolkitComposition } from '../scripts/m6/closure.mjs';
import { fixture, retainedFixture, encode, hash } from './m6/fixture.mjs';

// Assembly tests retain synthetic supplied observations; none authenticates a signed result.
function assemblyFixture() {
  const { record, carriers } = retainedFixture();
  const { policy } = fixture();
  policy.recordSha256 = hash(encode(record));
  return { record, policy, input: { integration: record.integration, artifacts: record.artifacts,
    gates: record.gates, publication: record.publication, receipts: carriers } };
}

function authenticationInput(toolkit = {}) {
  const input = { record: Buffer.from('{}'), bundle: Buffer.from('{}'), policy: {}, verifierPolicy: {},
    trustedRoot: Buffer.from('{}'), receipts: [], toolkit };
  // A forged capability must fail before metadata policies, receipts or signature verification.
  for (const key of ['policy', 'verifierPolicy', 'receipts']) Object.defineProperty(input, key, {
    enumerable: true, get() { throw new Error(`unexpected ${key} consumption`); },
  });
  return input;
}

test('forged public toolkit metadata cannot enter evidence authentication before capability checks', () => {
  const authority = { toolkitSha256: hash('toolkit'), compilerSha256: hash('compiler'),
    policySha256: hash('policy'), bindingTemplateSha256: hash('binding') };
  const publicShape = { authority, sourceCommit: '1'.repeat(40), sourceTree: '2'.repeat(40),
    witSha256: hash('wit') };
  for (const toolkit of [{}, publicShape, Object.freeze(structuredClone(publicShape)),
    Object.create(publicShape), { ...publicShape, materials: [] }, null]) {
    assert.throws(() => authenticateM6Evidence(authenticationInput(toolkit)), /TOOLKIT-CAPABILITY/);
  }
});

test('closed authentication input rejects unknown or omitted carriers before any capability admission', () => {
  const unknown = authenticationInput();
  unknown.verified = true;
  assert.throws(() => authenticateM6Evidence(unknown), /SHAPE/);
  const missing = authenticationInput();
  delete missing.toolkit;
  assert.throws(() => authenticateM6Evidence(missing), /SHAPE/);
});

test('empty, oversized and nonbyte record, bundle and root carriers reject before toolkit processing', () => {
  for (const [key, maximum] of [['record', 262144], ['bundle', 1048576], ['trustedRoot', 262144]]) {
    for (const value of [Buffer.alloc(0), Buffer.alloc(maximum + 1), 'not-bytes', new ArrayBuffer(2)]) {
      const input = authenticationInput();
      input[key] = value;
      assert.throws(() => authenticateM6Evidence(input), /M6-CLOSURE-BYTES/);
    }
  }
});

test('synthetic record assembly preserves the complete supplied observations and captured bytes', () => {
  const { record, policy, input } = assemblyFixture();
  assert.deepEqual(assembleM6Record(input, policy), encode(record));
});

test('assembly preserves unusual supplied counts, commands and environment instead of normalizing them', () => {
  const { record, policy, input } = assemblyFixture();
  const gate = record.gates[0];
  gate.cases[0].passed = 42;
  gate.command = ['synthetic-contract-check', 'explicit-argument'];
  gate.environment.runtime = 'synthetic-observed-runtime';
  const carrier = input.receipts.find(row => row.path === gate.receipt.path);
  const value = JSON.parse(carrier.data.toString());
  const { receipt: unused, ...observation } = gate;
  value.observation = observation;
  carrier.data = encode(value);
  gate.receipt.bytes = carrier.data.length;
  gate.receipt.sha256 = hash(carrier.data);
  policy.recordSha256 = hash(encode(record));
  const assembled = JSON.parse(assembleM6Record(input, policy).toString());
  assert.equal(assembled.gates[0].cases[0].passed, 42);
  assert.deepEqual(assembled.gates[0].command, gate.command);
  assert.equal(assembled.gates[0].environment.runtime, gate.environment.runtime);
});

test('assembly rejects missing retained receipt despite a complete independently pinned metadata record', () => {
  const { policy, input } = assemblyFixture();
  input.receipts = input.receipts.filter(row => row.path !== 'evidence/project-linux.json');
  assert.throws(() => assembleM6Record(input, policy), /M6-RETAINED-MISSING/);
});

test('assembly rejects substituted source and digest policy instead of resealing it for the caller', () => {
  for (const change of [p => { p.integrationTree = '3'.repeat(40); },
    p => { p.recordSha256 = hash('other'); }, p => { p.artifactPins[0].sha256 = hash('other'); }]) {
    const { policy, input } = assemblyFixture();
    change(policy);
    assert.throws(() => assembleM6Record(input, policy), /M6-/);
  }
});

test('assembly rejects resealed metadata observations that disagree with actual retained receipt bytes', () => {
  const { record, policy, input } = assemblyFixture();
  record.gates[0].cases[0].passed++;
  policy.recordSha256 = hash(encode(record));
  assert.throws(() => assembleM6Record(input, policy), /M6-RECEIPT-OBSERVATION/);
});

test('detached evidence cannot be retrieved through a forged or cloned metadata capability', () => {
  for (const capability of [{}, null, { recordSha256: hash('record'), integrationCommit: '1'.repeat(40),
    integrationTree: '2'.repeat(40) }]) assert.throws(() => m6Evidence(capability), /M6-CLOSURE-CAPABILITY/);
});

// Literal relationship fields are independent of the schema fixture and confer no capability.
function compositionFixture() {
  return {
    record: { integration: { commit: '1'.repeat(40), tree: '2'.repeat(40) }, artifacts: [
      { role: 'playground-toolkit', sha256: 'a'.repeat(64),
        source: { kind: 'git', revision: `${'1'.repeat(40)}:${'2'.repeat(40)}` } },
      { role: 'playground-compiler', sha256: 'b'.repeat(64),
        source: { kind: 'git', revision: `${'1'.repeat(40)}:${'2'.repeat(40)}` } },
      { role: 'compiler-release', sha256: 'c'.repeat(64),
        source: { kind: 'git', revision: `${'3'.repeat(40)}:${'4'.repeat(40)}` } },
      { role: 'chrome-linux', sha256: 'd'.repeat(64),
        source: { kind: 'upstream-archive', revision: '153.0.8010.12' } },
      { role: 'host-policy', sha256: 'e'.repeat(64),
        source: { kind: 'git', revision: `${'1'.repeat(40)}:${'2'.repeat(40)}` } },
      { role: 'node-runtime', sha256: '7'.repeat(64),
        source: { kind: 'upstream-archive', revision: '22.22.1' } },
      { role: 'provider', sha256: '8'.repeat(64),
        source: { kind: 'signed-package', revision: '6.0.3' } },
    ] },
    toolkit: { sourceCommit: '1'.repeat(40), sourceTree: '2'.repeat(40), authority: {
      toolkitSha256: 'a'.repeat(64), compilerSha256: 'b'.repeat(64), policySha256: 'e'.repeat(64),
    } },
    nested: { archiveSha256: 'c'.repeat(64), sourceCommit: '3'.repeat(40), sourceTree: '4'.repeat(40) },
    browser: { archiveSha256: 'd'.repeat(64) },
  };
}
const relationship = value => validateToolkitComposition(value.record, value.toolkit, value.nested, value.browser);
const role = (value, name) => value.record.artifacts.find(artifact => artifact.role === name);

test('pure composition preserves the distinct historical compiler source without creating a capability', () => {
  const value = compositionFixture(), unchanged = structuredClone(value);
  assert.equal(relationship(value), undefined);
  assert.deepEqual(value, unchanged);
  assert.notEqual(value.nested.sourceCommit, value.toolkit.sourceCommit);
  assert.notEqual(value.nested.sourceTree, value.toolkit.sourceTree);
  assert.throws(() => authenticateM6Evidence(authenticationInput(value.toolkit)), /TOOLKIT-CAPABILITY/);
  assert.throws(() => m6Evidence(value.toolkit), /M6-CLOSURE-CAPABILITY/);
});

test('each record digest binds its independently selected toolkit, compiler, browser or policy identity', () => {
  for (const name of ['playground-toolkit', 'playground-compiler', 'compiler-release', 'chrome-linux', 'host-policy']) {
    const value = compositionFixture();
    role(value, name).sha256 = '0'.repeat(64);
    assert.throws(() => relationship(value), /M6-TOOLKIT-BINDING/, name);
  }
  for (const change of [v => { v.toolkit.authority.toolkitSha256 = '0'.repeat(64); },
    v => { v.toolkit.authority.compilerSha256 = '0'.repeat(64); },
    v => { v.toolkit.authority.policySha256 = '0'.repeat(64); },
    v => { v.nested.archiveSha256 = '0'.repeat(64); }, v => { v.browser.archiveSha256 = '0'.repeat(64); }]) {
    const value = compositionFixture();
    change(value);
    assert.throws(() => relationship(value), /M6-TOOLKIT-BINDING/);
  }
});

test('composition source commit and tree must match the actual integration identity', () => {
  for (const object of ['record', 'toolkit']) for (const field of ['commit', 'tree']) {
    const value = compositionFixture();
    if (object === 'record') value.record.integration[field] = '5'.repeat(40);
    else value.toolkit[field === 'commit' ? 'sourceCommit' : 'sourceTree'] = '5'.repeat(40);
    assert.throws(() => relationship(value), /M6-TOOLKIT-BINDING/);
  }
});

test('historical compiler archive cannot be relabelled as the current integration source', () => {
  const value = compositionFixture();
  role(value, 'compiler-release').source.revision = `${'1'.repeat(40)}:${'2'.repeat(40)}`;
  assert.throws(() => relationship(value), /M6-TOOLKIT-SOURCE/);
  for (const field of ['sourceCommit', 'sourceTree']) {
    const changed = compositionFixture();
    changed.nested[field] = changed.toolkit[field];
    assert.throws(() => relationship(changed), /M6-TOOLKIT-SOURCE/);
  }
});

test('released compiler and playground artifacts require exact verified Git source kind and tree', () => {
  for (const name of ['compiler-release', 'playground-toolkit', 'playground-compiler']) {
    for (const kind of ['signed-package', 'upstream-archive']) {
      const value = compositionFixture();
      role(value, name).source.kind = kind;
      assert.throws(() => relationship(value), /M6-TOOLKIT-SOURCE/);
    }
    for (const field of [0, 1]) {
      const value = compositionFixture();
      const revision = role(value, name).source.revision.split(':');
      revision[field] = '5'.repeat(40);
      role(value, name).source.revision = revision.join(':');
      assert.throws(() => relationship(value), /M6-TOOLKIT-SOURCE/);
    }
  }
});

test('matching changed integration fields cannot leave playground Git provenance behind', () => {
  const value = compositionFixture();
  value.record.integration.tree = '5'.repeat(40);
  value.toolkit.sourceTree = '5'.repeat(40);
  assert.throws(() => relationship(value), /M6-TOOLKIT-SOURCE/);
});

test('node and provider file identities keep their separately selected source semantics', () => {
  const value = compositionFixture();
  assert.notEqual(role(value, 'node-runtime').sha256, value.nested.archiveSha256);
  assert.notEqual(role(value, 'provider').sha256, value.nested.archiveSha256);
  assert.equal(role(value, 'node-runtime').source.kind, 'upstream-archive');
  assert.equal(role(value, 'provider').source.kind, 'signed-package');
  assert.doesNotThrow(() => relationship(value));
});
