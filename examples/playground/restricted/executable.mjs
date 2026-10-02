// Executable authority comes from an authenticated fixed compiler, not response digests.
import { bindingRevision, exact, fail, isHash, sha256, utf8, world } from './limits.mjs';

export function verifyReceipt(receipt, expected, authority) {
  exact(receipt, ['revision', 'sourceSha256', 'frameSha256', 'compilerSha256', 'toolkitSha256', 'policySha256']);
  exact(authority, ['compilerSha256', 'toolkitSha256', 'policySha256', 'bindingTemplateSha256']);
  if (Object.values(authority).some(value => !isHash(value)) ||
      receipt.revision !== expected.revision || receipt.sourceSha256 !== expected.sourceSha256 ||
      !isHash(receipt.frameSha256) ||
      ['compilerSha256', 'toolkitSha256', 'policySha256'].some(key => receipt[key] !== authority[key])) {
    fail('COMPILER-AUTHORITY');
  }
}

export async function verifyExecutable(bytes, compiled, receipt, authority, templateBytes) {
  verifyReceipt(receipt, compiled.metadata, authority);
  if (await sha256(bytes) !== receipt.frameSha256 ||
      await sha256(templateBytes) !== authority.bindingTemplateSha256) fail('EXECUTABLE-AUTHORITY');
  const { identity, exports, artifacts } = compiled.metadata;
  if (identity.revision !== bindingRevision || identity.world !== world ||
      compiled.metadata.status !== 'compiled') fail('EXECUTABLE-IDENTITY');
  const component = compiled.artifacts[0];
  if (JSON.stringify([...component.subarray(0, 8)]) !== '[0,97,115,109,13,0,1,0]') fail('COMPONENT-HEADER');
  const chunks = [utf8('ZRYNA-SCALAR-COMPONENT-INTERFACE\0\x01')];
  function length(value) {
    const bytes = new Uint8Array(8); new DataView(bytes.buffer).setBigUint64(0, BigInt(value), true);
    chunks.push(bytes);
  }
  for (const entry of exports) {
    for (const key of ['logical', 'component', 'core']) {
      const bytes = utf8(entry[key]); length(bytes.length); chunks.push(bytes);
    }
    length(entry.arity);
  }
  const interfaceBytes = new Uint8Array(chunks.reduce((sum, bytes) => sum + bytes.length, 0));
  let offset = 0;
  for (const bytes of chunks) { interfaceBytes.set(bytes, offset); offset += bytes.length; }
  if (await sha256(interfaceBytes) !== identity.interfaceSha256) fail('INTERFACE-IDENTITY');
  const substitutions = { '__REVISION__': bindingRevision, '__WORLD__': world,
    '__COMPONENT_SHA256__': identity.componentSha256, '__INTERFACE_SHA256__': identity.interfaceSha256,
    '__COMPONENT_BYTES__': String(artifacts[0].bytes), '__CORE_OFFSET__': String(identity.coreOffset),
    '__CORE_BYTES__': String(identity.coreBytes), '__EXPORTS__': JSON.stringify(exports.map(entry =>
      ({ logical: entry.logical, component: entry.component, core: entry.core, arity: entry.arity }))) };
  let expectedLoader;
  try { expectedLoader = new TextDecoder('utf-8', { fatal: true, ignoreBOM: true }).decode(templateBytes); }
  catch { fail('BINDING-TEMPLATE'); }
  for (const [key, value] of Object.entries(substitutions)) expectedLoader = expectedLoader.replaceAll(key, value);
  if (await sha256(utf8(expectedLoader)) !== artifacts[1].sha256) fail('BINDING-TEMPLATE');
  const core = component.slice(identity.coreOffset, identity.coreOffset + identity.coreBytes);
  const module = await WebAssembly.compile(core);
  if (WebAssembly.Module.imports(module).length) fail('CORE-IMPORTS');
  const actual = WebAssembly.Module.exports(module);
  const names = exports.map(entry => entry.core).sort();
  if (actual.some(entry => entry.kind !== 'function') ||
      JSON.stringify(actual.map(entry => entry.name).sort()) !== JSON.stringify(names)) fail('CORE-EXPORTS');
}
