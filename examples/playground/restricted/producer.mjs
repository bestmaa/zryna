// Deterministic unsigned toolkit assembly. Publication and authentication are separate operations.
import { encodeTar } from '../../../scripts/distribution/archive-tar.mjs';
import { requireArchiveRuntime } from '../../../scripts/distribution/runtime.mjs';
import { bytes, orderedPaths, sha256 } from '../../../scripts/distribution/canonical.mjs';
import { exact, fail } from './limits.mjs';
import { toolkitIdentity, toolkitIssuer, toolkitRoot, validateToolkitEnvelope } from './toolkit-schema.mjs';
import { verifyWitSources } from './wit-closure.mjs';

export async function assembleToolkit(input, reviewed) {
  exact(input, ['source', 'files', 'materials', 'nestedCompiler', 'gates']);
  exact(reviewed, ['sourceCommit', 'sourceTree', 'nestedCompiler', 'requiredGates']);
  requireArchiveRuntime();
  if (!Array.isArray(input.files) || input.files.length < 1 || input.files.length > 128) fail('TOOLKIT-INVENTORY');
  // Check every carrier and the aggregate bound before retaining copies or compressing.
  let total = 0;
  for (const file of input.files) {
    exact(file, ['path', 'mode', 'data']);
    if (!(file.data instanceof Uint8Array) || file.data.length < 1 || file.data.length > 268435456 ||
        ![0o644, 0o755].includes(file.mode) || (total += file.data.length) > 536870912) fail('TOOLKIT-INVENTORY');
  }
  orderedPaths(input.files.map(file => file.path));
  const files = input.files.map(file => ({ path: file.path, mode: file.mode, data: Buffer.from(file.data) }));
  const document = { format: 'zryna.playground-toolkit.v1', version: 1, productVersion: '0.1.0',
    target: 'x86_64-unknown-linux-gnu', source: structuredClone(input.source),
    signing: { issuer: toolkitIssuer, certificateIdentity: toolkitIdentity },
    archive: { path: `${toolkitRoot}.tar.gz`, bytes: 1, sha256: '0'.repeat(64) },
    nestedCompiler: structuredClone(input.nestedCompiler),
    browser: { version: '153.0.8010.12',
      archiveSha256: '8aac35011c18f6e2d10696154af89a5728ac2ddd6dc6fad24ffdf243c3fcfd5a',
      inventorySha256: '110d5111c1a885596b2dba4fe5d04e570d26e1c57454d0a902f1115ce818635a' },
    files: files.map(file => ({ path: file.path, mode: file.mode, bytes: file.data.length, sha256: sha256(file.data) })),
    materials: structuredClone(input.materials), gates: structuredClone(input.gates) };
  const policy = { version: 1, ...structuredClone(reviewed), envelopeSha256: sha256(bytes(document)),
    archiveSha256: document.archive.sha256 };
  // Fail metadata/source/receipt/material admission before expensive archive construction.
  validateToolkitEnvelope(bytes(document), policy);
  verifyWitSources(new Map(files.map(file => [file.path, file.data])));
  const archive = await encodeTar(toolkitRoot, files, document.source.sourceDateEpoch);
  document.archive.bytes = archive.length;
  document.archive.sha256 = sha256(archive);
  const envelope = bytes(document);
  policy.archiveSha256 = document.archive.sha256;
  policy.envelopeSha256 = sha256(envelope);
  validateToolkitEnvelope(envelope, policy);
  // The resulting digest selection is a review candidate, never an independent trust policy.
  return { archive, envelope, candidatePolicy: policy };
}
