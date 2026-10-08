import { createHash } from 'node:crypto';

// Canonical identity and descriptive receipts grant no artifact or execution authority.
function canonical(value) {
  if (Array.isArray(value)) return `[${value.map(canonical).join(',')}]`;
  if (value && typeof value === 'object') {
    return `{${Object.keys(value).sort().map((key) =>
      `${JSON.stringify(key)}:${canonical(value[key])}`).join(',')}}`;
  }
  return JSON.stringify(value);
}

export function canonicalBytes(value) {
  return Buffer.from(`${canonical(value)}\n`, 'utf8');
}

function digest(domain, bytes) {
  return createHash('sha256')
    .update(Buffer.from(`ZRYNA-RESOLVED-BUILD-PLAN-V0\0${domain}\0`))
    .update(bytes)
    .digest('hex');
}

function cacheMaterial(document) {
  const material = {
    format: document.format,
    sourcePlan: document.sourcePlan,
    status: document.status,
    version: document.version,
  };
  if (document.nativeAppendix) material.nativeAppendix = document.nativeAppendix;
  return material;
}

export function deriveCacheKey(document) {
  return digest('plan', canonicalBytes(cacheMaterial(document)));
}

export function deriveTargetCacheKey(document, target) {
  const planKey = deriveCacheKey(document);
  return digest('target', canonicalBytes({ planKey, target }));
}

export function buildPlanReceipt(document) {
  const receipt = { cacheKey: document.cacheKey, native: Boolean(document.nativeAppendix) };
  const status = document.nativeAppendix?.status;
  if (status === 'proposed-specified-native-c-interop-v0' || status === 'specified-native-c-interop-v0') {
    // Specification metadata proves no artifact bytes, execution, reuse, or publication authority.
    return Object.freeze({ ...receipt, nativeAdmission: status === 'specified-native-c-interop-v0'
      ? 'denied-specified' : 'denied-proposed' });
  }
  return Object.freeze(receipt);
}
