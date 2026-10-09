import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { readFile } from 'node:fs/promises';
import test from 'node:test';
import { parseSource } from './native-c-prototype/source.mjs';
import { inspectFunction } from './native-c-prototype/resources.mjs';
import { inspectSources, scalarPrototypeHeader } from './native-c-prototype/declarations.mjs';

const read = name => readFile(new URL(`./native-c-abi-v0/${name}`, import.meta.url));
const document = JSON.parse(await read('declarations.ffi.json'));
const operations = new Map(document.operations.map(operation => [operation.key, operation]));
const sources = new Map(await Promise.all(document.sources.map(async source =>
  [source.path, await read(source.path.split('/').at(-1))])));
const inspect = body => inspectFunction(parseSource(Buffer.from(
  `function wrapper(seed: i32): i32 { ${body} }`,
)).functions[0], operations);
const open = index => `const out${index}: FfiHandleOut = Ffi.outHandle("fixture-c-v0@0/fixture_handle");
const status${index}: i32 = Ffi.rawCall("fixture-c-v0@0/fixture_open", seed, out${index});
if (status${index} !== 0) { return Ffi.foreignError("fixture-c-v0@0/fixture_open", status${index}); }
const handle${index}: FfiHandle = Ffi.takeHandle(out${index});`;
const bytes = `const loan: FfiBytes = Ffi.borrowBytes(input);
const pointer: FfiBytesOut = Ffi.outBytes();
const count: FfiCountOut = Ffi.outCount();
const status: i32 = Ffi.rawCall("fixture-c-v0@0/fixture_copy_bytes", loan, Ffi.byteLength(loan), pointer, count);
if (status !== 0) { return Ffi.foreignError("fixture-c-v0@0/fixture_copy_bytes", status); }
const owner: FfiOwnedBytes = Ffi.takeBytes(pointer, count);`;
const inspectBytes = suffix => inspectFunction(parseSource(Buffer.from(
  `function wrapper(input: Vec<i32>): i32 { ${bytes} ${suffix} }`,
)).functions[0], operations);
const rejects = (action, detail, code = 'ZRYNA-C4106') => assert.throws(action,
  error => error.code === code && error.message.includes(detail));

test('prototype parses every fixed scalar/buffer/handle site from complete source bytes', () => {
  const result = inspectSources(document, sources);
  assert.equal(result.state, 'prototype-source-inspected');
  assert.equal(result.inspections.length, 6);
  assert.equal(result.sites.length, 31);
  assert.deepEqual(result.sites, document.sites);
  assert.equal(new Set(result.sites.map(site => site.primitive)).size, 14);
  assert.equal('verifiedProgram' in result, false);
  const read = result.inspections.find(fn => fn.name === 'readSeed');
  const failure = read.plans.filter(plan => plan.role === 'nonzero-status');
  assert.deepEqual(failure.map(plan => plan.recoverable), [[1, 2], []]);
  assert.equal(failure[1].unknown, 'HostAbiFailure');
  assert.equal(failure[1].cleanup[0].release, 'fixture-c-v0@0/fixture_close');
});

test('source identities, exact file set and selected target fail before projection', () => {
  for (const target of ['universal', 'javascript', 'webassembly', 'component', 'all',
    'x86_64-pc-windows-msvc', 'aarch64-unknown-linux-gnu', 'x86_64-unknown-linux-gnux32'])
    rejects(() => inspectSources(document, sources, target), 'unsupported-target', 'ZRYNA-C4103');
  const replay = new Map(sources);
  const path = document.sources[0].path;
  replay.set(path, Buffer.concat([replay.get(path), Buffer.from('\n')]));
  rejects(() => inspectSources(document, replay), 'source-identity');
  const extra = new Map(sources); extra.set('extra.zry', Buffer.from(''));
  rejects(() => inspectSources(document, extra), 'exact-source-set');
  const ordinal = structuredClone(document); ordinal.operations[0].sourceBinding.ordinal = 1;
  rejects(() => inspectSources(ordinal, sources), 'operation-source-identity');
  assert.equal(inspectSources(document, sources).sites.length, 31, 'recovery retains no state');
});

