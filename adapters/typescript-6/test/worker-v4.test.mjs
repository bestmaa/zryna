import assert from 'node:assert/strict';
import { spawn } from 'node:child_process';
import { once } from 'node:events';
import { readFile } from 'node:fs/promises';
import test from 'node:test';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

import ts from '@typescript/typescript6';
import Ajv2020 from 'ajv/dist/2020.js';
import './worker-v4-compatibility.test.mjs';

const adapterRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const timeoutMs = 30_000;
const schema = JSON.parse(
  await readFile(new URL('../../../schemas/zryna-syntax-v4.schema.json', import.meta.url)),
);
const validateSnapshot = new Ajv2020({ allErrors: true, strict: true }).compile(schema);

async function exchange(requests, env = {}) {
  const child = spawn(process.execPath, ['src/worker-v4.mjs'], {
    cwd: adapterRoot,
    stdio: ['pipe', 'pipe', 'pipe'],
    windowsHide: true,
    env: { ...process.env, ...env },
  });
  const stdout = [];
  const stderr = [];
  child.stdout.on('data', (chunk) => stdout.push(chunk));
  child.stderr.on('data', (chunk) => stderr.push(chunk));
  child.stdin.end(`${requests.map(JSON.stringify).join('\n')}\n`);
  const exit = once(child, 'exit');
  const timeout = setTimeout(() => child.kill(), timeoutMs);
  const [code] = await exit;
  clearTimeout(timeout);
  assert.equal(code, 0, Buffer.concat(stderr).toString('utf8'));
  assert.equal(Buffer.concat(stderr).length, 0);
  return Buffer.concat(stdout).toString('utf8').split('\n').filter(Boolean).map(JSON.parse);
}

function analyze(id, text, path = 'src/main.zry') {
  return { id, method: 'analyze', params: { schema_version: 4, files: [{ path, text }] } };
}

function analyzeFiles(id, files) {
  return { id, method: 'analyze', params: { schema_version: 4, files } };
}

async function assertLoweredBoundary(exactFiles, extraFiles, env, message) {
  const [exact, extra, recovery] = await exchange([
    analyzeFiles(900, exactFiles), analyzeFiles(901, extraFiles), { id: 902, method: 'handshake' },
  ], { NODE_ENV: 'test', ...env });
  assert.equal(exact.error, undefined, `${message} exact: ${JSON.stringify(exact.error)}`);
  assert.equal(validateSnapshot(exact.result), true, `${message}: ${JSON.stringify(validateSnapshot.errors)}`);
  assert.equal(extra.result, undefined, `${message} first extra was accepted`);
  assert.equal(extra.error.code, 'ZRYNA-F1002', `${message}: ${JSON.stringify(extra.error)}`);
  assert.equal(recovery.result.protocol_version, 4);
}

test('v4 handshake remains syntax-only and explicitly versioned', async () => {
  const [response] = await exchange([{ id: 1, method: 'handshake' }]);
  assert.deepEqual(response, {
    id: 1,
    result: {
      provider: 'typescript-6', provider_version: ts.version, protocol_version: 4,
      capabilities: {
        module_resolution: false, semantic_diagnostics: false,
        control_flow_v1: true, data_ownership_syntax_v1: true,
      },
    },
  });
});

