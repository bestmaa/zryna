// Reconstructs the compiler's WIT source digest from the authenticated toolkit inventory.
import { createHash } from 'node:crypto';
import { bytes, orderedPaths, portablePath, sha256 } from '../../../scripts/distribution/canonical.mjs';
import { decodeJson } from './json.mjs';
import { exact, fail, isHash } from './limits.mjs';

export const witClosurePath = 'resources/wit-sources.json';
const rootPath = 'spec/wit/capability-profiles-v1/worlds.wit';

function length(value) {
  const output = Buffer.alloc(8);
  output.writeBigUInt64LE(BigInt(value));
  return output;
}

export function verifyWitSources(captured) {
  const carrier = captured.get(witClosurePath);
  if (!(carrier instanceof Uint8Array)) fail('WIT-CLOSURE');
  const document = decodeJson(carrier, 16384);
  if (!bytes(document).equals(Buffer.from(carrier))) fail('WIT-CLOSURE');
  exact(document, ['format', 'version', 'sources']);
  if (document.format !== 'zryna.playground-wit-sources.v1' || document.version !== 1 ||
      !Array.isArray(document.sources) || document.sources.length !== 34) fail('WIT-CLOSURE');
  const digest = createHash('sha256').update(Buffer.from('ZRYNA-PINNED-WIT-SOURCES\0\x01'));
  let total = 0;
  let root;
  for (const source of document.sources) {
    exact(source, ['path', 'resource', 'bytes', 'sha256']);
    portablePath(source.path);
    if (source.path.length > 96 || source.resource !== `resources/wit/${source.path}` ||
        !Number.isSafeInteger(source.bytes) || source.bytes < 1 ||
        source.bytes > (source.path === rootPath ? 16384 : 32768) || !isHash(source.sha256) ||
        (source.path !== rootPath && !/^wasi\/[a-z]+\/[a-z0-9-]+\.wit$/.test(source.path)) ||
        (total += source.bytes) > 262144) fail('WIT-CLOSURE');
    const data = captured.get(source.resource);
    if (!(data instanceof Uint8Array) || data.length !== source.bytes || sha256(data) !== source.sha256) {
      fail('WIT-CLOSURE');
    }
    const path = Buffer.from(source.path, 'utf8');
    digest.update(length(path.length)); digest.update(path);
    digest.update(length(data.length)); digest.update(data);
    if (source.path === rootPath) root = data;
  }
  orderedPaths(document.sources.map(source => source.path));
  if (!root || !Buffer.from(root).equals(captured.get('resources/browser.wit') ?? Buffer.alloc(0))) {
    fail('WIT-CLOSURE');
  }
  const resources = new Set(document.sources.map(source => source.resource));
  if ([...captured.keys()].some(path => path.startsWith('resources/wit/') && !resources.has(path))) {
    fail('WIT-CLOSURE');
  }
  return digest.digest('hex');
}
