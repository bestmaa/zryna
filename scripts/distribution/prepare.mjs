import { LIMITS, bytes, exactKeys, parseCanonical, requireValue, sha256 } from './canonical.mjs';
import { authenticateFiles, digestValue, targetPaths, validateTuples } from './inventory.mjs';
import { validateBootstrapPins } from './materials.mjs';
import { validateMaterials } from './material-record.mjs';
import { validateSourceReceipt } from './source-receipt.mjs';
import { validateRustMaterials } from './rust-materials.mjs';

const VERSION = '0.2.3';
const RELEASE_VERSIONS = new Set(['0.2.1', '0.2.2', VERSION]);

export function validateDistribution(record, {
  acceptedVersion = VERSION, productionCandidate = false,
} = {}) {
  exactKeys(record, ['format', 'version', 'source', 'target', 'recipe', 'files']);
  requireValue(record.format === 'zryna.distribution.v1', 'distribution format');
  requireValue(RELEASE_VERSIONS.has(acceptedVersion)
    && (!productionCandidate || acceptedVersion === VERSION)
    && record.version === acceptedVersion, 'unapproved distribution version');
  exactKeys(record.source, ['repository', 'ref', 'commit', 'tree', 'sourceDateEpoch']);
  requireValue(record.source.repository === 'https://github.com/zryna/zryna'
    && record.source.ref === (productionCandidate ? 'refs/heads/main' : `refs/tags/v${record.version}`)
    && /^[0-9a-f]{40}$/.test(record.source.commit)
    && /^[0-9a-f]{40}$/.test(record.source.tree), 'source identity');
  requireValue(Number.isSafeInteger(record.source.sourceDateEpoch)
    && record.source.sourceDateEpoch >= 0 && record.source.sourceDateEpoch <= 0xffffffff, 'source epoch');
  exactKeys(record.target, ['triple', 'archiveFormat', 'platformBaseline']);
  const paths = targetPaths(record.target.triple);
  requireValue(record.target.archiveFormat === paths.format, 'target archive format');
  const baseline = record.target.triple === 'x86_64-unknown-linux-gnu'
    ? { os: 'linux', distribution: 'ubuntu', version: '24.04', architecture: 'x86_64' }
    : { os: 'windows', product: 'windows-server', version: '2022', architecture: 'x86_64',
      runtime: 'operating-system-ucrt' };
  requireValue(bytes(record.target.platformBaseline).equals(bytes(baseline)), 'platform baseline');
  exactKeys(record.recipe, ['format', 'sha256']);
  requireValue(record.recipe.format === 'zryna.distribution-recipe.v1', 'recipe format');
  digestValue(record.recipe.sha256);
  requireValue(Array.isArray(record.files), 'distribution payload');
  requireValue(record.files.length <= LIMITS.files - 4, 'complete archive file count budget');
  validateTuples(record.files, record.target.triple, { complete: true, sourceRef: record.source.ref });
  validateBootstrapPins(record.files, record.target.triple);
  requireValue(record.files.every(file => ![paths.cli, 'metadata/distribution.json',
    'metadata/inventory.json', 'metadata/checksums.sha256'].includes(file.path)), 'cyclic payload');
}

// The workflow supplies captured, authenticated source/material bytes. This pure operation
// performs no network acquisition, command execution, path resolution or signing.
export function prepare({ version, source, target, recipe, files }, capturedFiles, options) {
  const record = { format: 'zryna.distribution.v1', version, source, target, recipe, files };
  validateDistribution(record, options);
  authenticateFiles(capturedFiles, files, target.triple);
  for (const path of ['metadata/materials.json', 'metadata/architecture-receipt.json']) {
    parseCanonical(capturedFiles.find(file => file.path === path).data);
  }
  const materialBytes = capturedFiles.find(file => file.path === 'metadata/materials.json').data;
  const materials = validateMaterials(materialBytes, record);
  const sourceReceipt = validateSourceReceipt(
    capturedFiles.find(file => file.path === 'metadata/architecture-receipt.json').data, record,
  );
  validateRustMaterials(files, target.triple, sourceReceipt, record.version);
  requireValue(capturedFiles.find(file => file.path === 'VERSION').data.equals(Buffer.from(`${version}\n`)),
    'VERSION does not match distribution');
  const distribution = bytes(record);
  requireValue(distribution.length <= LIMITS.record, 'prepared distribution byte budget');
  return {
    record,
    distribution,
    preparedDistribution: {
      format: record.format,
      logicalPath: 'prepared/metadata/distribution.json',
      size: distribution.length,
      sha256: sha256(distribution),
      archiveFileCount: files.length + 4,
    },
    materials: {
      format: materials.format,
      logicalPath: 'prepared/metadata/materials.json',
      size: materialBytes.length,
      sha256: sha256(materialBytes),
      fileCount: materials.files.length,
    },
  };
}
