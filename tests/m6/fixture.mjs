import { createHash } from 'node:crypto';
import { registry } from '../../scripts/m6/registry.mjs';

// Synthetic metadata exercises validation only. No receipt, download or signature is verified.
const integrationCommit = '1'.repeat(40);
const integrationTree = '2'.repeat(40);
const hash = value => createHash('sha256').update(value).digest('hex');
const sorted = value => Array.isArray(value) ? value.map(sorted) :
  value !== null && typeof value === 'object' ? Object.fromEntries(
    Object.keys(value).sort().map(key => [key, sorted(value[key])])) : value;
const encode = value => Buffer.from(`${JSON.stringify(sorted(value))}\n`);
const retained = name => `https://github.com/zryna/zryna/releases/download/synthetic-m6-fixture/${name}`;
const products = { compiler: '0.2.3', server: '0.5.0', extension: '0.5.0',
  setup: '0.1.0-candidate.3', playground: '0.1.0', chrome: '153.0.8010.12' };
const artifactVersions = {
  'compiler-release': '0.2.3', 'node-runtime': '22.22.1', provider: '6.0.3',
  'server-linux': '0.5.0', 'server-windows': '0.5.0', extension: '0.5.0',
  'setup-linux': '0.1.0-candidate.3', 'setup-windows': '0.1.0-candidate.3',
  'playground-toolkit': '0.1.0', 'playground-compiler': '0.1.0',
  'chrome-linux': '153.0.8010.12', 'bubblewrap-linux': '0.12.0',
  'python-linux': '3.12.3', 'host-policy': '1.0.0', 'docs-bundle': '1.0.0',
};
const selectedGateArtifacts = {
  project: 'compiler-release node-runtime provider',
  lsp: 'compiler-release node-runtime provider server-platform',
  formatter: 'compiler-release node-runtime provider',
  extension: 'compiler-release node-runtime provider extension server-platform',
  setup: 'setup-platform extension server-platform',
  playground: 'playground-toolkit playground-compiler node-runtime provider',
  transport: 'playground-toolkit',
  containment: 'playground-toolkit playground-compiler node-runtime provider bubblewrap-linux python-linux host-policy',
  browser: 'playground-toolkit chrome-linux host-policy',
  accessibility: 'playground-toolkit chrome-linux host-policy',
  documentation: 'docs-bundle compiler-release server-platform extension setup-platform',
  publication: 'playground-toolkit compiler-release docs-bundle',
  canonical: 'compiler-release playground-compiler server-platform extension setup-platform',
  hosted: 'compiler-release playground-compiler server-platform extension setup-platform',
};
const gateMaterials = id => {
  const platform = id.endsWith('-windows') ? 'windows' : 'linux';
  const family = id.slice(0, -(platform.length + 1));
  return selectedGateArtifacts[family].split(' ').map(role => role.replace('-platform', `-${platform}`));
};
const selectedFamilies = {
  project: ['linux windows', 'creation unsafe-path collision retained-root frozen-source javascript webassembly forbidden-override'],
  lsp: ['linux windows', 'framing open-change-close unicode-crlf compiler-diagnostics cancel-stale unsupported-method scalar-definition'],
  formatter: ['linux windows', 'scalar-document scalar-range m2-document m2-range m3-document m3-range idempotence token-comment-preservation rejected-source saved-import cancel-stale limits'],
  extension: ['linux windows', 'vsix-identity pre-source-handshake workspace-trust profile-selection diagnostics definition formatting scalar-run m2-run saved-file root-substitution cleanup'],
  setup: ['linux windows', 'inventory relocation tamper compatibility reproduction project-editor-exercise'],
  playground: ['linux', 'corpus-cli-parity editable-source actual-diagnostics scalar-observations export-arity i32-boundaries malformed source-budget request-budget frame-budget unsupported expired revision-cancellation single-admission authority-substitution inert-text recovery'],
  transport: ['linux windows', 'closed-carriers byte-boundaries revision-identity late-upload overlapping-finish failed-teardown unsupported-host'],
  containment: ['linux', 'nonroot-host helper-identity sealed-materials namespaces seccomp-denials no-ambient-authority process-tree membership-before-source memory tasks cpu scratch output compile-deadline cancel-deadline reap-cleanup acquisition-faults recovery'],
  browser: ['linux', 'archive-identity fresh-profile cgroup-baseline zero-imports no-linear-memory external-watchdog zero-worker evaluation-deadline cancellation memory tasks cpu session-isolation recovery'],
  accessibility: ['linux', 'keyboard labels-status focus-invalid-input unicode-navigation accessibility-tree'],
  documentation: ['linux windows', 'support-matrix export-identity runnable-project-editor-exercises'],
  publication: ['linux', 'immutable-artifacts outer-signature nested-signature independent-download runnable-playground-exercise'],
  canonical: ['linux windows', 'architecture format clippy workspace-tests rustdoc adapter protocol m0 m2 m3 tooling'],
  hosted: ['linux windows', 'required-jobs exact-source no-required-skips retained-evidence'],
};

