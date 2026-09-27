import { browserComponentIdentity, instantiateBrowserComponent } from
  '/.zryna/out/playground-add.build/component/playground-add.mjs';
import { boundedBytes, verifyManifest } from './transport.mjs';

const bundle = '/.zryna/out/playground-add.build/';
const MAX_COMPONENT_BYTES = 2 * 1024 * 1024;
const MAX_MANIFEST_BYTES = 16 * 1024;
const MAX_SOURCE_BYTES = 4096;

async function sha256(bytes) {
  return Array.from(new Uint8Array(await crypto.subtle.digest('SHA-256', bytes)))
    .map(byte => byte.toString(16).padStart(2, '0')).join('');
}

async function invoke(args) {
  const [manifestBytes, source, component] = await Promise.all([
    boundedBytes(`${bundle}zryna-browser-manifest-v1.json`, MAX_MANIFEST_BYTES),
    boundedBytes('/examples/universal/add.zry', MAX_SOURCE_BYTES),
    boundedBytes(`${bundle}component/playground-add.wasm`, MAX_COMPONENT_BYTES),
  ]);
  let manifest;
  try { manifest = JSON.parse(new TextDecoder('utf-8', { fatal: true }).decode(manifestBytes)); }
  catch { throw new Error('Invalid browser manifest'); }
  const identity = browserComponentIdentity;
  verifyManifest(manifest, await sha256(source), identity);
  const instance = await instantiateBrowserComponent(component, { deadlineMs: 4000 });
  if (typeof instance.add !== 'function' || Object.keys(instance).length !== 1) {
    throw new Error('Unsupported scalar interface');
  }
  return { value: instance.add(...args), componentSha256: identity.componentSha256 };
}

self.onmessage = async event => {
  const { id, args } = event.data ?? {};
  if (!Number.isSafeInteger(id) || id < 1 || !Array.isArray(args) || args.length !== 2 ||
      args.some(value => typeof value !== 'number' || !Number.isInteger(value) ||
        value < -2147483648 || value > 2147483647 || Object.is(value, -0))) {
    self.postMessage({ id, ok: false, error: 'Invalid signed i32 arguments' });
    return;
  }
  try { self.postMessage({ id, ok: true, ...await invoke(args) }); }
  catch (error) {
    const allowed = ['ZRYNA-B3991', 'ZRYNA-B3992', 'ZRYNA-B3993',
      'ZRYNA-B3994', 'ZRYNA-B3995'];
    self.postMessage({ id, ok: false, error: allowed.includes(error?.message) ?
      error.message : 'Browser component run failed' });
  }
};
