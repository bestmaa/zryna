import { createHash } from 'node:crypto';

import { verifySourceMaterials } from '../build-plan/validate.mjs';
import { reject, reviewedDocument } from './identity.mjs';

function exactMaterials(materials, inventory, code) {
  if (!(materials instanceof Map) || materials.size !== inventory.length ||
      inventory.some((item) => !materials.has(item.key))) {
    reject(code, 'material inventory has missing or extra identities');
  }
  for (const item of inventory) {
    const bytes = materials.get(item.key);
    if (!Buffer.isBuffer(bytes) || bytes.length > 1073741824 ||
        (item.size !== undefined && bytes.length !== item.size) ||
        createHash('sha256').update(bytes).digest('hex') !== item.sha256) {
      reject(code, 'material bytes differ from the independently retained plan');
    }
  }
}

export function verifyRecipeSources(review, sourceMaterials) {
  verifySourceMaterials(reviewedDocument(review), sourceMaterials);
}

// No host paths are accepted. This checks bytes; opening/retaining tools is driver-owned.
export function verifyRecipeMaterials(review, { toolMaterials, inputMaterials }) {
  reviewedDocument(review);
  exactMaterials(toolMaterials,
    review.bindings.reviewedTools.map((tool) => ({ key: tool.name, sha256: tool.sha256 })),
    'TRUST-TOOL');
  exactMaterials(inputMaterials, review.bindings.inputAllowlist, 'TRUST-NATIVE');
}
