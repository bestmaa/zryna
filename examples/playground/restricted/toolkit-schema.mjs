// Publication metadata is checked against a separately reviewed policy before authentication.
import { bytes, orderedPaths, portablePath, sha256 } from '../../../scripts/distribution/canonical.mjs';
import { decodeJson } from './json.mjs';
import { exact, fail, isHash } from './limits.mjs';

export const toolkitRoot = 'zryna-playground-0.1.0-x86_64-unknown-linux-gnu';
export const toolkitIdentity = 'https://github.com/zryna/zryna/.github/workflows/playground-release.yml@refs/tags/playground-v0.1.0';
export const toolkitIssuer = 'https://token.actions.githubusercontent.com';
export const toolkitRoles = Object.freeze({ compiler: 'bin/zryna-playground-compiler',
  policy: 'policy.json', bindingTemplate: 'resources/loader.js', wit: 'resources/browser.wit',
  witClosure: 'resources/wit-sources.json',
  compilerEnvelope: 'releases/compiler-envelope.json', compilerBundle: 'releases/compiler-envelope.sigstore.json',
  compilerArchive: 'releases/zryna-0.2.3-x86_64-unknown-linux-gnu.tar.gz',
  browserArchive: 'releases/chrome-linux64.zip', browserInventory: 'releases/chrome-linux64.inventory.json' });
const browserArchive = '8aac35011c18f6e2d10696154af89a5728ac2ddd6dc6fad24ffdf243c3fcfd5a';
const browserInventory = '110d5111c1a885596b2dba4fe5d04e570d26e1c57454d0a902f1115ce818635a';
const commit = value => typeof value === 'string' && /^[a-f0-9]{40}$/.test(value);

function descriptor(value) {
  exact(value, ['path', 'bytes', 'sha256', 'mode']);
  portablePath(value.path);
  if (!Number.isSafeInteger(value.bytes) || value.bytes < 1 || value.bytes > 268435456 ||
      !isHash(value.sha256) || ![0o644, 0o755].includes(value.mode)) fail('TOOLKIT-INVENTORY');
}

function source(value, policy) {
  exact(value, ['repository', 'ref', 'commit', 'tree', 'sourceDateEpoch']);
  if (value.repository !== 'https://github.com/zryna/zryna' ||
      value.ref !== 'refs/tags/playground-v0.1.0' || !commit(value.commit) || !commit(value.tree) ||
      value.commit !== policy.sourceCommit || value.tree !== policy.sourceTree ||
      !Number.isSafeInteger(value.sourceDateEpoch) || value.sourceDateEpoch < 0 ||
      value.sourceDateEpoch > 0xffffffff) fail('TOOLKIT-SOURCE');
}

