// No executable capability is issued until independent signature and complete archive checks pass.
import { decodeTar } from '../../../scripts/distribution/archive-tar.mjs';
import { sha256 } from '../../../scripts/distribution/canonical.mjs';
import { exact, fail } from './limits.mjs';
import { verifyToolkitSignature } from './signature.mjs';
import { toolkitRoles, toolkitRoot, validateToolkitEnvelope } from './toolkit-schema.mjs';
import { authenticateNestedCompiler } from './nested-compiler.mjs';
import { verifyBrowserMembers } from './browser-archive.mjs';
import { verifyWitSources } from './wit-closure.mjs';

const retained = new WeakMap();

export async function authenticateToolkit(input) {
  exact(input, ['envelope', 'bundle', 'archive', 'policy', 'verifierPolicy', 'trustedRoot']);
  if (![input.envelope, input.bundle, input.archive, input.trustedRoot]
    .every(value => value instanceof Uint8Array) || input.envelope.length < 1 || input.envelope.length > 262144 ||
      input.bundle.length < 1 || input.bundle.length > 1048576 || input.trustedRoot.length < 1 ||
      input.trustedRoot.length > 262144 || input.archive.length < 1 || input.archive.length > 537919488) {
    fail('TOOLKIT-BYTES');
  }
  // Snapshot caller-owned carriers before asynchronous archive processing.
  const envelope = Buffer.from(input.envelope), bundle = Buffer.from(input.bundle);
  const trustedRoot = Buffer.from(input.trustedRoot);
  const policy = structuredClone(input.policy), verifierPolicy = structuredClone(input.verifierPolicy);
  const document = validateToolkitEnvelope(envelope, policy);
  if (input.archive.length !== document.archive.bytes) fail('TOOLKIT-ARCHIVE');
  const archive = Buffer.from(input.archive);
  if (sha256(archive) !== document.archive.sha256) fail('TOOLKIT-ARCHIVE');
  verifyToolkitSignature(envelope, bundle, verifierPolicy, trustedRoot, policy.sourceCommit);
  const files = await decodeTar(archive, toolkitRoot, document.source.sourceDateEpoch);
  if (files.length !== document.files.length) fail('TOOLKIT-INVENTORY');
  const captured = new Map();
  for (let index = 0; index < files.length; index++) {
    const file = files[index], expected = document.files[index];
    if (file.path !== expected.path || file.mode !== expected.mode ||
        file.data.length !== expected.bytes || sha256(file.data) !== expected.sha256) fail('TOOLKIT-INVENTORY');
    captured.set(file.path, Buffer.from(file.data));
  }
  const nested = await authenticateNestedCompiler(captured, policy.nestedCompiler, document.materials, verifierPolicy, trustedRoot);
  verifyBrowserMembers(captured.get(toolkitRoles.browserArchive), captured.get(toolkitRoles.browserInventory));
  const witSha256 = verifyWitSources(captured);
  const descriptor = path => document.files.find(file => file.path === path);
  const authority = Object.freeze({ compilerSha256: descriptor(toolkitRoles.compiler).sha256,
    toolkitSha256: document.archive.sha256, policySha256: descriptor(toolkitRoles.policy).sha256,
    bindingTemplateSha256: descriptor(toolkitRoles.bindingTemplate).sha256 });
  const capability = Object.freeze({ authority, witSha256,
    sourceCommit: document.source.commit, sourceTree: document.source.tree });
  retained.set(capability, { captured, document, nested });
  return capability;
}

export function toolkitFile(capability, path) {
  const state = retained.get(capability);
  if (!state || !state.captured.has(path)) fail('TOOLKIT-CAPABILITY');
  return Buffer.from(state.captured.get(path));
}

export function toolkitMaterials(capability) {
  const state = retained.get(capability);
  if (!state) fail('TOOLKIT-CAPABILITY');
  return structuredClone(state.document.materials);
}

export function toolkitNestedCompiler(capability) {
  const state = retained.get(capability);
  if (!state) fail('TOOLKIT-CAPABILITY');
  return { ...structuredClone(state.document.nestedCompiler), sourceTree: state.nested.sourceTree };
}

export function toolkitBrowser(capability) {
  const state = retained.get(capability);
  if (!state) fail('TOOLKIT-CAPABILITY');
  return structuredClone(state.document.browser);
}
