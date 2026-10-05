// Reuses the released distribution verifier after separate nested signature/source authentication.
import { verifyArchive } from '../../../scripts/distribution/verify.mjs';
import { PROVIDERS } from '../../../scripts/distribution/inventory.mjs';
import { sha256 } from '../../../scripts/distribution/canonical.mjs';
import { fail } from './limits.mjs';
import { verifyCompilerSignature } from './signature.mjs';
import { toolkitRoles } from './toolkit-schema.mjs';

export async function authenticateNestedCompiler(captured, selected, materials, verifierPolicy, trustedRoot) {
  const envelope = captured.get(toolkitRoles.compilerEnvelope), bundle = captured.get(toolkitRoles.compilerBundle);
  const archive = captured.get(toolkitRoles.compilerArchive);
  if (!envelope || !bundle || !archive || envelope.length > 262144 || bundle.length > 1048576 ||
      sha256(envelope) !== selected.envelopeSha256 || sha256(archive) !== selected.archiveSha256) fail('NESTED-COMPILER');
  const { validateReleaseText } = await import('../../../scripts/distribution-release/validate.mjs');
  const document = validateReleaseText(new TextDecoder('utf-8', { fatal: true }).decode(envelope));
  if (document.version !== '0.2.3' || document.source.commit !== selected.sourceCommit ||
      document.workflow.path !== '.github/workflows/release.yml') fail('NESTED-COMPILER-SOURCE');
  const subject = document.subjects.find(value => value.target === 'x86_64-unknown-linux-gnu');
  if (!subject || subject.archive.sha256 !== selected.archiveSha256 || subject.archive.size !== archive.length) fail('NESTED-COMPILER-ARCHIVE');
  verifyCompilerSignature(envelope, bundle, verifierPolicy, trustedRoot, selected.sourceCommit);
  const verified = await verifyArchive(archive, { filename: subject.archive.path, size: subject.archive.size,
    sha256: subject.archive.sha256, version: document.version, source: document.source,
    target: { triple: subject.target, archiveFormat: 'tar-gzip', platformBaseline: subject.platformBaseline },
    recipe: document.recipe });
  const released = new Map(verified.files.map(file => [file.path, file]));
  const required = new Set(['runtime/node/bin/node', ...PROVIDERS]);
  const mounted = new Set();
  for (const material of materials.filter(value => value.mount.startsWith('/materials/'))) {
    const path = material.mount.slice('/materials/'.length), file = released.get(path);
    if (!required.has(path) || !file || file.data.length !== material.bytes || sha256(file.data) !== material.sha256 ||
        material.executable !== (file.mode === 0o755) || !captured.get(material.path)?.equals(file.data)) fail('NESTED-COMPILER-MATERIAL');
    mounted.add(path);
  }
  if ([...required].some(path => !mounted.has(path))) fail('NESTED-COMPILER-MATERIAL');
  return Object.freeze({ archiveSha256: verified.archiveSha256,
    sourceCommit: document.source.commit, sourceTree: document.source.tree });
}