test('v4 emits source-ordered nominal data syntax, dense types, and data expressions', async () => {
  const prefix = '// 😀\r\n';
  const text = `${prefix}export interface Pair extends ZrynaStruct { left: i32; right: i32; }\n`
    + 'interface Maybe extends ZrynaEnum { none: ZrynaNone; some: i32; }\n'
    + 'export function score(left: i32, right: i32): i32 { '
    + 'const pair: Pair = Pair({ left, right: clone(right) }); return pair.left; }';
  const [response] = await exchange([analyze(2, text)]);
  assert.equal(response.error, undefined, JSON.stringify(response.error));
  assert.deepEqual(response.result.diagnostics, []);
  assert.equal(response.result.schema_version, 4);
  assert.equal(validateSnapshot(response.result), true, JSON.stringify(validateSnapshot.errors));
  const file = response.result.files[0];
  assert.equal(file.data_declarations.length, 2);
  assert.deepEqual(file.data_declarations.map((declaration) => declaration.kind.kind), ['struct', 'enum']);
  assert.deepEqual(file.data_declarations[0].kind.fields.map((field) => field.name.text), ['left', 'right']);
  assert.deepEqual(file.data_declarations[1].kind.variants.map((variant) => [variant.name.text, variant.payload_type]), [
    ['none', null], ['some', 2],
  ]);
  assert.equal(file.data_declarations[0].span.start, Buffer.byteLength(prefix, 'utf8'));
  assert.ok(file.type_syntax.length >= 5);
  const expressions = file.functions[0].body.expressions;
  assert.ok(expressions.some((expression) => expression.kind.kind === 'struct-construction'));
  assert.ok(expressions.some((expression) => expression.kind.kind === 'clone'));
  assert.ok(expressions.some((expression) => expression.kind.kind === 'field-access'));
});

test('v4 shorthand construction fixture is accepted unchanged by the Rust contract', async () => {
  const text = await readFile(
    new URL('../../../tests/m3-fixtures/syntax-v4-shorthand.zry', import.meta.url),
    'utf8',
  );
  const expected = JSON.parse(await readFile(
    new URL('../../../tests/m3-fixtures/syntax-v4-shorthand.json', import.meta.url),
    'utf8',
  ));
  const [response] = await exchange([analyze(3, text)]);
  assert.equal(response.error, undefined, JSON.stringify(response.error));
  assert.deepEqual(response.result, expected);
  assert.equal(validateSnapshot(response.result), true, JSON.stringify(validateSnapshot.errors));
});

test('v4 preserves const BorrowMut assignment as syntax without assigning write-through semantics', async () => {
  const text = await readFile(
    new URL('../../../tests/m3-fixtures/exclusive-root-borrow.zry', import.meta.url),
    'utf8',
  );
  const expected = JSON.parse(await readFile(
    new URL('../../../tests/m3-fixtures/exclusive-root-borrow.json', import.meta.url),
    'utf8',
  ));
  const [response] = await exchange([analyze(31, text)]);
  assert.equal(response.error, undefined, JSON.stringify(response.error));
  assert.deepEqual(response.result, expected);
  assert.equal(validateSnapshot(response.result), true, JSON.stringify(validateSnapshot.errors));
  const statements = response.result.files[0].functions[0].body.statements;
  assert.equal(statements[1].kind.kind, 'block');
  assert.ok(statements.some((statement) => statement.kind.kind === 'assignment'));
  assert.ok(response.result.files[0].type_syntax.some((type) => type.kind.kind === 'borrow-mut'));
});

test('v4 preserves whole owned-root shared read syntax without assigning semantics', async () => {
  const text = await readFile(
    new URL('../../../tests/m3-fixtures/owned-root-borrow-reads.zry', import.meta.url),
    'utf8',
  );
  const expected = JSON.parse(await readFile(
    new URL('../../../tests/m3-fixtures/owned-root-borrow-reads.json', import.meta.url),
    'utf8',
  ));
  const [response] = await exchange([analyze(32, text)]);
  assert.equal(response.error, undefined, JSON.stringify(response.error));
  assert.deepEqual(response.result, expected);
  assert.equal(validateSnapshot(response.result), true, JSON.stringify(validateSnapshot.errors));
  assert.deepEqual(
    response.result.files[0].functions.map((fn) => fn.body.statements[1].kind.kind),
    ['block', 'block', 'block', 'block', 'block', 'block'],
  );

  const excludedText = await readFile(
    new URL('../../../tests/m3-fixtures/owned-root-borrow-exclusions.zry', import.meta.url),
    'utf8',
  );
  const excludedExpected = JSON.parse(await readFile(
    new URL('../../../tests/m3-fixtures/owned-root-borrow-exclusions.json', import.meta.url),
    'utf8',
  ));
  const [excluded] = await exchange([analyze(33, excludedText)]);
  assert.equal(excluded.error, undefined, JSON.stringify(excluded.error));
  assert.deepEqual(excluded.result, excludedExpected);
  assert.equal(validateSnapshot(excluded.result), true, JSON.stringify(validateSnapshot.errors));
});

