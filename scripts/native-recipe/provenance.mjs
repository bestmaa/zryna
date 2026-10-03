import { createHash } from 'node:crypto';

import { reject, reviewedDocument } from './identity.mjs';

function exactKeys(value, keys) {
  return value !== null && typeof value === 'object' && !Array.isArray(value) &&
    Object.keys(value).sort().join('\0') === [...keys].sort().join('\0');
}

// An internal comparison only; the reviewed #168 evidence extension does not exist yet.
export function preflightNativeProvenance(review, candidate, outputMaterials) {
  reviewedDocument(review);
  if (!exactKeys(candidate, ['recipeSha256', 'planKey', 'mode', 'outputs']) ||
      candidate.recipeSha256 !== review.recipeSha256 ||
      candidate.planKey !== review.bindings.planKey || candidate.mode !== 'trusted-native' ||
      !Array.isArray(candidate.outputs) || candidate.outputs.length !== review.bindings.outputAllowlist.length ||
      !(outputMaterials instanceof Map) || outputMaterials.size !== candidate.outputs.length) {
    reject('TRUST-PROVENANCE', 'provenance does not bind the exact recipe, plan, mode and output inventory');
  }
  for (const [index, output] of candidate.outputs.entries()) {
    const expected = review.bindings.outputAllowlist[index];
    if (!exactKeys(output, ['target', 'path', 'size', 'sha256']) ||
        output.target !== expected.target || output.path !== expected.path ||
        !Number.isSafeInteger(output.size) || output.size < 0 || output.size > 1073741824 ||
        typeof output.sha256 !== 'string' || !/^[0-9a-f]{64}$/.test(output.sha256)) {
      reject('TRUST-PROVENANCE', 'output metadata differs from the declared target-qualified allowlist');
    }
    const bytes = outputMaterials.get(output.path);
    if (!Buffer.isBuffer(bytes) || bytes.length !== output.size ||
        createHash('sha256').update(bytes).digest('hex') !== output.sha256) {
      reject('TRUST-PROVENANCE', 'output bytes differ from their declared inventory');
    }
  }
  // Matching hashes do not authenticate a builder, actual input use, enforced isolation,
  // complete object/ELF audits, or create-only publication. No observed-policy field is invented.
  reject('TRUST-PROVENANCE', 'authenticated builder, execution, isolation and output-audit evidence is unavailable');
}
