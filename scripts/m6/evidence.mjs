// Validates complete selected receipts; it does not execute gates or activate public support.
import { bytes, orderedPaths, portablePath, sha256 } from '../distribution/canonical.mjs';
import { decodeJson } from '../../examples/playground/restricted/json.mjs';
import { exact, fail, isHash } from '../../examples/playground/restricted/limits.mjs';
import { registry, requiredGates } from './registry.mjs';

const commit = value => typeof value === 'string' && /^[a-f0-9]{40}$/.test(value);
const name = value => typeof value === 'string' && /^[a-z0-9][a-z0-9-]{0,63}$/.test(value);
const version = value => typeof value === 'string' && /^(?:0|[1-9][0-9]*)(?:\.(?:0|[1-9][0-9]*)){2,3}(?:-[a-z0-9.-]+)?$/.test(value);
const text = value => typeof value === 'string' && value.length > 0 && value.length <= 1024;
export const registrySha256 = sha256(bytes(registry));

function durable(value) {
  if (!text(value)) fail('M6-RETENTION');
  let url;
  try { url = new URL(value); } catch { fail('M6-RETENTION'); }
  if (url.protocol !== 'https:' || url.username || url.password || url.search || url.hash ||
      url.hostname !== 'github.com' ||
      !/^\/zryna\/zryna\/releases\/download\/[A-Za-z0-9._-]+\/[A-Za-z0-9._-]+$/.test(url.pathname)) {
    fail('M6-RETENTION');
  }
}

function artifact(value) {
  exact(value, ['role', 'path', 'version', 'source', 'bytes', 'sha256', 'url']);
  portablePath(value.path);
  if (!name(value.role) || !version(value.version) || !Number.isSafeInteger(value.bytes) ||
      value.bytes < 1 || value.bytes > 2147483648 || !isHash(value.sha256)) fail('M6-ARTIFACT');
  exact(value.source, ['kind', 'revision', 'sha256']);
  if (!['git', 'signed-package', 'upstream-archive'].includes(value.source.kind) ||
      !text(value.source.revision) || !isHash(value.source.sha256) ||
      value.source.kind === 'git' && !/^[a-f0-9]{40}:[a-f0-9]{40}$/.test(value.source.revision)) fail('M6-SOURCE');
  durable(value.url);
}