test('v4 preserves conditional arm-local borrow scopes without assigning edge semantics', async () => {
  const text = await readFile(
    new URL('../../../tests/m3-fixtures/conditional-root-borrow.zry', import.meta.url),
    'utf8',
  );
  const expected = JSON.parse(await readFile(
    new URL('../../../tests/m3-fixtures/conditional-root-borrow.json', import.meta.url),
    'utf8',
  ));
  const [response] = await exchange([analyze(32, text)]);
  assert.equal(response.error, undefined, JSON.stringify(response.error));
  assert.deepEqual(response.result, expected);
  assert.equal(validateSnapshot(response.result), true, JSON.stringify(validateSnapshot.errors));
  const body = response.result.files[0].functions[0].body;
  const conditional = body.statements.find((statement) => statement.kind.kind === 'if');
  assert.ok(conditional);
  assert.equal(body.blocks[conditional.kind.then_block].statements.length, 1);
  assert.equal(body.blocks[conditional.kind.else_clause.block].statements.length, 1);
  assert.ok(response.result.files[0].type_syntax.some((type) => type.kind.kind === 'borrow'));
  assert.ok(response.result.files[0].type_syntax.some((type) => type.kind.kind === 'borrow-mut'));
});

test('v4 preserves a loop body borrow scope without assigning backedge semantics', async () => {
  const text = await readFile(
    new URL('../../../tests/m3-fixtures/loop-root-borrow.zry', import.meta.url),
    'utf8',
  );
  const expected = JSON.parse(await readFile(
    new URL('../../../tests/m3-fixtures/loop-root-borrow.json', import.meta.url),
    'utf8',
  ));
  const [response] = await exchange([analyze(33, text)]);
  assert.equal(response.error, undefined, JSON.stringify(response.error));
  assert.deepEqual(response.result, expected);
  assert.equal(validateSnapshot(response.result), true, JSON.stringify(validateSnapshot.errors));
  const body = response.result.files[0].functions[0].body;
  const loop = body.statements.find((statement) => statement.kind.kind === 'while');
  assert.ok(loop);
  assert.equal(body.blocks[loop.kind.body_block].statements.length, 3);
  assert.equal(body.statements[body.blocks[loop.kind.body_block].statements[0]].kind.kind, 'local-declaration');
  assert.ok(response.result.files[0].type_syntax.some((type) => type.kind.kind === 'borrow-mut'));
});

test('v4 preserves the loop-borrow exclusion matrix without assigning semantic admission', async () => {
  const text = await readFile(
    new URL('../../../tests/m3-fixtures/loop-root-borrow-exclusions.zry', import.meta.url),
    'utf8',
  );
  const expected = JSON.parse(await readFile(
    new URL('../../../tests/m3-fixtures/loop-root-borrow-exclusions.json', import.meta.url),
    'utf8',
  ));
  const [response] = await exchange([analyze(34, text)]);
  assert.equal(response.error, undefined, JSON.stringify(response.error));
  assert.deepEqual(response.result, expected);
  assert.equal(validateSnapshot(response.result), true, JSON.stringify(validateSnapshot.errors));
  assert.equal(response.result.files[0].functions.length, 12);
  assert.ok(response.result.files[0].functions.some((fn) =>
    fn.body.statements.filter((statement) => statement.kind.kind === 'while').length === 2));
});

test('v4 preserves projected borrow paths without assigning overlap semantics', async () => {
  const text = await readFile(
    new URL('../../../tests/m3-fixtures/projected-borrow.zry', import.meta.url),
    'utf8',
  );
  const expected = JSON.parse(await readFile(
    new URL('../../../tests/m3-fixtures/projected-borrow.json', import.meta.url),
    'utf8',
  ));
  const [response] = await exchange([analyze(35, text)]);
  assert.equal(response.error, undefined, JSON.stringify(response.error));
  assert.deepEqual(response.result, expected);
  assert.equal(validateSnapshot(response.result), true, JSON.stringify(validateSnapshot.errors));
  const expressions = response.result.files[0].functions[0].body.expressions;
  assert.equal(expressions.filter((expression) => expression.kind.kind === 'borrow-mut').length, 3);
  assert.equal(expressions.filter((expression) => expression.kind.kind === 'borrow').length, 2);
  assert.ok(expressions.some((expression) => expression.kind.kind === 'field-access'));
  assert.ok(expressions.some((expression) => expression.kind.kind === 'index'));
});

