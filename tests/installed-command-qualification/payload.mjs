// A strict test-fixture builder, kept outside production assembly and release admission.
import assert from 'node:assert/strict';
import { bytes, parseCanonical, sha256 } from '../../scripts/distribution/canonical.mjs';
import { installationDocuments } from '../../scripts/distribution/documents.mjs';
import { authenticateFiles, roleFor } from '../../scripts/distribution/inventory.mjs';
import { validateDistribution } from '../../scripts/distribution/prepare.mjs';
import { validateSourceReceipt } from '../../scripts/distribution/source-receipt.mjs';
import { rustMaterials } from '../../scripts/distribution/rust-materials.mjs';

const CANDIDATE_LOCK = '890d9b74f45a5af4d906cd00d90eb7ba2063662c7e9613593e2c0bdcb19e26b6';
export function fixturePayload(identity, captures, architecture, metadata) {
  const receipt = validateSourceReceipt(architecture, identity);
  assert.equal(receipt.inputs.find(input => input.logicalPath === 'Cargo.lock').sha256, CANDIDATE_LOCK);
  const records = rustMaterials(identity.target.triple);
  const packages = new Map(metadata.packages.map(pkg => [pkg.id, pkg]));
  const nodes = new Map(metadata.resolve.nodes.map(node => [node.id, node]));
  const pending = metadata.packages.filter(pkg => pkg.name === 'zryna'
    && metadata.workspace_members.includes(pkg.id)).map(pkg => pkg.id);
  assert.equal(pending.length, 1);
  const selected = new Set();
  while (pending.length) {
    const id = pending.pop();
    if (selected.has(id)) continue;
    selected.add(id);
    for (const dependency of nodes.get(id).deps) {
      if (dependency.dep_kinds.some(kind => kind.kind === null || kind.kind === 'build')) pending.push(dependency.pkg);
    }
  }
  const closure = [...selected].map(id => packages.get(id)).filter(pkg => pkg.source)
    .map(pkg => `${pkg.name}-${pkg.version}`).sort();
  assert.deepEqual(closure, records.map(pkg => `${pkg.name}-${pkg.version}`).sort(), 'pinned runtime Rust closure');
  const payload = [...captures, ...installationDocuments(identity.version, identity.target.triple)]
    .sort((a, b) => a.path < b.path ? -1 : 1);
  const tuple = file => {
    const role = roleFor(file.path, identity.target.triple);
    const material = role === 'runtime' || file.path === 'licenses/node-LICENSE' ? 'node-22.22.1'
      : file.path.includes('/@typescript/typescript6/') || file.path === 'licenses/typescript6-LICENSE.txt'
        ? 'typescript6-6.0.2'
        : file.path.includes('/@typescript/old/') || file.path.startsWith('licenses/typescript-')
          ? 'typescript-6.0.3' : 'source';
    const licenses = role === 'license' ? [file.path] : material === 'node-22.22.1'
      ? ['licenses/node-LICENSE'] : material === 'typescript6-6.0.2'
        ? ['licenses/typescript6-LICENSE.txt'] : material === 'typescript-6.0.3'
          ? ['licenses/typescript-LICENSE.txt', 'licenses/typescript-ThirdPartyNoticeText.txt'] : ['LICENSE'];
    return { path: file.path, size: file.data.length, sha256: sha256(file.data), role,
      mode: file.mode, material, licenses };
  };
  const materialFiles = payload.map(tuple);
  payload.push({ path: 'metadata/architecture-receipt.json', mode: 0o644, data: architecture },
    { path: 'metadata/materials.json', mode: 0o644,
      data: bytes({ format: 'zryna.distribution-materials.v1', files: materialFiles }) });
  payload.sort((a, b) => a.path < b.path ? -1 : 1);
  const files = payload.map(tuple);
  const expectedNotices = records.flatMap(record => record.files)
    .map(({ path, size, sha256 }) => ({ path, size, sha256 })).sort((a, b) => a.path < b.path ? -1 : 1);
  assert.deepEqual(files.filter(file => file.path.startsWith('licenses/rust/'))
    .map(({ path, size, sha256 }) => ({ path, size, sha256 })), expectedNotices);
  const record = { format: 'zryna.distribution.v1', ...identity, files };
  validateDistribution(record, { productionCandidate: true });
  authenticateFiles(payload, files, identity.target.triple);
  const distribution = bytes(record);
  assert.deepEqual(parseCanonical(distribution), record);
  return { record, distribution, payload, runtimeClosure: closure,
    qualificationLockSha256: CANDIDATE_LOCK };
}