test('primitive identity is parsed independently of sidecar claims and comments', () => {
  const missing = structuredClone(document); missing.sites.splice(0, 1);
  rejects(() => inspectSources(missing, sources), 'complete-primitive-site-set');
  const marker = structuredClone(document);
  marker.sites.find(site => site.primitive === 'rawCall').safety = 'safe';
  rejects(() => inspectSources(marker, sources), 'parsed-primitive-site');
  const utf8 = Buffer.from('// é has two UTF-8 bytes\nfunction f(): i32 { return Ffi.rawCall("fixture-c-v0@0/add", 1, 2); }');
  const fn = parseSource(utf8).functions[0];
  assert.equal(fn.start, utf8.indexOf('function'));
  assert.equal(fn.body[0].value.start, utf8.indexOf('Ffi.rawCall'));
  const comment = Buffer.from('function f(): i32 { /* Ffi.rawCall("fixture-c-v0@0/add", 1, 2) */ return 0; }');
  const forged = { ...document, sources: [{ path: 'f.zry', sha256: createHash('sha256').update(comment).digest('hex') }],
    operations: [], sites: [{ primitive: 'rawCall', path: 'f.zry' }] };
  rejects(() => inspectSources(forged, new Map([['f.zry', comment]])), 'complete-primitive-site-set');
});

test('independent exact-offset and authenticated same-length source mutants reject', () => {
  const shiftedSite = structuredClone(document);
  shiftedSite.sites.find(site => site.primitive === 'rawCall').start += 1;
  rejects(() => inspectSources(shiftedSite, sources), 'parsed-primitive-site');
  const shiftedBinding = structuredClone(document);
  shiftedBinding.operations.find(operation => operation.direction === 'import').sourceBinding.start += 1;
  rejects(() => inspectSources(shiftedBinding, sources), 'parsed-import-binding');
  const changed = new Map(sources);
  const path = 'tests/native-c-abi-v0/source-scalar.zry';
  const old = changed.get(path);
  const replacement = Buffer.from(old.toString().replace('Ffi.rawCall', 'Ffi.rawCa11'));
  assert.equal(replacement.length, old.length, 'byte-length identity cannot authenticate a primitive');
  changed.set(path, replacement);
  const resealed = structuredClone(document);
  const sha256 = createHash('sha256').update(replacement).digest('hex');
  resealed.sources.find(source => source.path === path).sha256 = sha256;
  for (const operation of resealed.operations) if (operation.sourceBinding.path === path)
    operation.sourceBinding.sha256 = sha256;
  for (const site of resealed.sites) if (site.path === path) site.sourceSha256 = sha256;
  rejects(() => inspectSources(resealed, changed), 'primitive');
});

test('reserved namespace and bounded source grammar reject aliases and unsupported call forms', async () => {
  const negatives = JSON.parse(await read('source-negatives.json'));
  for (const row of negatives.cases.slice(0, 8)) assert.throws(() => {
    for (const fn of parseSource(Buffer.from(row.source)).functions) inspectFunction(fn, operations);
  }, undefined, row.id);
  for (const expression of ['Ffi["rawCall"]("fixture-c-v0@0/add", 1, 2)',
    'Ffi?.rawCall("fixture-c-v0@0/add", 1, 2)', 'Ffi.rawCall<i32>("fixture-c-v0@0/add", 1, 2)',
    'Ffi.rawCall("fixture-c-v0@0/add", ...values)', 'Ffi.rawCall("fixture-c-v0@0/\\u0061dd", 1, 2)',
    'Ffi.unknown()', 'plain(1, 2)']) assert.throws(() => inspect(`return ${expression};`), undefined, expression);
  rejects(() => inspect('return Ffi.rawCall("fixture-c-v0@0/add", true, 2);'),
    'raw-argument-type', 'ZRYNA-C4104');
  rejects(() => inspect('return Ffi.rawCall("fixture-c-v0@0/add", 1);'), 'raw-arity', 'ZRYNA-C4104');
});

