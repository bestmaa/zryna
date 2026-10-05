// Authentication of detached evidence is separate from executable host admission.
import { bytes, sha256 } from '../distribution/canonical.mjs';
import { exact, fail } from '../../examples/playground/restricted/limits.mjs';
import { verifyToolkitSignature } from '../../examples/playground/restricted/signature.mjs';
import { toolkitBrowser, toolkitMaterials, toolkitNestedCompiler } from '../../examples/playground/restricted/toolkit.mjs';
import { registrySha256, validateM6Record } from './evidence.mjs';
import { validateRetainedReceipts } from './receipts.mjs';

const retained = new WeakMap();

export function authenticateM6Evidence(input) {
  exact(input, ['record', 'bundle', 'policy', 'verifierPolicy', 'trustedRoot', 'receipts', 'toolkit']);
  if (!(input.record instanceof Uint8Array) || input.record.length < 1 || input.record.length > 262144 ||
      !(input.bundle instanceof Uint8Array) || input.bundle.length < 1 || input.bundle.length > 1048576 ||
      !(input.trustedRoot instanceof Uint8Array) || input.trustedRoot.length < 1 ||
      input.trustedRoot.length > 262144) fail('M6-CLOSURE-BYTES');
  // The toolkit must be an actual retained signature/archive capability, never a metadata object.
  toolkitMaterials(input.toolkit);
  const nested = toolkitNestedCompiler(input.toolkit);
  const browser = toolkitBrowser(input.toolkit);
  const policy = structuredClone(input.policy), verifierPolicy = structuredClone(input.verifierPolicy);
  const recordBytes = Buffer.from(input.record), bundle = Buffer.from(input.bundle);
  const root = Buffer.from(input.trustedRoot);
  const record = validateM6Record(recordBytes, policy);
  validateToolkitComposition(record, input.toolkit, nested, browser);
  const receipts = validateRetainedReceipts(record, input.receipts);
  verifyToolkitSignature(recordBytes, bundle, verifierPolicy, root, policy.integrationCommit);
  const capability = Object.freeze({ recordSha256: sha256(recordBytes),
    integrationCommit: policy.integrationCommit, integrationTree: policy.integrationTree });
  retained.set(capability, { record, receipts });
  return capability;
}

// Pure relationship validation cannot create either an evidence or an executable capability.
export function validateToolkitComposition(record, toolkit, nested, browser) {
  const artifact = role => record.artifacts.find(value => value.role === role);
  if (record.integration.commit !== toolkit.sourceCommit || record.integration.tree !== toolkit.sourceTree ||
      artifact('playground-toolkit').sha256 !== toolkit.authority.toolkitSha256 ||
      artifact('playground-compiler').sha256 !== toolkit.authority.compilerSha256 ||
      artifact('compiler-release').sha256 !== nested.archiveSha256 ||
      artifact('chrome-linux').sha256 !== browser.archiveSha256 ||
      artifact('host-policy').sha256 !== toolkit.authority.policySha256) fail('M6-TOOLKIT-BINDING');
  for (const [role, commit, tree] of [['compiler-release', nested.sourceCommit, nested.sourceTree],
    ['playground-toolkit', toolkit.sourceCommit, toolkit.sourceTree],
    ['playground-compiler', toolkit.sourceCommit, toolkit.sourceTree]]) {
    const source = artifact(role).source;
    if (source.kind !== 'git' || source.revision !== `${commit}:${tree}`) fail('M6-TOOLKIT-SOURCE');
  }
}

export function m6Evidence(capability) {
  const state = retained.get(capability);
  if (!state) fail('M6-CLOSURE-CAPABILITY');
  // A detached signed observation is not an executable/browser/process-tree capability.
  return structuredClone(state);
}

export function assembleM6Record({ integration, artifacts, gates, publication, receipts }, policy) {
  const record = { format: 'zryna.m6-evidence.v1', version: 1,
    registrySha256, integration, artifacts, gates, publication };
  const result = bytes(record);
  validateM6Record(result, policy);
  validateRetainedReceipts(record, receipts);
  return result;
}
