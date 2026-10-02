// Correlation and byte authentication precede loading any compiler-generated module.
import { bindingRevision, exact, fail, isHash, isRevision, limits, sha256, sourcePath,
  utf8, world } from './limits.mjs';
import { decodeJson } from './json.mjs';
import { verifyReport } from './report.mjs';

export function decodeSourceRequest(bytes) {
  const value = decodeJson(bytes, limits.request);
  exact(value, ['version', 'revision', 'source']);
  if (value.version !== 1 || !isRevision(value.revision) ||
      utf8(value.source).length > limits.source) fail('REQUEST');
  return value;
}

export async function decodeCompilation(bytes, expected) {
  if (!(bytes instanceof Uint8Array) || bytes.length < 4 || bytes.length > limits.frame) fail('FRAME');
  exact(expected, ['revision', 'source', 'witSha256']);
  if (!expected || !isRevision(expected.revision) || !isHash(expected.witSha256)) fail('EXPECTATION');
  const sourceBytes = utf8(expected.source);
  if (sourceBytes.length > limits.source) fail('SOURCE-LIMIT');
  const size = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength).getUint32(0, true);
  if (size > limits.header || size > bytes.length - 4) fail('HEADER');
  const metadata = decodeJson(bytes.subarray(4, 4 + size), limits.header);
  exact(metadata, ['format', 'version', 'revision', 'sourcePath', 'sourceSha256',
    'sourceBytes', 'profile', 'status', 'report', 'failure', 'identity', 'exports', 'artifacts']);
  if (metadata.format !== 'zryna.playground-compilation.v1' || metadata.version !== 1 ||
      metadata.revision !== expected.revision || metadata.sourcePath !== sourcePath ||
      metadata.sourceBytes !== sourceBytes.length ||
      metadata.sourceSha256 !== await sha256(sourceBytes) ||
      metadata.profile !== 'browser-component-v1' ||
      !Array.isArray(metadata.artifacts) || !Array.isArray(metadata.exports)) fail('CORRELATION');
  verifyReport(metadata.report, expected.source);
  if (metadata.status !== 'compiled') {
    if (!['rejected', 'unavailable'].includes(metadata.status) || metadata.identity !== null ||
        metadata.artifacts.length || metadata.exports.length || bytes.length !== size + 4) fail('REJECTION');
    if (metadata.status === 'unavailable') {
      exact(metadata.failure, ['code', 'message']);
      if (!/^ZRYNA-F[0-9]{4}$/.test(metadata.failure.code) ||
          utf8(metadata.failure.message).length > limits.text || metadata.report.diagnostics.length) fail('REJECTION');
    } else if (metadata.failure !== null ||
        !metadata.report.diagnostics.some(record => record.severity === 'error')) fail('REJECTION');
    return { metadata, artifacts: [] };
  }
  if (metadata.failure !== null || metadata.report.diagnostics.some(record => record.severity === 'error') ||
      metadata.artifacts.length !== 3 || !metadata.exports.length) fail('COMPILED');
  exact(metadata.identity, ['revision', 'world', 'witSha256', 'componentSha256',
    'coreSha256', 'interfaceSha256', 'coreOffset', 'coreBytes']);
  const identity = metadata.identity;
  if (identity.revision !== bindingRevision || identity.world !== world ||
      identity.witSha256 !== expected.witSha256 ||
      ['componentSha256', 'coreSha256', 'interfaceSha256'].some(key => !isHash(identity[key])) ||
      !Number.isSafeInteger(identity.coreOffset) || identity.coreOffset < 0 ||
      !Number.isSafeInteger(identity.coreBytes) || identity.coreBytes < 8) fail('IDENTITY');
  const names = new Set();
  const coreNames = new Set();
  for (const entry of metadata.exports) {
    exact(entry, ['logical', 'component', 'core', 'arity']);
    if (typeof entry.logical !== 'string' || entry.logical.length > 1024 ||
        !/^[A-Za-z_][A-Za-z0-9_]*$/.test(entry.logical) ||
        !Number.isSafeInteger(entry.arity) || entry.arity < 0 || entry.arity > 256 ||
        names.has(entry.logical)) fail('EXPORTS');
    const hex = Array.from(utf8(entry.logical)).map(byte => byte.toString(16).padStart(2, '0')).join('');
    if (entry.component !== `zryna-export-${hex}` || typeof entry.core !== 'string' ||
        entry.core.length > 1024 || !/^[A-Za-z_][A-Za-z0-9_]*$/.test(entry.core) ||
        coreNames.has(entry.core)) fail('EXPORTS');
    names.add(entry.logical); coreNames.add(entry.core);
  }
  const artifacts = [];
  let offset = size + 4;
  for (const [index, role] of ['component', 'loader', 'declarations'].entries()) {
    const entry = metadata.artifacts[index];
    exact(entry, ['role', 'bytes', 'sha256']);
    const cap = index === 0 ? limits.component : limits.binding;
    if (entry.role !== role || !Number.isSafeInteger(entry.bytes) || entry.bytes < 1 ||
        entry.bytes > cap || !isHash(entry.sha256) || entry.bytes > bytes.length - offset) fail('ARTIFACT');
    const content = bytes.slice(offset, offset + entry.bytes);
    if (await sha256(content) !== entry.sha256) fail('ARTIFACT-HASH');
    if (index) {
      try { new TextDecoder('utf-8', { fatal: true }).decode(content); }
      catch { fail('ARTIFACT-UTF8'); }
    }
    artifacts.push(content);
    offset += entry.bytes;
  }
  if (offset !== bytes.length || metadata.artifacts[0].sha256 !== identity.componentSha256 ||
      identity.coreOffset > artifacts[0].length - identity.coreBytes ||
      await sha256(artifacts[0].subarray(identity.coreOffset,
        identity.coreOffset + identity.coreBytes)) !== identity.coreSha256) fail('CORE-IDENTITY');
  return { metadata, artifacts };
}

export async function boundedResponse(response, signal) {
  const length = response.headers.get('content-length');
  if (!response.ok || !response.body || response.redirected ||
      (length !== null && (!/^(0|[1-9][0-9]*)$/.test(length) || Number(length) > limits.frame))) fail('RESPONSE');
  const reader = response.body.getReader();
  const chunks = [];
  let size = 0;
  let rejectAbort;
  const aborted = new Promise((_, reject) => { rejectAbort = reject; });
  void aborted.catch(() => {});
  const cancel = () => {
    rejectAbort(new Error('PLAYGROUND-CANCELLED'));
    void reader.cancel().catch(() => {});
  };
  signal?.addEventListener('abort', cancel, { once: true });
  try {
    if (signal?.aborted) cancel();
    while (true) {
      if (signal?.aborted) fail('CANCELLED');
      const { value, done } = await Promise.race([reader.read(), aborted]);
      if (signal?.aborted) fail('CANCELLED');
      if (done) break;
      size += value.length;
      if (size > limits.frame) fail('RESPONSE-LIMIT');
      chunks.push(value);
    }
  } finally {
    signal?.removeEventListener('abort', cancel);
    // A hostile underlying stream cannot indefinitely hold cancellation or UI admission.
    void reader.cancel().catch(() => {});
    reader.releaseLock();
  }
  const bytes = new Uint8Array(size);
  let offset = 0;
  for (const chunk of chunks) { bytes.set(chunk, offset); offset += chunk.length; }
  return bytes;
}
