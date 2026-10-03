import { describeNativeRecipe, reject } from './identity.mjs';
import { verifyRecipeMaterials, verifyRecipeSources } from './materials.mjs';

export { describeNativeRecipe } from './identity.mjs';

// Repository-only prerequisite checker. It cannot return an execution capability.
export function preflightNativeRecipe(request) {
  if (!request || !['pure-source', 'trusted-native', 'untrusted-playground'].includes(request.mode)) {
    reject('TRUST-MODE', 'the caller must select a known trust mode');
  }
  const review = describeNativeRecipe(request.planBytes, request.packageAuthority, request.stepId);
  verifyRecipeSources(review, request.sourceMaterials);
  if (request.mode !== 'trusted-native') {
    reject('TRUST-OPERATION', 'native recipes are forbidden in the selected mode');
  }
  verifyRecipeMaterials(review, request);
  if (typeof request.expectedRecipeSha256 !== 'string' ||
      !/^[0-9a-f]{64}$/.test(request.expectedRecipeSha256) ||
      request.expectedRecipeSha256 !== review.recipeSha256) {
    reject('TRUST-NATIVE', 'independent approval of the exact recipe identity is required');
  }
  // The accepted #361 schema permits only provisional-pending-364. Neither an approval
  // digest nor caller-provided booleans can prove appendix acceptance, #417 conformance,
  // retained executable identity or actual OS enforcement. There is deliberately no success path.
  reject('TRUST-NATIVE', 'native appendix acceptance and authenticated ABI/host enforcement are unavailable');
}