test('v4 preserves overlapping shared projected paths as syntax only', async () => {
  const text = await readFile(
    new URL('../../../tests/m3-fixtures/projected-borrow-shared-overlap.zry', import.meta.url),
    'utf8',
  );
  const expected = JSON.parse(await readFile(
    new URL('../../../tests/m3-fixtures/projected-borrow-shared-overlap.json', import.meta.url),
    'utf8',
  ));
  const [response] = await exchange([analyze(36, text)]);
  assert.equal(response.error, undefined, JSON.stringify(response.error));
  assert.deepEqual(response.result, expected);
  assert.equal(validateSnapshot(response.result), true, JSON.stringify(validateSnapshot.errors));
  const expressions = response.result.files[0].functions[0].body.expressions;
  assert.equal(expressions.filter((expression) => expression.kind.kind === 'borrow').length, 2);
});

test('v4 preserves projected-borrow exclusions for deterministic semantic rejection', async () => {
  const text = await readFile(
    new URL('../../../tests/m3-fixtures/projected-borrow-exclusions.zry', import.meta.url),
    'utf8',
  );
  const expected = JSON.parse(await readFile(
    new URL('../../../tests/m3-fixtures/projected-borrow-exclusions.json', import.meta.url),
    'utf8',
  ));
  const [response] = await exchange([analyze(37, text)]);
  assert.equal(response.error, undefined, JSON.stringify(response.error));
  assert.deepEqual(response.result, expected);
  assert.equal(validateSnapshot(response.result), true, JSON.stringify(validateSnapshot.errors));
  assert.equal(response.result.files[0].functions.length, 13);
  const expressions = response.result.files[0].functions.flatMap((fn) => fn.body.expressions);
  assert.ok(expressions.some((expression) => expression.kind.kind === 'field-access'));
  assert.ok(expressions.some((expression) => expression.kind.kind === 'index'));
});

test('v4 reserves only the frozen prototype-sensitive names', async () => {
  const text = 'interface then extends ZrynaStruct { arguments: i32; eval: i32; }';
  const [response] = await exchange([analyze(4, text)]);
  assert.equal(response.error, undefined, JSON.stringify(response.error));
  assert.deepEqual(response.result.diagnostics, []);
  assert.equal(validateSnapshot(response.result), true, JSON.stringify(validateSnapshot.errors));
});

test('v4 emits every aggregate and ownership form with schema-valid postorder arenas', async () => {
  const text = `
interface Pair extends ZrynaStruct { left: i32; right: i32; }
interface Maybe extends ZrynaEnum { none: ZrynaNone; some: i32; }
export function exercise(left: i32, right: i32, weak: Weak<Pair>): i32 {
  const pair: Pair = Pair({ left, right });
  const fixed: FixedArray<i32, 2> = FixedArray<i32, 2>([left, right]);
  let vector: Vec<i32> = Vec<i32>([fixed[0], pair.right]);
  const owner: Shared<Pair> = shared(pair);
  const weaker: Weak<Pair> = downgrade(owner);
  const choice: Maybe = Maybe.some(clone(borrow(pair).left));
  push(vector, borrowMut(pair).right);
  const selected: i32 = match(choice, {
    "Maybe.none": () => left,
    "Maybe.some": (item) => item
  });
  upgradeWeak(weaker, (strong) => { clone(strong); }, () => { clone(pair); });
  return selected;
}`;
  const [response] = await exchange([analyze(20, text)]);
  assert.equal(response.error, undefined, JSON.stringify(response.error));
  assert.equal(validateSnapshot(response.result), true, JSON.stringify(validateSnapshot.errors));
  const file = response.result.files[0];
  const tags = file.functions[0].body.expressions.map((expression) => expression.kind.kind);
  for (const tag of [
    'struct-construction', 'fixed-array-construction', 'vec-construction', 'index',
    'field-access', 'shared', 'downgrade', 'clone', 'borrow', 'borrow-mut',
    'enum-construction', 'vec-push', 'match',
  ]) assert.ok(tags.includes(tag), `missing ${tag}`);
  const statementTags = file.functions[0].body.statements.map((statement) => statement.kind.kind);
  assert.ok(statementTags.includes('expression-statement'));
  assert.ok(statementTags.includes('weak-upgrade'));
  const match = file.functions[0].body.expressions.find((expression) => expression.kind.kind === 'match');
  assert.deepEqual(match.kind.arms.map((arm) => [arm.type_name.text, arm.variant.text, arm.binding?.text ?? null]), [
    ['Maybe', 'none', null], ['Maybe', 'some', 'item'],
  ]);
});

