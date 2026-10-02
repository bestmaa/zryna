import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import test from 'node:test';
import { canonical, checkDesign, declarationDigest, sha256, validateShape, wire }
  from './native-c-abi-v0/check-design.mjs';

const read = name => readFile(new URL(`./native-c-abi-v0/${name}`, import.meta.url));
const bytes = (await read('declarations.ffi.json')).toString('utf8');
const candidate = JSON.parse(bytes);
const header = await read('candidate.h');
const sources = new Map(await Promise.all(candidate.sources.map(async source =>
  [source.path, await read(source.path.split('/').at(-1))])));
const malformed = JSON.parse(await read('malformed-declarations.json'));
const reviewVectors = JSON.parse(await read('review-vectors.json'));
const negativeSources = JSON.parse(await read('source-negatives.json'));
const clone = () => structuredClone(candidate);

function resealPolicies(document) {
  for (const library of document.libraries) {
    const operations = document.operations.filter(operation => operation.library === library.id)
      .map(({ sourceBinding, ...policy }) => policy);
    library.policySha256 = sha256(wire({ allocators: library.allocators, kinds: library.kinds, operations }));
  }
}

test('complete sidecar schema, source bindings and policy identities match the fixed digest', async () => {
  assert.equal(checkDesign(bytes, sources, header), '7eb6d630326d1dac6542a3579e9e1db42768f923358ffa4ed33d6dbcd556aa1e');
  assert.equal((await read('declarations.sha256')).toString().trim(), declarationDigest(bytes));
  assert.equal(candidate.operations.length, 8);
  assert.deepEqual(candidate.operations.map(operation => operation.sourceBinding.ordinal),
    [0, 1, 2, 3, 4, 5, 6, 7]);
  assert.equal(candidate.sites.length, 31);
  assert.equal(new Set(candidate.sites.map(site => site.primitive)).size, 14);
  assert.deepEqual(candidate.operations.find(operation => operation.symbol === 'fixture_open')
    .statuses.map(status => status.code), [0, 1, 2]);
});

test('independent malformed declarations reject without a matching compiler producer', () => {
  assert.equal(malformed.length, 38);
  assert.ok(malformed.some(row => row.id === 'wrong-canonical-operation-ordinal'
    && row.reseal && row.code === 'ZRYNA-C4106'));
  for (const row of malformed) {
    const document = clone();
    let target = row.target === 'root' ? document : row.target === 'library' ? document.libraries[0]
      : row.target === 'raw-site' ? document.sites.find(site => site.primitive === 'rawCall')
        : document.operations.find(operation => operation.symbol === row.symbol);
    for (const key of row.field.slice(0, -1)) target = target[key];
    const key = row.field.at(-1);
    if (row.delete) delete target[key];
    else target[key] = structuredClone(row.value);
    if (row.reseal) resealPolicies(document);
    assert.throws(() => checkDesign(wire(document), sources, header),
      error => error.message.startsWith(`${row.code}:`), row.id);
  }
  assert.equal(checkDesign(bytes, sources, header), declarationDigest(bytes), 'valid recovery after rejection');
  const unused = clone();
  unused.libraries[0].allocators.unshift({ id: 'fixture-c-v0@0/_unused',
    kind: unused.libraries[0].kinds[0], category: 'handle',
    create: 'fixture-c-v0@0/add', release: 'fixture-c-v0@0/fixture_close' });
  resealPolicies(unused);
  assert.throws(() => checkDesign(wire(unused), sources, header), /ZRYNA-C4105:allocator-reference/,
    'an unused allocator cannot refer to an existing operation with no matching creation');
});