export function validateToolkitEnvelope(input, policy) {
  exact(policy, ['version', 'sourceCommit', 'sourceTree', 'envelopeSha256', 'archiveSha256',
    'nestedCompiler', 'requiredGates']);
  if (policy.version !== 1 || !commit(policy.sourceCommit) || !commit(policy.sourceTree) ||
      !isHash(policy.envelopeSha256) || !isHash(policy.archiveSha256)) fail('TOOLKIT-POLICY');
  const document = decodeJson(input, 262144);
  if (sha256(input) !== policy.envelopeSha256 || !bytes(document).equals(Buffer.from(input))) {
    fail('TOOLKIT-ENVELOPE');
  }
  exact(document, ['format', 'version', 'productVersion', 'target', 'source', 'signing',
    'archive', 'nestedCompiler', 'browser', 'files', 'materials', 'gates']);
  if (document.format !== 'zryna.playground-toolkit.v1' || document.version !== 1 ||
      document.productVersion !== '0.1.0' || document.target !== 'x86_64-unknown-linux-gnu') {
    fail('TOOLKIT-VERSION');
  }
  source(document.source, policy);
  exact(document.signing, ['issuer', 'certificateIdentity']);
  if (document.signing.issuer !== toolkitIssuer || document.signing.certificateIdentity !== toolkitIdentity) {
    fail('TOOLKIT-SIGNER');
  }
  exact(document.archive, ['path', 'bytes', 'sha256']);
  if (document.archive.path !== toolkitRoot + '.tar.gz' || !isHash(document.archive.sha256) ||
      document.archive.sha256 !== policy.archiveSha256 || !Number.isSafeInteger(document.archive.bytes) ||
      document.archive.bytes < 1 || document.archive.bytes > 537919488) fail('TOOLKIT-ARCHIVE');
  for (const value of [document.nestedCompiler, policy.nestedCompiler]) {
    exact(value, ['version', 'envelopeSha256', 'archiveSha256', 'sourceCommit']);
    if (value.version !== '0.2.3' || !isHash(value.envelopeSha256) ||
        !isHash(value.archiveSha256) || !commit(value.sourceCommit)) fail('TOOLKIT-NESTED-COMPILER');
  }
  if (!bytes(document.nestedCompiler).equals(bytes(policy.nestedCompiler))) fail('TOOLKIT-NESTED-COMPILER');
  exact(document.browser, ['version', 'archiveSha256', 'inventorySha256']);
  if (document.browser.version !== '153.0.8010.12' || document.browser.archiveSha256 !== browserArchive ||
      document.browser.inventorySha256 !== browserInventory) fail('TOOLKIT-BROWSER');
  if (!Array.isArray(document.files) || !document.files.length || document.files.length > 128) {
    fail('TOOLKIT-INVENTORY');
  }
  document.files.forEach(descriptor);
  orderedPaths(document.files.map(file => file.path));
  const files = new Map(document.files.map(file => [file.path, file]));
  if (document.files.reduce((sum, file) => sum + file.bytes, 0) > 536870912 ||
      Object.values(toolkitRoles).some(path => !files.has(path)) ||
      files.get(toolkitRoles.compiler).mode !== 0o755) fail('TOOLKIT-INVENTORY');
  if (files.get(toolkitRoles.wit).mode !== 0o644 || document.files.some(file =>
    file.path.startsWith('resources/wit/') && (file.mode !== 0o644 || file.bytes > 32768))) {
    fail('TOOLKIT-WIT');
  }
  for (const [path, digest] of [[toolkitRoles.compilerEnvelope, document.nestedCompiler.envelopeSha256],
    [toolkitRoles.compilerArchive, document.nestedCompiler.archiveSha256],
    [toolkitRoles.browserArchive, document.browser.archiveSha256],
    [toolkitRoles.browserInventory, document.browser.inventorySha256]]) {
    if (files.get(path).sha256 !== digest || files.get(path).mode !== 0o644) fail('TOOLKIT-CAPTURED-PIN');
  }
  for (const [path, maximum] of [[toolkitRoles.compilerEnvelope, 262144],
    [toolkitRoles.compilerBundle, 1048576], [toolkitRoles.browserInventory, 8388608],
    [toolkitRoles.witClosure, 16384]]) {
    if (files.get(path).bytes > maximum || files.get(path).mode !== 0o644) fail('TOOLKIT-CAPTURED-PIN');
  }
  if (!Array.isArray(document.materials) || !document.materials.length || document.materials.length > 128) {
    fail('TOOLKIT-MATERIALS');
  }
  const mounts = new Set();
  for (const item of document.materials) {
    exact(item, ['path', 'mount', 'bytes', 'sha256', 'executable']);
    const file = files.get(item.path);
    if (!file || file.bytes !== item.bytes || file.sha256 !== item.sha256 ||
        typeof item.executable !== 'boolean' || item.executable !== (file.mode === 0o755) ||
        typeof item.mount !== 'string' || !item.mount.startsWith('/') || mounts.has(item.mount)) {
      fail('TOOLKIT-MATERIALS');
    }
    portablePath(item.mount.slice(1));
    if (!(item.mount === '/app/compiler' || item.mount.startsWith('/materials/') ||
        item.mount.startsWith('/lib/x86_64-linux-gnu/') || item.mount.startsWith('/lib64/')) ||
        item.mount === '/app/compiler' && item.path !== toolkitRoles.compiler ||
        item.mount === '/materials/runtime/node/bin/node' && !item.executable) fail('TOOLKIT-MATERIALS');
    mounts.add(item.mount);
  }
  orderedPaths([...mounts].map(mount => mount.slice(1)).sort());
  if (!mounts.has('/app/compiler') || !mounts.has('/materials/runtime/node/bin/node')) fail('TOOLKIT-MATERIALS');
  if (!Array.isArray(policy.requiredGates) || !policy.requiredGates.length || policy.requiredGates.length > 128 ||
      policy.requiredGates.some(name => typeof name !== 'string' || !/^[a-z0-9][a-z0-9-]{0,63}$/.test(name)) ||
      new Set(policy.requiredGates).size !== policy.requiredGates.length ||
      !Array.isArray(document.gates) || document.gates.length !== policy.requiredGates.length) fail('TOOLKIT-GATES');
  const gates = new Set();
  for (const gate of document.gates) {
    exact(gate, ['name', 'sourceCommit', 'status', 'receiptPath', 'receiptSha256']);
    if (!policy.requiredGates.includes(gate.name) || gates.has(gate.name) ||
        gate.sourceCommit !== policy.sourceCommit || gate.status !== 'passed' || !isHash(gate.receiptSha256) ||
        gate.receiptPath !== `evidence/${gate.name}.json` ||
        files.get(gate.receiptPath)?.sha256 !== gate.receiptSha256) {
      fail('TOOLKIT-GATES');
    }
    gates.add(gate.name);
  }
  return document;
}