test('output initialization requires the exact immediate terminal status guard', () => {
  rejects(() => inspect('const out: FfiI32Out = Ffi.outI32(); return Ffi.readI32(out);'), 'uninitialized-output');
  const acquiring = open(0);
  rejects(() => inspect(acquiring.replace(/if \(status0[^\n]+\n/, '') + 'return 0;'), 'missing-status-guard');
  rejects(() => inspect(acquiring.replace('Ffi.foreignError("fixture-c-v0@0/fixture_open", status0)',
    'Ffi.foreignError("fixture-c-v0@0/fixture_read", status0)') + 'return 0;'), 'status-dominance');
  rejects(() => inspect(acquiring.replace('fixture_open", status0)', 'fixture_open", seed)') + 'return 0;'),
    'status-dominance');
  rejects(() => inspect('return Ffi.foreignError("fixture-c-v0@0/fixture_open", 1);'), 'unguarded-foreign-error');
});

test('owner kind, allocator identity, repeated release and repeated take reject', () => {
  rejects(() => inspect(open(0) + 'Ffi.release("fixture-c-v0@0/fixture_release_bytes", handle0); return 0;'),
    'owner-library-kind-allocator');
  rejects(() => inspect(open(0) + 'Ffi.release("fixture-c-v0@0/fixture_close", handle0);'
    + 'Ffi.release("fixture-c-v0@0/fixture_close", handle0); return 0;'), 'stale-token');
  rejects(() => inspect(open(0) + 'const second: FfiHandle = Ffi.takeHandle(out0); return 0;'), 'stale-token');
  const forged = structuredClone(document);
  forged.operations.find(operation => operation.symbol === 'fixture_close').resources[0].allocator = 'other@0/open';
  const map = new Map(forged.operations.map(operation => [operation.key, operation]));
  const fn = parseSource(Buffer.from(`function f(seed: i32): i32 { ${open(0)}
    Ffi.release("fixture-c-v0@0/fixture_close", handle0); return 0; }`)).functions[0];
  rejects(() => inspectFunction(fn, map), 'owner-library-kind-allocator');
});

test('byte loans retain exact count pairing and outputs retain their acquiring call', () => {
  rejects(() => inspectFunction(parseSource(Buffer.from(`function f(input: Vec<i32>): i32 {
    ${bytes.replace('Ffi.byteLength(loan)', '3')} return 0; }`)).functions[0], operations),
  'unpaired-byte-count');
  rejects(() => inspectBytes('const again: FfiOwnedBytes = Ffi.takeBytes(pointer, count); return 0;'), 'stale-token');
  rejects(() => inspectBytes('Ffi.release("fixture-c-v0@0/fixture_close", owner); return 0;'),
    'owner-library-kind-allocator');
  const bad = bytes.replace('const owner: FfiOwnedBytes = Ffi.takeBytes(pointer, count);',
    'const other: FfiCountOut = Ffi.outCount(); const owner: FfiOwnedBytes = Ffi.takeBytes(pointer, other);');
  rejects(() => inspectFunction(parseSource(Buffer.from(`function f(input: Vec<i32>): i32 {
    ${bad} return 0; }`)).functions[0], operations), 'uninitialized-output');
  const creation = index => `const p${index}: FfiBytesOut = Ffi.outBytes();
    const n${index}: FfiCountOut = Ffi.outCount();
    const s${index}: i32 = Ffi.rawCall("fixture-c-v0@0/fixture_copy_bytes", loan, Ffi.byteLength(loan), p${index}, n${index});
    if (s${index} !== 0) { return Ffi.foreignError("fixture-c-v0@0/fixture_copy_bytes", s${index}); }`;
  const mixedCallOutputs = `function mixed(input: Vec<i32>): i32 {
    const loan: FfiBytes = Ffi.borrowBytes(input); ${creation(0)} ${creation(1)}
    const owner: FfiOwnedBytes = Ffi.takeBytes(p0, n1); return 0; }`;
  rejects(() => inspectFunction(parseSource(Buffer.from(mixedCallOutputs)).functions[0], operations),
    'unpaired-byte-outputs');
});

test('released bytes never reenter subsequent return or preparation cleanup plans', () => {
  const result = inspectBytes('const copied: Vec<i32> = Ffi.copyBytes(owner);'
    + 'Ffi.release("fixture-c-v0@0/fixture_release_bytes", owner); return 0;');
  assert.equal(result.plans.find(plan => plan.role === 'copy-prepare-failure').cleanup.length, 1);
  assert.deepEqual(result.plans.at(-1).cleanup, []);
  const prefix = inspect(open(0) + open(1) + 'return 0;');
  const returned = prefix.plans.at(-1).cleanup;
  assert.equal(returned.length, 2);
  assert.ok(returned[0].identity > returned[1].identity, 'newest acquisition cleans first');
  const secondFailure = prefix.plans.filter(plan => plan.role === 'nonzero-status')[1];
  assert.equal(secondFailure.cleanup.length, 1, 'failed acquisition does not create an owner');
  assert.equal(secondFailure.cleanup[0].identity, returned[1].identity);
});

test('foreign tokens stay within their wrapper and total scalar exports exclude effects', () => {
  for (const source of ['function f(owner: FfiHandle): i32 { return 0; }',
    'function f(input: Vec<i32>): FfiBytes { return Ffi.borrowBytes(input); }',
    'export function copied(input: Vec<i32>): Vec<i32> { return input; }',
    'export function f(seed: i32): i32 { return Ffi.rawCall("fixture-c-v0@0/add", seed, 1); }'])
    assert.throws(() => inspectFunction(parseSource(Buffer.from(source)).functions[0], operations));
});

test('prototype budgets intersect the contract at exact and first-extra observations', () => {
  assert.doesNotThrow(() => inspect(Array.from({ length: 64 }, (_, index) => open(index)).join('\n') + 'return 0;'));
  rejects(() => inspect(Array.from({ length: 65 }, (_, index) => open(index)).join('\n') + 'return 0;'),
    'live-obligations', 'ZRYNA-C4107');
  const parameters = count => Array.from({ length: count }, (_, index) => `p${index}: i32`).join(', ');
  assert.doesNotThrow(() => parseSource(Buffer.from(`function f(${parameters(16)}): i32 { return 0; }`)));
  rejects(() => parseSource(Buffer.from(`function f(${parameters(17)}): i32 { return 0; }`)), 'parameters', 'ZRYNA-C4107');
  const functions = count => Array.from({ length: count }, (_, index) => `function f${index}(): i32 { return 0; }`).join('\n');
  assert.equal(parseSource(Buffer.from(functions(256))).functions.length, 256);
  rejects(() => parseSource(Buffer.from(functions(257))), 'functions', 'ZRYNA-C4107');
  const exactBytes = Buffer.alloc(2 * 1024 * 1024, 32);
  assert.equal(parseSource(exactBytes).functions.length, 0);
  rejects(() => parseSource(Buffer.concat([exactBytes, Buffer.from(' ')])), 'source-bytes', 'ZRYNA-C4107');
  const addition = count => `function f(): i32 { return ${Array(count).fill('0').join('+')}; }`;
  assert.doesNotThrow(() => parseSource(Buffer.from(addition(128))));
  rejects(() => parseSource(Buffer.from(addition(129))), 'expression-depth', 'ZRYNA-C4107');
});

test('prototype export header retains explicit C spelling and excluded export rejection', () => {
  assert.equal(scalarPrototypeHeader(document), '#ifndef ZRYNA_NATIVE_C_PROTOTYPE_EXPORTS_H\n'
    + '#define ZRYNA_NATIVE_C_PROTOTYPE_EXPORTS_H\n#include <stdint.h>\n'
    + 'int32_t zryna_c_v0_e_add(int32_t, int32_t);\n#endif\n');
  const explicit = structuredClone(document);
  const exported = explicit.operations.at(-1);
  exported.result = 'c-int'; exported.parameters[0].abi = 'c-int';
  assert.match(scalarPrototypeHeader(explicit), /int zryna_c_v0_e_add\(int, int32_t\);/);
  exported.parameters[0].abi = 'handle-in';
  rejects(() => scalarPrototypeHeader(explicit), 'export-surface', 'ZRYNA-C4104');
  exported.parameters[0].abi = 'c-i32'; exported.mode = 'status';
  rejects(() => scalarPrototypeHeader(explicit), 'export-surface', 'ZRYNA-C4104');
  for (const logicalName of ['constructor', '__proto__', 'prototype', 'then', 'await', 'a'.repeat(116)]) {
    const invalidName = structuredClone(document);
    invalidName.operations.at(-1).logicalName = logicalName;
    invalidName.operations.at(-1).symbol = `zryna_c_v0_e_${logicalName}`;
    rejects(() => scalarPrototypeHeader(invalidName), 'export-symbol', 'ZRYNA-C4102');
  }
  const collision = structuredClone(document);
  collision.operations[0].symbol = 'ZRYNA_C_V0_E_ADD';
  rejects(() => scalarPrototypeHeader(collision), 'export-symbol', 'ZRYNA-C4102');
  const defensive = parseSource(Buffer.from('export function constructor(): i32 { return 0; }')).functions[0];
  rejects(() => inspectFunction(defensive, operations), 'export-name', 'ZRYNA-C4104');
});
