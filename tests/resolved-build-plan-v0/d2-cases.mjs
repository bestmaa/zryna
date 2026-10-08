import assert from 'node:assert/strict';
import test from 'node:test';
import { createHash } from 'node:crypto';
import { wire } from '../package-release-v1/builders.mjs';
import { fixture as recipeFixture } from '../native-recipe/fixture.mjs';
import { describeNativeRecipe, preflightNativeRecipe } from '../../scripts/native-recipe/preflight.mjs';
import { deriveCacheKey, validateBuildPlan, validateBuildPlanBytes } from '../../scripts/build-plan/validate.mjs';

export function independentCacheKey(document) {
  const material = {
    format: document.format,
    sourcePlan: document.sourcePlan,
    status: document.status,
    version: document.version,
  };
  if (document.nativeAppendix) material.nativeAppendix = document.nativeAppendix;
  return createHash('sha256')
    .update(Buffer.from('ZRYNA-RESOLVED-BUILD-PLAN-V0\0plan\0'))
    .update(wire(material))
    .digest('hex');
}

// The original entry supplies its shared cache builder; hashing remains an independent oracle.
export function registerD2Cases({ withCache }) {
  test('D2 states pin the exact #364 tuple without upgrading drafts or admitting capability claims', async () => {
    for (const status of ['proposed-specified-native-c-interop-v0', 'specified-native-c-interop-v0']) {
      const { document, request } = await recipeFixture();
      assert.deepEqual(validateBuildPlan(document, request.packageAuthority), { cacheKey: document.cacheKey, native: true });
      assert.equal(document.nativeAppendix.status, 'provisional-pending-364');
      document.nativeAppendix.status = status;
      document.nativeAppendix.abi.callingConvention = 'sysv-amd64-c-v0';
      const valid = withCache(document);
      assert.deepEqual(validateBuildPlanBytes(wire(valid), request.packageAuthority), {
        cacheKey: valid.cacheKey, native: true, nativeAdmission: status === 'specified-native-c-interop-v0' ? 'denied-specified' : 'denied-proposed',
      });
      for (const [field, value] of [
        ['identity', 'another-abi'], ['version', '1'], ['version', 0],
        ['targetTriple', 'x86_64-pc-windows-msvc'], ['callingConvention', 'system-v-amd64-c'],
        ['carrierModel', 'scalar-v1'], ['ownershipModel', 'ownership-runtime-v1'],
        ['decisionIssue', 361], ['artifactApproved', true],
      ]) {
        const malformed = structuredClone(valid);
        malformed.nativeAppendix.abi[field] = value;
        assert.throws(() => validateBuildPlanBytes(wire(withCache(malformed)), request.packageAuthority), /P361-SCHEMA/);
      }
      assert.equal(valid.nativeAppendix.status, status);
      for (const status of ['specified', 'accepted', 'implemented', 'conformance-passed', 'publicly-supported']) {
        const claimed = structuredClone(valid);
        claimed.nativeAppendix.status = status;
        assert.throws(() => validateBuildPlan(withCache(claimed), request.packageAuthority), /P361-SCHEMA/);
      }
      const crossTarget = structuredClone(valid);
      crossTarget.nativeAppendix.target = 'javascript';
      assert.throws(() => validateBuildPlan(withCache(crossTarget), request.packageAuthority), /P361-SCHEMA/);
      const wrongTriple = structuredClone(valid);
      wrongTriple.nativeAppendix.abi.targetTriple = wrongTriple.sourcePlan.targets[1].triple = 'aarch64-unknown-linux-gnu';
      assert.throws(() => validateBuildPlan(withCache(wrongTriple), request.packageAuthority), /P361-SCHEMA/);
    }
  });

  test('D2 runtime hashes bind identity but never authenticate bytes or admit recipe execution', async () => {
    for (const status of ['proposed-specified-native-c-interop-v0', 'specified-native-c-interop-v0']) {
      const { document, request } = await recipeFixture();
      document.nativeAppendix.status = status;
      document.nativeAppendix.abi.callingConvention = 'sysv-amd64-c-v0';
      const initialKey = deriveCacheKey(document);
      const mismatched = structuredClone(document);
      mismatched.nativeAppendix.abi.runtime = { ...mismatched.nativeAppendix.abi.runtime, sha256: '0'.repeat(64) };
      assert.throws(() => validateBuildPlan(withCache(mismatched), request.packageAuthority), /P361-NATIVE: native ABI runtime identity differs/);
      document.nativeAppendix.abi.runtime.sha256 = document.sourcePlan.targets[1].runtime.sha256 = '9'.repeat(64);
      const planBytes = wire(withCache(document));
      const receipt = validateBuildPlanBytes(planBytes, request.packageAuthority);
      assert.equal(receipt.nativeAdmission, status === 'specified-native-c-interop-v0' ? 'denied-specified' : 'denied-proposed');
      assert.notEqual(receipt.cacheKey, initialKey);
      assert.equal(receipt.cacheKey, independentCacheKey(document));
      const review = describeNativeRecipe(planBytes, request.packageAuthority, request.stepId);
      assert.throws(() => preflightNativeRecipe({ ...request, planBytes, expectedRecipeSha256: review.recipeSha256,
        nativeAppendixAccepted: true, runtimeArtifactAuthenticated: true, ffiConformance: true }),
      /TRUST-NATIVE: native appendix acceptance and authenticated ABI\/host enforcement are unavailable/);
    }
  });
}
