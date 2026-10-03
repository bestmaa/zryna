import { createHash } from 'node:crypto';

import { canonicalBytes, MAX_PLAN_BYTES, validateBuildPlanBytes } from '../build-plan/validate.mjs';

const reviews = new WeakMap();

export function reject(code, detail) {
  // Details are fixed policy text, never source-controlled paths or environment values.
  throw new Error(`${code}: ${detail}`);
}

function freeze(value) {
  if (value && typeof value === 'object') {
    Object.values(value).forEach(freeze);
    Object.freeze(value);
  }
  return value;
}

export function sourceKey(source) {
  return `${source.graphRole}:${source.package.id}:${source.package.sourceSha256}:${source.path}`;
}

// This is an in-memory review projection, not an extension of #168 or execution authority.
export function describeNativeRecipe(planBytes, packageAuthority, stepId) {
  if (!Buffer.isBuffer(planBytes) || planBytes.length > MAX_PLAN_BYTES) {
    reject('P361-BUDGET', 'wire input exceeds its bound');
  }
  const snapshot = Buffer.from(planBytes);
  validateBuildPlanBytes(snapshot, packageAuthority);
  const document = JSON.parse(snapshot);
  const appendix = document.nativeAppendix;
  if (!appendix || typeof stepId !== 'string') {
    reject('TRUST-NATIVE', 'an exact declared native compilation step is required');
  }
  const step = appendix.compilation.steps.find((item) => item.id === stepId);
  if (!step) reject('TRUST-NATIVE', 'the requested native compilation step is absent');
  const source = document.sourcePlan;
  const inputs = step.inputs.map((input) => {
    if (input.kind === 'source') {
      const entry = source.sources.find((item) => sourceKey(item) === sourceKey(input));
      return { key: `source:${sourceKey(input)}`, ...entry };
    }
    const collection = input.kind === 'static' ? appendix.acquisition.staticArtifacts
      : input.kind === 'shared' ? appendix.acquisition.sharedArtifacts : appendix.acquisition.sysroots;
    return { key: `${input.kind}:${input.id}`, ...collection.find((item) => item.id === input.id) };
  });
  const bindings = freeze({
    planKey: document.cacheKey,
    stepId,
    packageLockSha256: source.packageLockSha256,
    rootPackage: source.rootPackage,
    host: source.host,
    target: source.targets.find((target) => target.id === step.target),
    abi: appendix.abi,
    executionPolicy: source.executionPolicy,
    executable: source.hostTools.find((tool) => tool.name === step.tool),
    // An inventory is not permission to spawn descendants or select another tool.
    reviewedTools: source.hostTools,
    invocation: step.invocation,
    inputAllowlist: inputs,
    objectOutput: step.output,
    link: appendix.linking,
    outputAllowlist: source.outputs.filter((output) => output.target === step.target),
  });
  const recipeSha256 = createHash('sha256')
    .update('ZRYNA-NATIVE-RECIPE-PREFLIGHT-V0\0review\0')
    .update(canonicalBytes(bindings)).digest('hex');
  const review = Object.freeze({ recipeSha256, bindings });
  reviews.set(review, freeze(document));
  return review;
}

export function reviewedDocument(review) {
  const document = reviews.get(review);
  if (!document) reject('TRUST-NATIVE', 'a locally validated recipe review is required');
  return document;
}