test('v4 reserved forms fail closed on aliases, arity, spreads, holes, and noncanonical match keys', async () => {
  const cases = [
    'push(value)',
    'Vec<i32>([value,, value])',
    'Vec<i32>([...value])',
    'match(value, { "Maybe.none": (item, other) => item })',
    'match(value, { "Maybe.none": item => item })',
    "match(value, { 'Maybe.none': () => value })",
    'match(value, { "Maybe\\u002enone": () => value })',
  ];
  for (const [index, expression] of cases.entries()) {
    const source = `export function bad(value: i32): i32 { ${expression}; return value; }`;
    const [response, recovery] = await exchange([analyze(30 + index, source), { id: 60 + index, method: 'handshake' }]);
    assert.equal(response.error.code, 'ZRYNA-F2002', expression);
    assert.equal(recovery.result.protocol_version, 4);
  }
});

test('v4 preserves canonical string tokens and rejects escapes, templates, and raw newlines', async () => {
  const valid = `export function strings(): String { const left: String = 'plain'; const right: String = "double"; return left; }`;
  const invalid = [
    'export function bad(): String { return "escaped\\n"; }',
    'export function bad(): String { return `template`; }',
    'export function bad(): String { return "raw\nnewline"; }',
  ];
  const responses = await exchange([
    analyze(70, valid),
    ...invalid.map((source, index) => analyze(71 + index, source)),
  ]);
  assert.equal(responses[0].error, undefined, JSON.stringify(responses[0].error));
  assert.equal(validateSnapshot(responses[0].result), true, JSON.stringify(validateSnapshot.errors));
  const spellings = responses[0].result.files[0].functions[0].body.expressions
    .filter((expression) => expression.kind.kind === 'string-literal')
    .map((expression) => expression.kind.spelling);
  assert.deepEqual(spellings, ["'plain'", '"double"']);
  for (const response of responses.slice(1)) assert.equal(response.error.code, 'ZRYNA-F2002');
});

test('v4 fixed-array length emits numeric and spelling forms at the exact limit and rejects first extra', async () => {
  const exactSource = `export function exact(value: FixedArray<i32, 1048576>): i32 {
    const empty: FixedArray<i32, 1048576> = FixedArray<i32, 1048576>([]);
    return 0;
  }`;
  const annotationExtra = 'export function extra(value: FixedArray<i32, 1048577>): i32 { return 0; }';
  const constructionExtra = `export function extra(): i32 {
    const value: i32 = FixedArray<i32, 1048577>([]);
    return value;
  }`;
  const [exact, firstExtra, constructionFirstExtra, recovery] = await exchange([
    analyze(80, exactSource), analyze(81, annotationExtra), analyze(82, constructionExtra),
    { id: 83, method: 'handshake' },
  ]);
  assert.equal(exact.error, undefined, JSON.stringify(exact.error));
  assert.equal(validateSnapshot(exact.result), true, JSON.stringify(validateSnapshot.errors));
  const fixedTypes = exact.result.files[0].type_syntax.filter((type) => type.kind.kind === 'fixed-array');
  assert.ok(fixedTypes.length >= 3);
  for (const type of fixedTypes) {
    assert.equal(type.kind.length, 1_048_576);
    assert.equal(type.kind.length_spelling, '1048576');
  }
  assert.equal(firstExtra.error.code, 'ZRYNA-F1002');
  assert.equal(constructionFirstExtra.error.code, 'ZRYNA-F1002');
  assert.equal(recovery.result.protocol_version, 4);
});

