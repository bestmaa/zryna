// Only the fixed compiler-generated binding is executable; source remains protocol data.
import { exact, fail, isI32, limits } from './limits.mjs';
import { decodeCompilation } from './protocol.mjs';
import { verifyExecutable } from './executable.mjs';

let consumed = false;
self.onmessage = async event => {
  if (consumed) return;
  consumed = true;
  const request = event.data;
  let componentSha256 = null;
  let loaderUrl;
  try {
    exact(request, ['revision', 'logical', 'args', 'bytes', 'expected', 'receipt', 'authority', 'templateBytes']);
    const compiled = await decodeCompilation(request.bytes, request.expected);
    if (request.revision !== request.expected.revision || compiled.metadata.status !== 'compiled') fail('STALE');
    componentSha256 = compiled.metadata.identity.componentSha256;
    const entry = compiled.metadata.exports.find(entry => entry.logical === request.logical);
    if (!entry || !Array.isArray(request.args) || request.args.length !== entry.arity ||
        request.args.some(value => !isI32(value))) fail('ARGUMENTS');
    await verifyExecutable(request.bytes, compiled, request.receipt, request.authority, request.templateBytes);
    loaderUrl = URL.createObjectURL(new Blob([compiled.artifacts[1]], { type: 'text/javascript' }));
    const { browserComponentIdentity: identity, instantiateBrowserComponent } = await import(loaderUrl);
    const expected = compiled.metadata.identity;
    exact(identity, ['revision', 'world', 'componentSha256', 'interfaceSha256']);
    if (Object.keys(identity).some(key => identity[key] !== expected[key])) fail('BINDING-IDENTITY');
    const instance = await instantiateBrowserComponent(compiled.artifacts[0], { deadlineMs: limits.evaluateMs });
    const names = compiled.metadata.exports.map(entry => entry.logical).sort();
    if (JSON.stringify(Object.keys(instance).sort()) !== JSON.stringify(names) ||
        typeof instance[entry.logical] !== 'function') fail('BINDING-EXPORTS');
    const value = instance[entry.logical](...request.args);
    if (!isI32(value)) fail('OBSERVATION');
    self.postMessage({ revision: request.revision, logical: request.logical,
      componentSha256, ok: true, value, error: null });
  } catch (error) {
    const message = /^(?:PLAYGROUND-[A-Z-]+|ZRYNA-B[0-9]{4})$/.test(error?.message) ?
      error.message : 'PLAYGROUND-WORKER';
    self.postMessage({ revision: request?.revision, logical: request?.logical,
      componentSha256, ok: false, value: null, error: message });
  } finally { if (loaderUrl) URL.revokeObjectURL(loaderUrl); }
};
