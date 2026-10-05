// Checks the entire captured upstream ZIP against the separately pinned ordinary-file inventory.
import { inflateRawSync } from 'node:zlib';
import { crc32 } from '../../../scripts/distribution/archive-zip.mjs';
import { orderedPaths, portablePath, sha256 } from '../../../scripts/distribution/canonical.mjs';
import { decodeJson } from './json.mjs';
import { exact, fail, isHash } from './limits.mjs';

const reject = () => fail('BROWSER-ARCHIVE');
const require = condition => { if (!condition) reject(); };

export function verifyBrowserMembers(archive, inventoryBytes) {
  require(Buffer.isBuffer(archive) && archive.length >= 22 && archive.length <= 268435456);
  require(inventoryBytes instanceof Uint8Array && inventoryBytes.length > 0 && inventoryBytes.length <= 8388608);
  const inventory = decodeJson(inventoryBytes, 8388608);
  require(Array.isArray(inventory) && inventory.length > 0 && inventory.length <= 512);
  let declared = 0;
  for (const file of inventory) {
    exact(file, ['path', 'bytes', 'sha256']);
    portablePath(file.path);
    require(file.path.startsWith('chrome-linux64/') && Number.isSafeInteger(file.bytes) &&
      file.bytes >= 0 && file.bytes <= 536870912 && isHash(file.sha256));
    declared += file.bytes;
  }
  orderedPaths(inventory.map(file => file.path));
  require(declared <= 1073741824 && inventory.some(file => file.path === 'chrome-linux64/chrome'));
  const expected = new Map(inventory.map(file => [file.path, file]));
  const eocd = archive.length - 22;
  require(archive.readUInt32LE(eocd) === 0x06054b50 && archive.readUInt16LE(eocd + 20) === 0 &&
    archive.readUInt16LE(eocd + 4) === 0 && archive.readUInt16LE(eocd + 6) === 0);
  const count = archive.readUInt16LE(eocd + 10), start = archive.readUInt32LE(eocd + 16);
  require(count > 0 && count <= 1024 && count === archive.readUInt16LE(eocd + 8) &&
    start + archive.readUInt32LE(eocd + 12) === eocd);
  let cursor = start, expanded = 0;
  const seen = new Set(), spellings = new Map(), paths = [], spans = [];
  for (let index = 0; index < count; index++) {
    require(cursor + 46 <= eocd && archive.readUInt32LE(cursor) === 0x02014b50);
    const flags = archive.readUInt16LE(cursor + 8), method = archive.readUInt16LE(cursor + 10);
    const checksum = archive.readUInt32LE(cursor + 16), compressed = archive.readUInt32LE(cursor + 20);
    const size = archive.readUInt32LE(cursor + 24), nameLength = archive.readUInt16LE(cursor + 28);
    const next = cursor + 46 + nameLength + archive.readUInt16LE(cursor + 30) + archive.readUInt16LE(cursor + 32);
    require(next <= eocd && nameLength > 0 && nameLength <= 200 && (flags & ~0x808) === 0 &&
      [0, 8].includes(method) && archive.readUInt16LE(cursor + 34) === 0 && size <= 536870912 &&
      (expanded += size) <= 1073741824);
    const nameBytes = archive.subarray(cursor + 46, cursor + 46 + nameLength);
    require(nameBytes.every(byte => byte >= 32 && byte < 127));
    const name = nameBytes.toString('ascii'), directory = name.endsWith('/');
    const path = directory ? name.slice(0, -1) : name;
    portablePath(path);
    require(path === 'chrome-linux64' || path.startsWith('chrome-linux64/'));
    const segments = path.split('/');
    for (let depth = 1; depth <= segments.length; depth++) {
      const prefix = segments.slice(0, depth).join('/'), folded = prefix.toLowerCase();
      require(!spellings.has(folded) || spellings.get(folded) === prefix);
      spellings.set(folded, prefix);
    }
    require(!seen.has(path.toLowerCase()));
    seen.add(path.toLowerCase());
    if (!directory) paths.push(path);
    const external = archive.readUInt32LE(cursor + 38), mode = external >>> 16;
    const creator = archive.readUInt16LE(cursor + 4) >>> 8;
    if (creator === 3) require([0, directory ? 0o040000 : 0o100000].includes(mode & 0o170000) && !(mode & 0o6000));
    require(!(external & 0x10) || directory);
    const local = archive.readUInt32LE(cursor + 42);
    require(local + 30 <= start && archive.readUInt32LE(local) === 0x04034b50 &&
      archive.readUInt16LE(local + 6) === flags && archive.readUInt16LE(local + 8) === method);
    const localName = archive.readUInt16LE(local + 26);
    const dataStart = local + 30 + localName + archive.readUInt16LE(local + 28);
    let end = dataStart + compressed;
    require(end <= start && localName === nameLength &&
      archive.subarray(local + 30, local + 30 + localName).equals(nameBytes));
    if (!(flags & 8)) {
      require(archive.readUInt32LE(local + 14) === checksum && archive.readUInt32LE(local + 18) === compressed &&
        archive.readUInt32LE(local + 22) === size);
    } else {
      require(end + 12 <= start);
      const signed = archive.readUInt32LE(end) === 0x08074b50;
      const offset = end + (signed ? 4 : 0);
      require(offset + 12 <= start && archive.readUInt32LE(offset) === checksum &&
        archive.readUInt32LE(offset + 4) === compressed && archive.readUInt32LE(offset + 8) === size);
      end = offset + 12;
    }
    spans.push([local, end]);
    const data = archive.subarray(dataStart, dataStart + compressed);
    let output = data;
    if (method === 8) {
      try {
        const result = inflateRawSync(data, { info: true, maxOutputLength: Math.max(size, 1) });
        require(result.engine.bytesWritten === data.length);
        output = result.buffer;
      } catch { reject(); }
    }
    require(output.length === size && crc32(output) === checksum);
    if (directory) require(size === 0);
    else require(expected.has(path) && expected.get(path).bytes === size && expected.get(path).sha256 === sha256(output));
    cursor = next;
  }
  orderedPaths(paths.sort());
  spans.sort((left, right) => left[0] - right[0]);
  let boundary = 0;
  for (const [begin, end] of spans) { require(begin === boundary); boundary = end; }
  require(boundary === start && cursor === eocd && paths.length === expected.size);
  return structuredClone(inventory);
}
