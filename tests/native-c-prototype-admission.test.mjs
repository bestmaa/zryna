import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { readFile } from 'node:fs/promises';
import test from 'node:test';
import { inspectPrototype } from './native-c-prototype/admission.mjs';
import { checkDesign } from './native-c-abi-v0/check-design.mjs';

const read = name => readFile(new URL(`./native-c-abi-v0/${name}`, import.meta.url));
const bytes = await read('declarations.ffi.json');
const reference = JSON.parse(bytes);
const header = await read('candidate.h');
const sources = new Map(await Promise.all(reference.sources.map(async source =>
  [source.path, await read(source.path.split('/').at(-1))])));
const malformed = JSON.parse(await read('malformed-declarations.json'));
const digest = value => createHash('sha256').update(value).digest('hex');
const canonical = value => Array.isArray(value) ? `[${value.map(canonical).join(',')}]`
  : value !== null && typeof value === 'object' ? `{${Object.keys(value).sort()
    .map(key => `${JSON.stringify(key)}:${canonical(value[key])}`).join(',')}}` : JSON.stringify(value);
const wire = value => Buffer.from(`${canonical(value)}\n`);

function refreshPolicies(document) {
  for (const library of document.libraries) library.policySha256 = digest(wire({
    allocators: library.allocators, kinds: library.kinds,
    operations: document.operations.filter(operation => operation.library === library.id)
      .map(({ sourceBinding, ...policy }) => policy),
  }));
}

function refreshSource(document, path, source) {
  const sha256 = digest(source);
  document.sources.find(row => row.path === path).sha256 = sha256;
  for (const operation of document.operations) if (operation.sourceBinding.path === path)
    operation.sourceBinding.sha256 = sha256;
  for (const site of document.sites) if (site.path === path) {
    site.sourceSha256 = sha256;
    site.spelling = source.subarray(site.start, site.end).toString('utf8');
  }
}

test('locked schema and parsed-source composition inspects the complete reference without a compiler seal', () => {
  const result = inspectPrototype(bytes, sources, header);
  assert.equal(result.declarationDigest, '7eb6d630326d1dac6542a3579e9e1db42768f923358ffa4ed33d6dbcd556aa1e');
  assert.equal(result.state, 'prototype-source-inspected');
  assert.equal(result.inspections.length, 6);
  assert.deepEqual(result.sites, reference.sites);
  assert.match(result.header, /int32_t zryna_c_v0_e_add\(int32_t, int32_t\);/);
  for (const authority of ['verifiedProgram', 'verifiedDeclarations', 'verifiedMir', 'object', 'link'])
    assert.equal(authority in result, false, authority);
});

test('composition rejects all 38 independent malformed sidecars before exposing a result', () => {
  assert.equal(malformed.length, 38);
  for (const row of malformed) {
    const document = structuredClone(reference);
    let target = row.target === 'root' ? document : row.target === 'library' ? document.libraries[0]
      : row.target === 'raw-site' ? document.sites.find(site => site.primitive === 'rawCall')
        : document.operations.find(operation => operation.symbol === row.symbol);
    for (const key of row.field.slice(0, -1)) target = target[key];
    if (row.delete) delete target[row.field.at(-1)];
    else target[row.field.at(-1)] = structuredClone(row.value);
    if (row.reseal) refreshPolicies(document);
    assert.throws(() => inspectPrototype(wire(document), sources, header),
      error => error.message.startsWith(`${row.code}:`), row.id);
  }
  assert.equal(inspectPrototype(bytes, sources, header).sites.length, 31);
});

test('composition authenticates exact source/header bytes and independently selected target', () => {
  const changedHeader = Buffer.from(header); changedHeader[0] ^= 1;
  assert.throws(() => inspectPrototype(bytes, sources, changedHeader), /ZRYNA-C4102:header-digest/);
  const changed = new Map(sources);
  const path = reference.sources[0].path;
  changed.set(path, Buffer.concat([changed.get(path), Buffer.from('\n')]));
  assert.throws(() => inspectPrototype(bytes, changed, header), /ZRYNA-C4106:source-bytes/);
  const extra = new Map(sources); extra.set('extra.zry', Buffer.from(''));
  assert.throws(() => inspectPrototype(bytes, extra, header), /exact-source-set/);
  for (const target of ['all', 'universal', 'javascript', 'webassembly', 'component',
    'x86_64-pc-windows-msvc', 'aarch64-unknown-linux-gnu', 'x86_64-unknown-linux-gnux32'])
    assert.throws(() => inspectPrototype(bytes, sources, header, target),
      error => error.code === 'ZRYNA-C4103' && error.message.includes('unsupported-target'), target);
});

test('fresh matching hashes do not conceal an unreported parsed raw call', () => {
  const document = structuredClone(reference);
  const changed = new Map(sources);
  const path = 'tests/native-c-abi-v0/source-scalar.zry';
  const source = Buffer.concat([changed.get(path), Buffer.from(
    '\nfunction unreported(): i32 { return Ffi.rawCall("fixture-c-v0@0/add", 1, 2); }\n',
  )]);
  changed.set(path, source); refreshSource(document, path, source);
  assert.doesNotThrow(() => checkDesign(wire(document), changed, header));
  assert.throws(() => inspectPrototype(wire(document), changed, header), /complete-primitive-site-set/);
});

test('matching sidecar spans and digests cannot promote an intrinsic in a comment to a source node', () => {
  const document = structuredClone(reference);
  const path = 'comment.zry';
  const spelling = 'Ffi.rawCall("fixture-c-v0@0/add", 1, 2)';
  const source = Buffer.from(`function f(): i32 { /* ${spelling} */ return 0; }`);
  const sha256 = digest(source);
  const start = source.indexOf(spelling);
  const end = start + Buffer.byteLength(spelling);
  const add = document.operations.find(operation => operation.symbol === 'add');
  add.sourceBinding = { path, sha256, start, end, ordinal: 0 };
  document.operations = [add];
  document.libraries[0].allocators = []; document.libraries[0].kinds = [];
  document.sources = [{ path, sha256 }];
  document.sites = [{ primitive: 'rawCall', safety: 'unsafe-raw', operation: add.key,
    path, sourceSha256: sha256, start, end, spelling }];
  refreshPolicies(document);
  const isolated = new Map([[path, source]]);
  assert.doesNotThrow(() => checkDesign(wire(document), isolated, header));
  assert.throws(() => inspectPrototype(wire(document), isolated, header), /complete-primitive-site-set/);
});

test('authenticated same-length argument and guard mutations still require parsed types and status dominance', () => {
  for (const [path, before, after, code] of [
    ['tests/native-c-abi-v0/source-scalar.zry', 'add", left, right', 'add", true, right', 'ZRYNA-C4104'],
    ['tests/native-c-abi-v0/source-handle.zry', 'opened !== 0', 'opened === 0', 'ZRYNA-C4106'],
  ]) {
    const document = structuredClone(reference);
    const changed = new Map(sources);
    const original = changed.get(path);
    assert.ok(original.includes(before), 'independent fixture mutation must replace a real site');
    const source = Buffer.from(original.toString('utf8').replace(before, after));
    assert.equal(source.length, original.length);
    changed.set(path, source); refreshSource(document, path, source);
    assert.doesNotThrow(() => checkDesign(wire(document), changed, header));
    assert.throws(() => inspectPrototype(wire(document), changed, header), error => error.code === code);
  }
  assert.equal(inspectPrototype(bytes, sources, header).inspections.length, 6);
});