function selectedArtifact(role, version) {
  return {
    role, version, path: `artifacts/${role}.bin`, bytes: 1, sha256: hash(`artifact:${role}`),
    source: { kind: 'git', revision: `${integrationCommit}:${integrationTree}`, sha256: hash(`source:${role}`) },
    url: retained(`${role}.bin`),
  };
}

function fixture() {
  const artifacts = Object.entries(artifactVersions).map(([role, version]) => selectedArtifact(role, version));
  const gates = registry.gates.flatMap(family => family.platforms.map(platform => {
    const id = `${family.id}-${platform}`;
    return { id, integrationCommit, artifactRoles: artifacts.map(row => row.role),
      environment: { platform, architecture: 'x86_64', osVersion: 'synthetic', runtime: 'synthetic' },
      command: ['synthetic-contract-check', id], startedAt: '2026-10-02T00:00:00.000Z',
      finishedAt: '2026-10-02T00:00:01.000Z', exitCode: 0,
      cases: family.cases.map(id => ({ id, passed: 1, failed: 0, skipped: 0 })),
      receipt: { path: `evidence/${id}.json`, bytes: 1, sha256: hash(`receipt:${id}`), url: retained(`${id}.json`) },
      hosted: family.id === 'hosted' ? { runId: 1, jobId: 2, sourceCommit: integrationCommit, conclusion: 'success' } : null,
    };
  }));
  const record = { format: 'zryna.m6-evidence.v1', version: 1, registrySha256: hash(encode(registry)),
    integration: { repository: 'https://github.com/zryna/zryna', commit: integrationCommit, tree: integrationTree, clean: true },
    artifacts, gates, publication: { status: 'published-and-verified',
      toolkitSha256: hash('artifact:playground-toolkit'), docsManifestSha256: hash('artifact:docs-bundle'),
      verificationReceipt: { path: 'evidence/publication-linux.json', bytes: 1,
        sha256: hash('receipt:publication-linux'), url: retained('publication-linux.json') } } };
  const policy = { version: 1, integrationCommit, integrationTree, recordSha256: hash(encode(record)),
    artifactPins: Object.entries(artifactVersions).map(([role, version]) => selectedArtifact(role, version)) };
  return { record, policy };
}

export { fixture, integrationCommit, integrationTree, hash, encode, retained, products, artifactVersions, selectedGateArtifacts, gateMaterials, selectedFamilies };

// These are synthetic retained buffers, not compiler, browser or signed execution evidence.
function retainedFixture() {
  const { record } = fixture();
  const carriers = [];
  const values = [];
  for (const gate of record.gates) {
    gate.artifactRoles = gateMaterials(gate.id);
    const descriptor = (suffix, text) => {
      const path = `retained/${gate.id}/${suffix}`;
      const data = Buffer.from(text);
      carriers.push({ path, data });
      return { path, bytes: data.length, sha256: hash(data) };
    };
    const { receipt: unused, ...observation } = gate;
    const value = { format: 'zryna.m6-gate-receipt.v1', version: 1,
      integration: structuredClone(record.integration), observation: structuredClone(observation),
      materials: gate.artifactRoles.map(role => structuredClone(record.artifacts.find(row => row.role === role))),
      execution: { cwd: '.', profile: 'synthetic', corpusSha256: hash('synthetic-corpus'),
        inputs: [descriptor('input.txt', 'synthetic input\n')],
        stdout: descriptor('stdout.txt', 'synthetic output\n'), stderr: descriptor('stderr.txt', '') } };
    value.execution.corpusSha256 = value.execution.inputs[0].sha256;
    const data = encode(value);
    gate.receipt.bytes = data.length;
    gate.receipt.sha256 = hash(data);
    carriers.push({ path: gate.receipt.path, data });
    values.push(value);
  }
  record.publication.verificationReceipt = { ...record.gates.find(row => row.id === 'publication-linux').receipt };
  return { record, carriers, values };
}

export { retainedFixture };