test('v4 lowered M3 project inventories accept exact totals and reject first extra', async () => {
  const file = (path, text) => ({ path, text });
  const declaration = (name, field = 'value') => `interface ${name} extends ZrynaStruct { ${field}: i32; }`;
  await assertLoweredBoundary(
    [file('a.zry', declaration('A')), file('b.zry', declaration('B'))],
    [file('a.zry', declaration('A')), file('b.zry', declaration('B')), file('c.zry', declaration('C'))],
    { ZRYNA_TEST_NOMINAL_DECLARATIONS_PER_PROJECT: '2' }, 'project declarations',
  );
  await assertLoweredBoundary(
    [file('exact.zry', `${declaration('A', 'a')} ${declaration('B', 'b')}`)],
    [file('extra.zry', `${declaration('A', 'a')} ${declaration('B', 'b')} ${declaration('C', 'c')}`)],
    { ZRYNA_TEST_MEMBERS_PER_PROJECT: '2' }, 'project members',
  );
  const missingTypeFunction = (name) => `export function ${name}() { return 1; }`;
  await assertLoweredBoundary(
    [file('a.zry', missingTypeFunction('a')), file('b.zry', missingTypeFunction('b'))],
    [file('a.zry', missingTypeFunction('a')), file('b.zry', missingTypeFunction('b')), file('c.zry', missingTypeFunction('c'))],
    { ZRYNA_TEST_TYPE_SYNTAX_NODES_PER_PROJECT: '2' }, 'project type nodes',
  );
});

test('v4 lowered construction and match inventories accept exact and reject first extra', async () => {
  const file = (text) => [{ path: 'src/main.zry', text }];
  const fn = (body) => `export function value(a: i32, b: i32, c: i32): i32 { ${body} return a; }`;
  await assertLoweredBoundary(
    file(fn('Pair({ a, b });')),
    file(fn('Pair({ a, b, c });')),
    { ZRYNA_TEST_OBJECT_INITIALIZERS_PER_CONSTRUCTION: '2' }, 'object initializers',
  );
  await assertLoweredBoundary(
    file(fn('Vec<i32>([a, b]);')),
    file(fn('Vec<i32>([a, b, c]);')),
    { ZRYNA_TEST_ARRAY_ELEMENTS_PER_CONSTRUCTION: '2' }, 'array elements',
  );
  await assertLoweredBoundary(
    file(fn('Pair({ a }); Pair({ b });')),
    file(fn('Pair({ a }); Pair({ b }); Pair({ c });')),
    { ZRYNA_TEST_CONSTRUCTION_OPERANDS_PER_PROJECT: '2' }, 'project construction operands',
  );
  const match = (arms) => `match(a, { ${arms} });`;
  await assertLoweredBoundary(
    file(fn(match('"Maybe.a": () => a, "Maybe.b": () => b'))),
    file(fn(match('"Maybe.a": () => a, "Maybe.b": () => b, "Maybe.c": () => c'))),
    { ZRYNA_TEST_MATCH_ARMS_PER_EXPRESSION: '2' }, 'match arms per expression',
  );
  await assertLoweredBoundary(
    file(fn(`${match('"Maybe.a": () => a')} ${match('"Maybe.b": () => b')}`)),
    file(fn(`${match('"Maybe.a": () => a')} ${match('"Maybe.b": () => b')} ${match('"Maybe.c": () => c')}`)),
    { ZRYNA_TEST_MATCH_ARMS_PER_PROJECT: '2' }, 'project match arms',
  );
});