test('duplicate keys, noncanonical wire, invalid UTF-8 and case collisions reject', () => {
  for (const duplicate of [bytes.replace('"version":0', '"version":0,"version":0'),
    bytes.replace('"version":0', '"version":1,"version":0'), bytes.trimEnd(), ` ${bytes}`]) {
    assert.throws(() => checkDesign(duplicate, sources, header), /ZRYNA-C4100:/);
  }
  assert.throws(() => checkDesign(Buffer.from([0xc0, 0xaf]), sources, header), /ZRYNA-C4100:/);
  const collision = clone();
  collision.operations.at(-1).symbol = 'ADD';
  assert.throws(() => checkDesign(wire(collision), sources, header), /ZRYNA-C4102:symbol-collision/);
  const replay = new Map(sources);
  const first = candidate.sources[0].path;
  replay.set(first, Buffer.concat([sources.get(first), Buffer.from('\n')]));
  assert.throws(() => checkDesign(bytes, replay, header), /ZRYNA-C4106:source-bytes/);
  assert.throws(() => checkDesign(reviewVectors.identityExample.wire, sources, header), /ZRYNA-C4101:/,
    'illustrative identity encodings are not complete sidecars');
});

test('closed schema checks exact and first-extra collection limits independently of identity', () => {
  const paths = [
    ['sources', 256], ['libraries', 16], ['operations', 256], ['sites', 4096],
    ['parameters', 16], ['resources', 8], ['statuses', 16], ['kinds', 16], ['allocators', 16],
  ];
  for (const [metric, limit] of paths) {
    const document = clone();
    const object = ['parameters', 'resources', 'statuses'].includes(metric)
      ? document.operations.find(operation => operation.symbol === 'fixture_open')
      : ['kinds', 'allocators'].includes(metric) ? document.libraries[0] : document;
    const exemplar = object[metric][0];
    object[metric] = Array.from({ length: limit }, (_, index) => metric === 'kinds'
      ? `kind${index}` : structuredClone(exemplar));
    assert.equal(validateShape(document), true, `${metric} exact schema bound`);
    object[metric].push(metric === 'kinds' ? 'extra' : structuredClone(exemplar));
    assert.equal(validateShape(document), false, `${metric} first extra`);
    assert.throws(() => checkDesign(wire(document), sources, header), /ZRYNA-C4107:/, metric);
  }
  assert.throws(() => checkDesign(' '.repeat(1048577), sources, header), /ZRYNA-C4107:wire-bytes/);
  assert.throws(() => checkDesign('['.repeat(17) + '0' + ']'.repeat(17) + '\n', sources, header), /ZRYNA-C4107:wire-depth/);
  const exactDepth = '['.repeat(16) + '0' + ']'.repeat(16) + '\n';
  assert.throws(() => checkDesign(exactDepth, sources, header), /ZRYNA-C4101:/,
    'exact depth passes the predecode depth gate but remains the wrong sidecar shape');
});

test('source and signature changes alter identity without inferred typedef equivalence', () => {
  const changed = clone();
  changed.operations[0].parameters[0].abi = 'c-int';
  resealPolicies(changed);
  assert.notEqual(declarationDigest(wire(changed)), declarationDigest(bytes));
  const reordered = clone();
  const sum = reordered.operations.find(operation => operation.symbol === 'sum_bytes');
  [sum.parameters[0], sum.parameters[1]] = [sum.parameters[1], sum.parameters[0]];
  resealPolicies(reordered);
  assert.throws(() => checkDesign(wire(reordered), sources, header), /ZRYNA-C4105:/);
  assert.notEqual(canonical(sum.parameters), canonical(candidate.operations.find(operation =>
    operation.symbol === 'sum_bytes').parameters));
});