export function validateM6Record(input, policy) {
  exact(policy, ['version', 'integrationCommit', 'integrationTree', 'recordSha256', 'artifactPins']);
  if (policy.version !== 1 || !commit(policy.integrationCommit) || !commit(policy.integrationTree) ||
      !isHash(policy.recordSha256) || !Array.isArray(policy.artifactPins) ||
      policy.artifactPins.length < registry.requiredArtifacts.length || policy.artifactPins.length > 128) fail('M6-POLICY');
  const document = decodeJson(input, 262144);
  if (sha256(input) !== policy.recordSha256 || !bytes(document).equals(Buffer.from(input))) fail('M6-RECORD');
  exact(document, ['format', 'version', 'registrySha256', 'integration', 'artifacts', 'gates', 'publication']);
  if (document.format !== 'zryna.m6-evidence.v1' || document.version !== 1 ||
      document.registrySha256 !== registrySha256) fail('M6-REGISTRY');
  exact(document.integration, ['repository', 'commit', 'tree', 'clean']);
  if (document.integration.repository !== 'https://github.com/zryna/zryna' ||
      document.integration.commit !== policy.integrationCommit || document.integration.tree !== policy.integrationTree ||
      document.integration.clean !== true) fail('M6-INTEGRATION');
  if (!Array.isArray(document.artifacts) || document.artifacts.length !== policy.artifactPins.length) fail('M6-ARTIFACTS');
  const artifacts = new Map();
  for (const value of document.artifacts) {
    artifact(value);
    if (artifacts.has(value.role)) fail('M6-ARTIFACTS');
    artifacts.set(value.role, value);
  }
  orderedPaths([...artifacts.values()].map(value => value.path).sort());
  if (registry.requiredArtifacts.some(role => !artifacts.has(role))) fail('M6-ARTIFACTS');
  const pins = new Set();
  for (const pin of policy.artifactPins) {
    artifact(pin);
    if (pins.has(pin.role) || !artifacts.has(pin.role) ||
        !bytes(artifacts.get(pin.role)).equals(bytes(pin))) fail('M6-ARTIFACT-PIN');
    pins.add(pin.role);
  }
  for (const [role, product] of [['compiler-release', 'compiler'], ['server-linux', 'server'],
    ['server-windows', 'server'], ['extension', 'extension'], ['setup-linux', 'setup'],
    ['setup-windows', 'setup'], ['playground-toolkit', 'playground'], ['playground-compiler', 'playground'],
    ['chrome-linux', 'chrome']]) {
    if (artifacts.get(role)?.version !== registry.products[product]) fail('M6-VERSION');
  }
  if (!Array.isArray(document.gates) || document.gates.length !== requiredGates.length) fail('M6-GATES');
  const seen = new Set();
  for (const gate of document.gates) {
    exact(gate, ['id', 'integrationCommit', 'artifactRoles', 'environment', 'command', 'startedAt',
      'finishedAt', 'exitCode', 'cases', 'receipt', 'hosted']);
    const family = registry.gates.find(row => row.platforms.some(platform => `${row.id}-${platform}` === gate.id));
    if (!family || seen.has(gate.id) || gate.integrationCommit !== policy.integrationCommit || gate.exitCode !== 0) fail('M6-GATE');
    seen.add(gate.id);
    exact(gate.environment, ['platform', 'osVersion', 'architecture', 'runtime']);
    if (!family.platforms.includes(gate.environment.platform) || gate.id !== `${family.id}-${gate.environment.platform}` ||
        !text(gate.environment.osVersion) || gate.environment.architecture !== 'x86_64' || !text(gate.environment.runtime)) fail('M6-ENVIRONMENT');
    if (!Array.isArray(gate.artifactRoles) || !gate.artifactRoles.length || gate.artifactRoles.length > 128 ||
        new Set(gate.artifactRoles).size !== gate.artifactRoles.length ||
        gate.artifactRoles.some(role => !artifacts.has(role)) || !Array.isArray(gate.command) || !gate.command.length ||
        gate.command.length > 128 || gate.command.some(argument => !text(argument))) fail('M6-EXECUTION');
    if (family.artifactRoles.some(role => !gate.artifactRoles.includes(
      role.replace('-platform', `-${gate.environment.platform}`)))) fail('M6-GATE-ARTIFACTS');
    const timestamp = value => typeof value === 'string' && /^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}\.\d{3}Z$/.test(value) &&
      Number.isFinite(Date.parse(value)) && new Date(value).toISOString() === value;
    if (!timestamp(gate.startedAt) || !timestamp(gate.finishedAt) || Date.parse(gate.finishedAt) < Date.parse(gate.startedAt)) fail('M6-TIME');
    if (!Array.isArray(gate.cases) || gate.cases.length !== family.cases.length) fail('M6-CASES');
    const cases = new Set();
    for (const result of gate.cases) {
      exact(result, ['id', 'passed', 'failed', 'skipped']);
      if (!family.cases.includes(result.id) || cases.has(result.id) || !Number.isSafeInteger(result.passed) ||
          result.passed < 1 || result.passed > 1000000 || result.failed !== 0 || result.skipped !== 0) fail('M6-CASES');
      cases.add(result.id);
    }
    exact(gate.receipt, ['path', 'bytes', 'sha256', 'url']);
    if (gate.receipt.path !== `evidence/${gate.id}.json` || !isHash(gate.receipt.sha256) ||
        !Number.isSafeInteger(gate.receipt.bytes) || gate.receipt.bytes < 1 || gate.receipt.bytes > 262144) fail('M6-RECEIPT');
    durable(gate.receipt.url);
    if (family.id === 'hosted') {
      exact(gate.hosted, ['runId', 'jobId', 'sourceCommit', 'conclusion']);
      if (![gate.hosted.runId, gate.hosted.jobId].every(value => Number.isSafeInteger(value) && value > 0) ||
          gate.hosted.sourceCommit !== policy.integrationCommit || gate.hosted.conclusion !== 'success') fail('M6-HOSTED');
    } else if (gate.hosted !== null) fail('M6-HOSTED');
  }
  exact(document.publication, ['status', 'toolkitSha256', 'docsManifestSha256', 'verificationReceipt']);
  exact(document.publication.verificationReceipt, ['path', 'bytes', 'sha256', 'url']);
  const verification = document.publication.verificationReceipt;
  const publicationGate = document.gates.find(gate => gate.id === 'publication-linux');
  if (document.publication.status !== 'published-and-verified' ||
      document.publication.toolkitSha256 !== artifacts.get('playground-toolkit').sha256 ||
      document.publication.docsManifestSha256 !== artifacts.get('docs-bundle').sha256 ||
      verification.path !== 'evidence/publication-linux.json' ||
      !bytes(verification).equals(bytes(publicationGate.receipt))) fail('M6-PUBLICATION');
  return document;
}