test('v4 lowered type module and nesting inventories accept exact and reject first extra', async () => {
  const file = (text) => [{ path: 'src/main.zry', text }];
  const fn = (name, type) => `export function ${name}(): ${type} { return 1; }`;
  await assertLoweredBoundary(
    file(`${fn('a', 'i32')} ${fn('b', 'i32')}`),
    file(`${fn('a', 'i32')} ${fn('b', 'i32')} ${fn('c', 'i32')}`),
    { ZRYNA_TEST_TYPE_SYNTAX_NODES_PER_MODULE: '2' }, 'module type nodes',
  );
  await assertLoweredBoundary(
    file(fn('exact', 'Vec<i32>')),
    file(fn('extra', 'Vec<Vec<i32>>')),
    { ZRYNA_TEST_TYPE_SYNTAX_NESTING: '2' }, 'type nesting',
  );
});

test('v4 rejects prototype-sensitive, structural, malformed, and unsupported data syntax atomically', async () => {
  const cases = [
    'interface Bad extends ZrynaStruct { __proto__: i32; }',
    'interface Bad extends ZrynaStruct { value?: i32; }',
    'interface Bad extends ZrynaStruct { [value: string]: i32; }',
    'interface Bad extends Other { value: i32; }',
    'type Bad = { value: i32 };',
    'class Bad { value: i32; }',
    'interface Bad extends ZrynaEnum { some: i32; some: i32; }',
  ];
  for (const [index, source] of cases.entries()) {
    const [response, recovery] = await exchange([
      analyze(index, `${source}\nexport function okay(): i32 { return 1; }`, `bad/${index}.zry`),
      { id: 100 + index, method: 'handshake' },
    ]);
    assert.equal(response.result, undefined, source);
    assert.equal(response.error.code, 'ZRYNA-F2002', source);
    assert.equal(recovery.result.protocol_version, 4);
  }
});

test('v4 applies one portable identifier grammar to every identifier-bearing path', async () => {
  const overlong = `a${'b'.repeat(128)}`;
  const cases = [
    'export function café(): i32 { return 1; }',
    'export function $value(): i32 { return 1; }',
    `export function ${overlong}(): i32 { return 1; }`,
    'export function value(constructor: i32): i32 { return constructor; }',
    'export function value(): i32 { const prototype: i32 = 1; return prototype; }',
    'import { café } from "./value.zry";',
    'export function value(): i32 { return __proto__; }',
    'export function bad-name(): i32 { return 1; }',
  ];
  for (const [index, source] of cases.entries()) {
    const [response, recovery] = await exchange([
      analyze(120 + index, source, `bad-name/${index}.zry`), { id: 140 + index, method: 'handshake' },
    ]);
    assert.equal(response.error.code, 'ZRYNA-F2002', source);
    assert.equal(recovery.result.protocol_version, 4);
  }
});

test('v4 data budgets accept exact, reject first extra, and recover', async () => {
  const declaration = (name) => `interface ${name} extends ZrynaStruct { value: i32; }`;
  const [exact, extra, recovery] = await exchange([
    analyze(1, `${declaration('A')} ${declaration('B')}`),
    analyze(2, `${declaration('A')} ${declaration('B')} ${declaration('C')}`),
    { id: 3, method: 'handshake' },
  ], { NODE_ENV: 'test', ZRYNA_TEST_NOMINAL_DECLARATIONS: '2' });
  assert.equal(exact.error, undefined, JSON.stringify(exact.error));
  assert.equal(extra.result, undefined);
  assert.equal(extra.error.code, 'ZRYNA-F1002');
  assert.equal(recovery.result.protocol_version, 4);

  const [membersExact, membersExtra] = await exchange([
    analyze(4, 'interface A extends ZrynaStruct { a: i32; b: i32; }'),
    analyze(5, 'interface A extends ZrynaStruct { a: i32; b: i32; c: i32; }'),
  ], { NODE_ENV: 'test', ZRYNA_TEST_MEMBERS_PER_DECLARATION: '2' });
  assert.equal(membersExact.error, undefined);
  assert.equal(membersExtra.error.code, 'ZRYNA-F1002');
});