test('individual ASCII and UTF-8 limits distinguish exact from first extra', () => {
  const fields = [
    ['name', 128, document => document.operations[0].parameters[0], 'name', length => 'n'.repeat(length)],
    ['key', 257, document => document.operations[0], 'key', length => 'k'.repeat(length)],
    ['path', 256, document => document.sources[0], 'path', length => `${'p'.repeat(length - 4)}.zry`],
    ['library-id', 128, document => document.libraries[0], 'id', length => `${'l'.repeat(length - 2)}@0`],
    ['library-version', 128, document => document.libraries[0], 'version', length => '0'.repeat(length)],
    ['spelling', 4096, document => document.sites[0], 'spelling', length => `Ffi.outI32(${'x'.repeat(length - 12)})`],
  ];
  for (const [metric, limit, select, field, value] of fields) {
    const document = clone();
    select(document)[field] = value(limit);
    assert.equal(Buffer.byteLength(select(document)[field]), limit, metric);
    assert.equal(validateShape(document), true, `${metric} exact shape bound`);
    select(document)[field] = value(limit + 1);
    assert.equal(validateShape(document), false, `${metric} first extra`);
    assert.throws(() => checkDesign(wire(document), sources, header), /ZRYNA-C4107:/, metric);
  }
  for (const [length, expected] of [[4096, /ZRYNA-C4106:/], [4097, /ZRYNA-C4107:primitive-spelling-bytes/]]) {
    const document = clone();
    const prefix = 'Ffi.outI32(';
    const remaining = length - Buffer.byteLength(prefix) - 1;
    document.sites[0].spelling = `${prefix}${'é'.repeat(Math.floor(remaining / 2))}${'x'.repeat(remaining % 2)})`;
    assert.equal(Buffer.byteLength(document.sites[0].spelling), length);
    assert.equal(validateShape(document), true, 'UTF-8 byte cap is stricter than schema character cap');
    assert.throws(() => checkDesign(wire(document), sources, header), expected);
  }
  const exactWire = ` ${bytes.padEnd(1048575, ' ')}`;
  assert.equal(Buffer.byteLength(exactWire), 1048576);
  assert.throws(() => checkDesign(exactWire, sources, header), /ZRYNA-C4100:/,
    'exact wire budget passes size gating, then rejects noncanonical bytes');
});

test('aggregate string-value bytes count each occurrence before source authority', () => {
  const stringBytes = value => typeof value === 'string' ? Buffer.byteLength(value)
    : Array.isArray(value) ? value.reduce((sum, child) => sum + stringBytes(child), 0)
      : value && typeof value === 'object' ? Object.values(value).reduce((sum, child) => sum + stringBytes(child), 0) : 0;
  const document = clone();
  const siteOverhead = stringBytes({ ...document.sites[0], spelling: '' });
  while (stringBytes(document) + siteOverhead + 4096 <= 65536) {
    document.sites.push({ ...document.sites[0], spelling: `Ffi.outI32(${'x'.repeat(4084)})` });
  }
  const remaining = 65536 - stringBytes(document) - stringBytes({ ...document.sites[0], spelling: '' });
  assert.ok(remaining >= 12 && remaining <= 4096);
  document.sites.push({ ...document.sites[0], spelling: `Ffi.outI32(${'x'.repeat(remaining - 12)})` });
  assert.equal(stringBytes(document), 65536);
  assert.equal(validateShape(document), true);
  assert.throws(() => checkDesign(wire(document), sources, header), /ZRYNA-C4106:/,
    'exact aggregate budget reaches source verification; fabricated sites still reject');
  document.sites.at(-1).spelling += 'x';
  assert.equal(stringBytes(document), 65537);
  assert.throws(() => checkDesign(wire(document), sources, header), /ZRYNA-C4107:string-value-bytes/);
});

test('concrete rejected source spellings and token flows remain inert reference inputs', () => {
  assert.equal(negativeSources.format, 'zryna.native-c-source-negatives.v0');
  assert.equal(negativeSources.status, 'reference-only-not-executed');
  assert.equal(negativeSources.cases.length, 16);
  assert.equal(new Set(negativeSources.cases.map(row => row.id)).size, 16);
  assert.deepEqual([...new Set(negativeSources.cases.map(row => row.boundary))].sort(), ['ABI', 'resource', 'source']);
  for (const row of negativeSources.cases) {
    assert.ok(row.source.length > 0 && row.expected.length > 0, row.id);
  }
});
