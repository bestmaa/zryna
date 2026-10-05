import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { readFileSync } from 'node:fs';
import { test } from 'node:test';
import { bytes, sha256 } from '../scripts/distribution/canonical.mjs';
import { verifyWitSources, witClosurePath } from '../examples/playground/restricted/wit-closure.mjs';

// Actual checked-in pinned source bytes; this does not run or authenticate a compiler binary.
function fixture() {
  const root = new URL('../', import.meta.url);
  const pins = readFileSync(new URL('crates/zryna-backend-webassembly/src/wit_world_audit/pins.rs', root), 'utf8');
  const tuples = [...pins.slice(pins.indexOf('const SOURCES:'), pins.indexOf('const SOURCE_BYTES:'))
    .matchAll(/pin\(\s*[\s\S]*?,\s*"([^"]+)",\s*"([a-f0-9]{64})",\s*\)/g)]
    .map(match => ({ path: match[1], sha256: match[2] })).sort((a, b) => a.path.localeCompare(b.path, 'en'));
  assert.equal(tuples.length, 34);
  const captured = new Map();
  const sources = tuples.map(tuple => {
    const actual = tuple.path.startsWith('spec/') ? tuple.path :
      `crates/zryna-backend-webassembly/tests/wit-world-audit-v1/dependencies/${tuple.path.slice(5)}`;
    const data = readFileSync(new URL(actual, root));
    assert.equal(sha256(data), tuple.sha256);
    const resource = `resources/wit/${tuple.path}`;
    captured.set(resource, data);
    if (tuple.path.startsWith('spec/')) captured.set('resources/browser.wit', Buffer.from(data));
    return { ...tuple, resource, bytes: data.length };
  });
  const document = { format: 'zryna.playground-wit-sources.v1', version: 1, sources };
  captured.set(witClosurePath, bytes(document));
  return { captured, document };
}

test('actual 34 pinned WIT sources reproduce the native domain-separated serialization independently', () => {
  const { captured, document } = fixture();
  const chunks = [Buffer.from([90, 82, 89, 78, 65, 45, 80, 73, 78, 78, 69, 68, 45,
    87, 73, 84, 45, 83, 79, 85, 82, 67, 69, 83, 0, 1])];
  for (const source of document.sources) {
    const path = Buffer.from(source.path), data = captured.get(source.resource);
    const pathSize = Buffer.alloc(8), dataSize = Buffer.alloc(8);
    pathSize.writeUInt32LE(path.length); dataSize.writeUInt32LE(data.length);
    chunks.push(pathSize, path, dataSize, data);
  }
  const expected = createHash('sha256').update(Buffer.concat(chunks)).digest('hex');
  assert.equal(verifyWitSources(captured), expected);
  assert.notEqual(expected, sha256(captured.get('resources/browser.wit')));
});

test('missing, extra, swapped or mutated source bytes reject even after descriptor resealing', () => {
  for (const mutate of [
    ({ captured }) => captured.delete(witClosurePath),
    ({ document }) => document.sources.pop(),
    ({ document }) => document.sources.reverse(),
    ({ document }) => document.sources[1] = { ...document.sources[0] },
    ({ captured, document }) => captured.delete(document.sources[0].resource),
    ({ captured, document }) => captured.set(document.sources[0].resource, Buffer.from('substituted')),
    ({ captured }) => captured.set('resources/browser.wit', Buffer.from('wrong world')),
    ({ captured }) => captured.set('resources/wit/wasi/io/extra.wit', Buffer.from('extra')),
    ({ document }) => document.sources[0].resource = 'other/resource.wit',
    ({ document }) => document.sources[0].bytes = 16385,
    ({ document }) => document.sources[1].bytes = 32769,
    ({ document }) => document.sources[0].path = 'spec/../worlds.wit',
  ]) {
    const value = fixture(); mutate(value);
    if (value.captured.has(witClosurePath)) value.captured.set(witClosurePath, bytes(value.document));
    assert.throws(() => verifyWitSources(value.captured));
  }
});

test('WIT closure rejects noncanonical, oversized and open metadata before source hashing', () => {
  const { captured, document } = fixture();
  for (const carrier of [Buffer.alloc(16385), Buffer.from(JSON.stringify(document)),
    bytes({ ...document, algorithm: 'different' }), bytes({ ...document, version: 2 }),
    bytes({ ...document, sources: [{ ...document.sources[0], extra: true }, ...document.sources.slice(1)] })]) {
    const substituted = new Map(captured); substituted.set(witClosurePath, carrier);
    assert.throws(() => verifyWitSources(substituted));
  }
});
