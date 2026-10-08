import { canonicalBounded, sha256 } from '../../distribution-release/canonical.mjs';
import { validateSourceBuildReceiptText } from '../../distribution-release/validate-source-build-receipt.mjs';

function reject(message) {
  throw new Error(`D422-PROVISION: ${message}`);
}

export function productionArchitecture(bytes, source) {
  if (!Buffer.isBuffer(bytes)) reject('source architecture receipt bytes are missing');
  const receipt = validateSourceBuildReceiptText(bytes.toString('utf8'));
  if (receipt.source.repository !== source.repository || receipt.source.commit !== source.commit
    || receipt.source.tree !== source.tree) reject('source architecture identity differs');
  const observation = {
    command: receipt.command,
    inputs: receipt.inputs,
    report: receipt.report,
    toolchain: receipt.toolchain,
  };
  return {
    receipt,
    qualificationBytes: Buffer.from(`${canonicalBounded({
      format: 'zryna.release-qualification-architecture.v1',
      status: 'provisional-candidate',
      productionAdmission: 'forbidden',
      source: { ...receipt.source, ref: 'refs/heads/main' },
      ...observation,
    })}\n`),
  };
}

export function toolchains(observed, architectureBytes) {
  const evidence = sha256(architectureBytes);
  return observed.toolchains.map((record) => ({
    name: record.name,
    version: record.version,
    origin: record.origin,
    sha256: record.sha256,
    signatureEvidenceSha256: record.name === 'node'
      ? record.observationEvidenceSha256 : evidence,
  }));
}

