import assert from 'node:assert/strict';
import { spawn } from 'node:child_process';
import { once } from 'node:events';
import { readFile } from 'node:fs/promises';
import test from 'node:test';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

import ts from '@typescript/typescript6';
import Ajv2020 from 'ajv/dist/2020.js';

import { PROTOCOL_V3_LIMITS } from '../src/limits-v3.mjs';

const adapterRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const schema = JSON.parse(
  await readFile(new URL('../../../schemas/zryna-syntax-v3.schema.json', import.meta.url)),
);
const validateSnapshot = new Ajv2020({ allErrors: true, strict: true }).compile(schema);
const CHILD_TIMEOUT_MS = 30_000;

test('production protocol limits match the historical M2 contract', async () => {
  const planning = JSON.parse(
    await readFile(new URL('../../../tests/m2-contract-v1.json', import.meta.url)),
  );
  assert.deepEqual(PROTOCOL_V3_LIMITS, {
    requestBytes: planning.limits.protocolRequestBytes,
    responseBytes: planning.limits.protocolResponseBytes,
  });
});

async function exchangeRaw(input, options = {}) {
  const child = spawn(process.execPath, ['src/worker-v3.mjs'], {
    cwd: adapterRoot,
    stdio: ['pipe', 'pipe', 'pipe'],
    windowsHide: true,
    env: { ...process.env, ...options.env },
  });
  const stdout = [];
  const stderr = [];
  child.stdout.on('data', (chunk) => stdout.push(chunk));
  child.stderr.on('data', (chunk) => stderr.push(chunk));
  child.stdin.end(input);
  const exit = once(child, 'exit');
  const timeout = setTimeout(() => child.kill(), CHILD_TIMEOUT_MS);
  const [code] = await exit;
  clearTimeout(timeout);
  assert.equal(code, 0, Buffer.concat(stderr).toString('utf8'));
  assert.equal(Buffer.concat(stderr).length, 0);
  return Buffer.concat(stdout).toString('utf8').split('\n').filter(Boolean).map(JSON.parse);
}

function analyze(id, files) {
  return { id, method: 'analyze', params: { schema_version: 3, files } };
}

function exchange(requests, options) {
  return exchangeRaw(`${requests.map(JSON.stringify).join('\n')}\n`, options);
}

async function assertBudgetBoundary(exactFiles, overflowFiles, env, message) {
  const [exact, overflow, recovery] = await exchange([
    analyze(10, exactFiles),
    analyze(11, overflowFiles),
    { id: 12, method: 'handshake' },
  ], { env: { NODE_ENV: 'test', ...env } });
  assert.equal(exact.error, undefined, JSON.stringify(exact.error));
  assert.equal(overflow.result, undefined);
  assert.equal(overflow.error.code, 'ZRYNA-F1002');
  assert.match(overflow.error.message, message);
  assert.equal(recovery.result.protocol_version, 3);
}

test('v3 handshake is explicit and retains provider authority boundaries', async () => {
  const [response] = await exchange([{ id: 1, method: 'handshake' }]);
  assert.deepEqual(response, {
    id: 1,
    result: {
      provider: 'typescript-6',
      provider_version: ts.version,
      protocol_version: 3,
      capabilities: {
        module_resolution: false,
        semantic_diagnostics: false,
        control_flow_v1: true,
      },
    },
  });
  assert.equal(ts.version, '6.0.3');
});

test('complete ControlFlowV1 syntax uses canonical source-order arenas and exact spans', async () => {
  const request = JSON.parse(await readFile(new URL('../../../tests/fixtures/typescript-adapter-v3-request.json', import.meta.url)));
  const expected = (await readFile(new URL('../../../tests/fixtures/typescript-adapter-v3-result.json', import.meta.url), 'utf8')).trim();
  const text = request.params.files[0].text;
  const [response] = await exchange([request]);
  assert.equal(response.error, undefined, JSON.stringify(response.error));
  assert.equal(validateSnapshot(response.result), true, JSON.stringify(validateSnapshot.errors));
  assert.equal(JSON.stringify(response.result), expected);
  assert.deepEqual(response.result.diagnostics, []);
  const source = response.result.files[0];
  assert.equal(source.id, 0);
  assert.equal(source.path, 'src/main.zry');
  assert.equal(source.imports.length, 1);
  assert.equal(source.imports[0].specifier.text, './math.zry');
  assert.equal(source.imports[0].specifier.value_span.start + 1, source.imports[0].specifier.token_span.start + 2);
  assert.deepEqual(source.imports[0].bindings.map(({ imported, local, as_span }) => [imported.text, local.text, as_span !== null]), [
    ['plus', 'add', true],
    ['truth', 'truth', false],
  ]);
  assert.equal(source.functions[0].export_span, null);
  assert.notEqual(source.functions[1].export_span, null);
  const body = source.functions[1].body;
  assert.equal(body.root_block, 0);
  assert.deepEqual(body.blocks.map((block) => block.statements), [
    [0, 1, 2, 5, 7, 9],
    [3],
    [4],
    [6],
    [8],
  ]);
  assert.deepEqual(body.statements.map((statement) => statement.kind.kind), [
    'local-declaration', 'local-declaration', 'if', 'assignment', 'assignment',
    'while', 'assignment', 'block', 'local-declaration', 'return',
  ]);
  assert.deepEqual(body.expressions.map((expression) => expression.kind.kind), [
    'call', 'reference', 'negation', 'i32-literal', 'multiplication', 'reference',
    'reference', 'i32-literal', 'call', 'reference', 'i32-literal',
    'subtraction', 'reference', 'i32-literal', 'less-than', 'reference', 'i32-literal',
    'addition', 'reference', 'reference',
  ]);
  for (const expression of body.expressions) {
    assert.ok(expression.span.start <= expression.span.end);
    assert.ok(expression.span.end <= Buffer.byteLength(text, 'utf8'));
  }
});

test('checked-in M2 semantic fixtures are exact real adapter responses', async () => {
  for (const stem of [
    'm2-straight-line',
    'm2-semantic-negative',
    'm2-control-flow-positive',
    'm2-control-flow-negative',
  ]) {
    const request = JSON.parse(
      await readFile(new URL(`../../../tests/fixtures/${stem}-request.json`, import.meta.url)),
    );
    const expected = (
      await readFile(new URL(`../../../tests/fixtures/${stem}-result.json`, import.meta.url), 'utf8')
    ).trim();
    const [response] = await exchange([request]);

    assert.equal(response.error, undefined, JSON.stringify(response.error));
    assert.equal(validateSnapshot(response.result), true, JSON.stringify(validateSnapshot.errors));
    assert.equal(JSON.stringify(response.result), expected);
  }
});

test('native local-declaration fixtures are exact TypeScript 6 responses', async () => {
  const fixture = new URL('../../../crates/zryna-frontend/tests/native_parser_v3_locals/', import.meta.url);
  const main = await readFile(new URL('main.zry', fixture), 'utf8');
  const math = await readFile(new URL('math.zry', fixture), 'utf8');
  const useBefore = await readFile(new URL('../../../tests/m2-fixtures/invalid/use-before-declaration/main.zry', import.meta.url), 'utf8');
  const rejected = await readFile(new URL('rejected.zry', fixture), 'utf8');
  const [localsResponse, useBeforeResponse, rejectedResponse] = await exchange([
    analyze(1, [{ path: 'src/math.zry', text: math }, { path: 'src/main.zry', text: main }]),
    analyze(2, [{ path: 'src/main.zry', text: useBefore }]),
    analyze(3, [{ path: 'src/rejected.zry', text: rejected }]),
  ]);
  const localsSnapshot = JSON.parse(await readFile(new URL('locals.snapshot.json', fixture), 'utf8'));
  const useBeforeSnapshot = JSON.parse(await readFile(new URL('use-before.snapshot.json', fixture), 'utf8'));
  const rejectedSnapshot = JSON.parse(await readFile(new URL('rejected.response.json', fixture), 'utf8'));
  assert.deepEqual(localsResponse.result, localsSnapshot);
  assert.deepEqual(useBeforeResponse.result, useBeforeSnapshot);
  assert.deepEqual(rejectedResponse, rejectedSnapshot);
});

test('native root-assignment fixtures are exact TypeScript 6 responses', async () => {
  const fixture = new URL('../../../crates/zryna-frontend/tests/native_parser_v3_assignments/', import.meta.url);
  const main = (await readFile(new URL('main.zry', fixture), 'utf8')).replace(/\r?\n/g, '\r\n');
  const math = await readFile(new URL('math.zry', fixture), 'utf8');
  const constant = await readFile(new URL('const-assignment.zry', fixture), 'utf8');
  const rejected = await readFile(new URL('rejected.zry', fixture), 'utf8');
  const [assignmentResponse, constResponse, rejectedResponse] = await exchange([
    analyze(1, [{ path: 'src/math.zry', text: math }, { path: 'src/main.zry', text: main }]),
    analyze(2, [{ path: 'src/main.zry', text: constant }]),
    analyze(3, [{ path: 'src/rejected.zry', text: rejected }]),
  ]);
  const assignmentSnapshot = JSON.parse(await readFile(new URL('assignments.snapshot.json', fixture), 'utf8'));
  const constSnapshot = JSON.parse(await readFile(new URL('const-assignment.snapshot.json', fixture), 'utf8'));
  const rejectedSnapshot = JSON.parse(await readFile(new URL('rejected.response.json', fixture), 'utf8'));
  assert.deepEqual(assignmentResponse.result, assignmentSnapshot);
  assert.deepEqual(constResponse.result, constSnapshot);
  assert.deepEqual(rejectedResponse, rejectedSnapshot);
});

test('root assignments count toward the pinned worker statement budget', async () => {
  const root = (count) => `export function value(): i32 { ${'value = 1; '.repeat(count)}return 1; }`;
  await assertBudgetBoundary(
    [{ path: 'src/main.zry', text: root(2) }],
    [{ path: 'src/main.zry', text: root(3) }],
    { ZRYNA_TEST_STATEMENTS_PER_FUNCTION: '3' },
    /function exceeds the statement limit/,
  );
});

test('native zero-argument call fixtures are exact TypeScript 6 responses', async () => {
  const fixture = new URL('../../../crates/zryna-frontend/tests/native_parser_v3_calls/', import.meta.url);
  const main = (await readFile(new URL('main.zry', fixture), 'utf8')).replace(/\r?\n/g, '\r\n');
  const math = await readFile(new URL('math.zry', fixture), 'utf8');
  const unresolved = await readFile(new URL('unresolved.zry', fixture), 'utf8');
  const rejected = await readFile(new URL('rejected.zry', fixture), 'utf8');
  const argumented = await readFile(new URL('argumented.zry', fixture), 'utf8');
  const [callsResponse, unresolvedResponse, rejectedResponse, argumentedResponse] = await exchange([
    analyze(1, [{ path: 'src/math.zry', text: math }, { path: 'src/main.zry', text: main }]),
    analyze(2, [{ path: 'src/main.zry', text: unresolved }]),
    analyze(3, [{ path: 'src/rejected.zry', text: rejected }]),
    analyze(4, [{ path: 'src/argumented.zry', text: argumented }]),
  ]);
  const callsSnapshot = JSON.parse(await readFile(new URL('calls.snapshot.json', fixture), 'utf8'));
  const unresolvedSnapshot = JSON.parse(await readFile(new URL('unresolved.snapshot.json', fixture), 'utf8'));
  const rejectedSnapshot = JSON.parse(await readFile(new URL('rejected.response.json', fixture), 'utf8'));
  assert.deepEqual(callsResponse.result, callsSnapshot);
  assert.deepEqual(unresolvedResponse.result, unresolvedSnapshot);
  assert.deepEqual(rejectedResponse, rejectedSnapshot);
  assert.equal(argumentedResponse.error, undefined, JSON.stringify(argumentedResponse.error));
  assert.equal(validateSnapshot(argumentedResponse.result), true, JSON.stringify(validateSnapshot.errors));
  const expressions = argumentedResponse.result.files[0].functions[1].body.expressions;
  assert.equal(expressions.find(({ kind }) => kind.kind === 'call').kind.arguments.length, 1);
});

test('native compact signed-literal fixtures are exact TypeScript 6 responses', async () => {
  const fixture = new URL('../../../crates/zryna-frontend/tests/native_parser_v3_signed/', import.meta.url);
  const main = (await readFile(new URL('main.zry', fixture), 'utf8')).replace(/\r?\n/g, '\r\n');
  const math = await readFile(new URL('math.zry', fixture), 'utf8');
  const expected = JSON.parse(await readFile(new URL('signed.snapshot.json', fixture), 'utf8'));
  const [signed, valueNegation, spacedNegation, commentNegation, negativeZero] = await exchange([
    analyze(1, [{ path: 'src/math.zry', text: math }, { path: 'src/main.zry', text: main }]),
    analyze(2, [{ path: 'src/main.zry', text: 'function f(x: i32): i32 { return -x; }' }]),
    analyze(3, [{ path: 'src/main.zry', text: 'function f(): i32 { return - 1; }' }]),
    analyze(4, [{ path: 'src/main.zry', text: 'function f(): i32 { return -/*gap*/1; }' }]),
    analyze(5, [{ path: 'src/main.zry', text: 'function f(): i32 { return -0; }' }]),
  ]);
  assert.deepEqual(signed.result, expected);
  assert.equal(validateSnapshot(signed.result), true, JSON.stringify(validateSnapshot.errors));
  assert.deepEqual(signed.result.files.map(({ id, path }) => [id, path]), [
    [0, 'src/main.zry'], [1, 'src/math.zry'],
  ]);
  for (const response of [valueNegation, spacedNegation, commentNegation, negativeZero]) {
    assert.equal(response.error, undefined, JSON.stringify(response.error));
    assert.equal(response.result.files[0].functions[0].body.expressions.at(-1).kind.kind, 'negation');
  }
});

test('native identifier-negation fixtures are exact TypeScript 6 responses', async () => {
  const fixture = new URL('../../../crates/zryna-frontend/tests/native_parser_v3_negation/', import.meta.url);
  const main = (await readFile(new URL('main.zry', fixture), 'utf8')).replace(/\r?\n/g, '\r\n');
  const math = await readFile(new URL('math.zry', fixture), 'utf8');
  const rejected = await readFile(new URL('rejected.zry', fixture), 'utf8');
  const expected = JSON.parse(await readFile(new URL('negation.snapshot.json', fixture), 'utf8'));
  const rejectedExpected = JSON.parse(await readFile(new URL('rejected.response.json', fixture), 'utf8'));
  const [negation, rejectedResponse, zero, spaced, commented, call, boolean] = await exchange([
    analyze(1, [{ path: 'src/math.zry', text: math }, { path: 'src/main.zry', text: main }]),
    analyze(2, [{ path: 'src/main.zry', text: rejected }]),
    analyze(3, [{ path: 'src/main.zry', text: 'function f(): i32 { return -0; }' }]),
    analyze(4, [{ path: 'src/main.zry', text: 'function f(): i32 { return - 1; }' }]),
    analyze(5, [{ path: 'src/main.zry', text: 'function f(): i32 { return -/*gap*/1; }' }]),
    analyze(6, [{ path: 'src/main.zry', text: 'function f(): i32 { return -helper(); }' }]),
    analyze(7, [{ path: 'src/main.zry', text: 'function f(): i32 { return -true; }' }]),
  ]);
  assert.deepEqual(negation.result, expected);
  assert.equal(validateSnapshot(negation.result), true, JSON.stringify(validateSnapshot.errors));
  assert.deepEqual(rejectedResponse, rejectedExpected);
  assert.equal(rejectedResponse.error.code, 'ZRYNA-F2002');
  for (const response of [zero, spaced, commented, call, boolean]) {
    assert.equal(response.error, undefined, JSON.stringify(response.error));
    assert.equal(response.result.files[0].functions[0].body.expressions.at(-1).kind.kind, 'negation');
  }
});

test('native numeric-negation fixtures preserve the signed-literal boundary', async () => {
  const fixture = new URL('../../../crates/zryna-frontend/tests/native_parser_v3_numeric_negation/', import.meta.url);
  const main = (await readFile(new URL('main.zry', fixture), 'utf8')).replace(/\r?\n/g, '\r\n');
  const math = await readFile(new URL('math.zry', fixture), 'utf8');
  const rejected = await readFile(new URL('rejected.zry', fixture), 'utf8');
  const expected = JSON.parse(await readFile(new URL('numeric.snapshot.json', fixture), 'utf8'));
  const rejectedExpected = JSON.parse(await readFile(new URL('rejected.response.json', fixture), 'utf8'));
  const analyzeWidth = (id, digitCount) => analyze(id, [{
    path: 'src/main.zry', text: `function f(): i32 { return -${'1'.repeat(digitCount)}; }`,
  }]);
  const [numeric, rejectedResponse, compact, fallback, overflow] = await exchange([
    analyze(1, [{ path: 'src/math.zry', text: math }, { path: 'src/main.zry', text: main }]),
    analyze(2, [{ path: 'src/main.zry', text: rejected }]),
    analyzeWidth(3, 63), analyzeWidth(4, 64), analyzeWidth(5, 65),
  ]);
  assert.deepEqual(numeric.result, expected);
  assert.equal(validateSnapshot(numeric.result), true, JSON.stringify(validateSnapshot.errors));
  assert.deepEqual(rejectedResponse, rejectedExpected);
  assert.equal(rejectedResponse.error.code, 'ZRYNA-F2002');
  const compactNodes = compact.result.files[0].functions[0].body.expressions;
  const fallbackNodes = fallback.result.files[0].functions[0].body.expressions;
  assert.deepEqual(compactNodes.map(({ kind }) => kind.kind), ['i32-literal']);
  assert.deepEqual(fallbackNodes.map(({ kind }) => kind.kind), ['i32-literal', 'negation']);
  assert.equal(fallbackNodes[0].kind.spelling.length, 64);
  assert.equal(overflow.error.code, 'ZRYNA-F2002');
});

test('file IDs and UTF-8 spans remain deterministic for shuffled batches', async () => {
  const prefix = '// 😀\r\n';
  const source = `${prefix}export function value(): i32 { return 1; }`;
  const files = [{ path: 'z.zry', text: source }, { path: 'a.zry', text: source }];
  const [first, second] = await exchange([analyze(1, files), analyze(2, [...files].reverse())]);
  assert.deepEqual(first.result, second.result);
  assert.deepEqual(first.result.files.map((file) => [file.id, file.path]), [[0, 'a.zry'], [1, 'z.zry']]);
  assert.equal(first.result.files[0].functions[0].span.start, Buffer.byteLength(prefix, 'utf8'));
});

test('signed decimal literals remain atomic while value negation is an operator', async () => {
  const text = 'export function value(x: i32): i32 { const minimum: i32 = -2147483648; return -x; }';
  const [response] = await exchange([analyze(1, [{ path: 'signed.zry', text }])]);
  assert.deepEqual(response.result.diagnostics, []);
  const expressions = response.result.files[0].functions[0].body.expressions;
  assert.deepEqual(expressions.map((expression) => expression.kind.kind), [
    'i32-literal', 'reference', 'negation',
  ]);
  assert.equal(expressions[0].kind.spelling, '-2147483648');
  assert.equal(expressions[1].kind.name.text, 'x');
  assert.equal(expressions[2].kind.operand, 1);
});

test('the exact ControlFlowV1 operator inventory is emitted and no wider', async () => {
  const text = [
    'export function operators(a: i32, b: i32): bool {',
    '  const yes: bool = true;',
    '  const add: i32 = a + b;',
    '  const sub: i32 = a - b;',
    '  const mul: i32 = a * b;',
    '  const eq: bool = a === b;',
    '  const ne: bool = a !== b;',
    '  const lt: bool = a < b;',
    '  const le: bool = a <= b;',
    '  const gt: bool = a > b;',
    '  const ge: bool = a >= b;',
    '  return yes;',
    '}',
  ].join('\n');
  const [response] = await exchange([analyze(1, [{ path: 'operators.zry', text }])]);
  assert.deepEqual(response.result.diagnostics, []);
  const kinds = response.result.files[0].functions[0].body.expressions.map((expression) => expression.kind.kind);
  for (const expected of [
    'bool-literal', 'addition', 'subtraction', 'multiplication', 'equal', 'not-equal',
    'less-than', 'less-equal', 'greater-than', 'greater-equal',
  ]) assert.ok(kinds.includes(expected), expected);
});

test('unsupported forms fail the entire request without partial sibling syntax', async () => {
  const cases = [
    'export function value(): i32 { return (1); }',
    'export function value(): i32 { if (true) return 1; return 2; }',
    'export function value(): i32 { let x: i32 = 1; x += 1; return x; }',
    'export function value(): i32 { return object.value; }',
    'export function value(a: i32): i32 { return a + object.value; }',
    'export function value(): i32 { return value(...args); }',
    '#!/usr/bin/env node\nexport function value(): i32 { return 1; }',
  ];
  for (const [index, invalid] of cases.entries()) {
    const text = `import { helper } from "./helper.zry"; export function good(): i32 { return 1; } ${invalid}`;
    const [response, recovery] = await exchange([
      analyze(index, [{ path: `bad/${index}.zry`, text }]),
      { id: 100 + index, method: 'handshake' },
    ]);
    assert.equal(response.result, undefined);
    assert.equal(response.error.code, 'ZRYNA-F2002');
    assert.equal(recovery.result.protocol_version, 3);
  }
});

test('imports are source-faithful and never resolved by the provider', async () => {
  const invalid = [
    'import value from "./value.zry";',
    'import * as value from "./value.zry";',
    'import { value } from "value";',
    'import { value } from "./value.zry"',
    'import { value } from "./value\\u002ezry";',
    'import { value } from "./café.zry";',
  ];
  for (const [index, declaration] of invalid.entries()) {
    const text = `${declaration} export function valid(): i32 { return 1; }`;
    const [response] = await exchange([analyze(index, [{ path: `imports/${index}.zry`, text }])]);
    assert.equal(response.result, undefined);
    assert.equal(response.error.code, 'ZRYNA-F2002');
    assert.ok(!JSON.stringify(response).includes('resolved'));
  }
  const [accepted] = await exchange([
    analyze(99, [{ path: 'colon.zry', text: 'import { value } from "./a:b.zry";' }]),
  ]);
  assert.equal(accepted.error, undefined, JSON.stringify(accepted.error));
  assert.equal(accepted.result.files[0].imports[0].specifier.text, './a:b.zry');
});

test('TypeScript parse recovery fails the entire request', async () => {
  const text = 'import { good } from "./good.zry"; export function good(): i32 { return 1; } export function broken(: i32): i32 { return 2; }';
  const [response] = await exchange([analyze(1, [{ path: 'parse.zry', text }])]);
  assert.equal(response.result, undefined);
  assert.equal(response.error.code, 'ZRYNA-F2002');
  assert.match(response.error.message, /TS[0-9]+/);
});

test('exact and plus-one source budgets recover on the same worker', async () => {
  const [exact, overflow, recovery] = await exchange([
    analyze(1, [{ path: 'exact.zry', text: ' '.repeat(8) }]),
    analyze(2, [{ path: 'overflow.zry', text: ' '.repeat(9) }]),
    { id: 3, method: 'handshake' },
  ], { env: { NODE_ENV: 'test', ZRYNA_TEST_SOURCE_BYTES: '8', ZRYNA_TEST_SOURCE_FILE_BYTES: '9' } });
  assert.equal(exact.error, undefined);
  assert.equal(overflow.error.code, 'ZRYNA-F1002');
  assert.equal(recovery.result.protocol_version, 3);
});

test('transport, response, file-count, and per-file byte boundaries reject plus one and recover', async () => {
  const handshake = JSON.stringify({ id: 1, method: 'handshake' });
  const exactLine = `${handshake}${' '.repeat(64 - Buffer.byteLength(handshake))}`;
  const [requestExact, requestOverflow, requestRecovery] = await exchangeRaw(
    `${exactLine}\n${' '.repeat(65)}\n${handshake}\n`,
    { env: { NODE_ENV: 'test', ZRYNA_TEST_REQUEST_BYTES: '64' } },
  );
  assert.equal(requestExact.result.protocol_version, 3);
  assert.equal(requestOverflow.result, undefined);
  assert.equal(requestOverflow.error.code, 'ZRYNA-F1002');
  assert.equal(requestRecovery.result.protocol_version, 3);

  const handshakeResponse = {
    id: 1,
    result: {
      provider: 'typescript-6',
      provider_version: ts.version,
      protocol_version: 3,
      capabilities: {
        module_resolution: false,
        semantic_diagnostics: false,
        control_flow_v1: true,
      },
    },
  };
  const responseBytes = Buffer.byteLength(JSON.stringify(handshakeResponse));
  const [responseExact, responseOverflow, responseRecovery] = await exchange([
    { id: 1, method: 'handshake' },
    analyze(2, [{ path: 'large.zry', text: 'export function value(): i32 { return 1; }' }]),
    { id: 1, method: 'handshake' },
  ], { env: { NODE_ENV: 'test', ZRYNA_TEST_RESPONSE_BYTES: String(responseBytes) } });
  assert.deepEqual(responseExact, handshakeResponse);
  assert.equal(responseOverflow.result, undefined);
  assert.equal(responseOverflow.error.code, 'ZRYNA-F1002');
  assert.deepEqual(responseRecovery, handshakeResponse);

  const empty = (name) => ({ path: `${name}.zry`, text: '' });
  await assertBudgetBoundary(
    [empty('a'), empty('b')],
    [empty('a'), empty('b'), empty('c')],
    { ZRYNA_TEST_FILES: '2' },
    /source-file limit/,
  );
  await assertBudgetBoundary(
    [{ path: 'exact.zry', text: ' '.repeat(8) }],
    [{ path: 'overflow.zry', text: ' '.repeat(9) }],
    { ZRYNA_TEST_SOURCE_FILE_BYTES: '8', ZRYNA_TEST_SOURCE_BYTES: '9' },
    /source file exceeds the byte limit/,
  );
});

test('import declaration and binding limits reject the first extra item atomically', async () => {
  const importLine = (index, bindings = `name${index}`) =>
    `import { ${bindings} } from "./module${index}.zry";`;
  const file = (name, lines) => ({ path: name, text: `${lines.join('\n')}\n` });
  await assertBudgetBoundary(
    [file('exact.zry', [importLine(0), importLine(1)])],
    [file('overflow.zry', [importLine(0), importLine(1), importLine(2)])],
    { ZRYNA_TEST_IMPORTS_PER_FILE: '2' },
    /module exceeds the import-declaration limit/,
  );
  await assertBudgetBoundary(
    [file('a.zry', [importLine(0)]), file('b.zry', [importLine(1)])],
    [file('a.zry', [importLine(0)]), file('b.zry', [importLine(1)]), file('c.zry', [importLine(2)])],
    { ZRYNA_TEST_IMPORTS_PER_PROJECT: '2' },
    /project exceeds the import-declaration limit/,
  );
  await assertBudgetBoundary(
    [file('exact.zry', [importLine(0, 'a, b')])],
    [file('overflow.zry', [importLine(0, 'a, b, c')])],
    { ZRYNA_TEST_BINDINGS_PER_IMPORT: '2' },
    /import exceeds the imported-name limit/,
  );
  await assertBudgetBoundary(
    [file('exact.zry', [importLine(0), importLine(1)])],
    [file('overflow.zry', [importLine(0), importLine(1), importLine(2)])],
    { ZRYNA_TEST_BINDINGS_PER_PROJECT: '2' },
    /project exceeds the imported-name limit/,
  );
});

test('function and parameter limits enforce exact and plus-one boundaries', async () => {
  const fn = (name, parameters = '') =>
    `export function ${name}(${parameters}): i32 { return 1; }`;
  const file = (name, declarations) => ({ path: name, text: `${declarations.join('\n')}\n` });
  await assertBudgetBoundary(
    [file('exact.zry', [fn('a'), fn('b')])],
    [file('overflow.zry', [fn('a'), fn('b'), fn('c')])],
    { ZRYNA_TEST_FUNCTIONS_PER_FILE: '2' },
    /module exceeds the function limit/,
  );
  await assertBudgetBoundary(
    [file('a.zry', [fn('a')]), file('b.zry', [fn('b')])],
    [file('a.zry', [fn('a')]), file('b.zry', [fn('b')]), file('c.zry', [fn('c')])],
    { ZRYNA_TEST_FUNCTIONS_PER_PROJECT: '2' },
    /project exceeds the function limit/,
  );
  await assertBudgetBoundary(
    [file('exact.zry', [fn('value', 'a: i32, b: i32')])],
    [file('overflow.zry', [fn('value', 'a: i32, b: i32, c: i32')])],
    { ZRYNA_TEST_PARAMETERS_PER_FUNCTION: '2' },
    /function exceeds the parameter limit/,
  );
  await assertBudgetBoundary(
    [file('exact.zry', [fn('a', 'x: i32'), fn('b', 'x: i32')])],
    [file('overflow.zry', [fn('a', 'x: i32'), fn('b', 'x: i32'), fn('c', 'x: i32')])],
    { ZRYNA_TEST_PARAMETERS_PER_PROJECT: '2' },
    /project exceeds the parameter limit/,
  );
});

test('block and statement limits enforce exact and plus-one boundaries', async () => {
  const file = (name, text) => ({ path: name, text });
  const fn = (name, body) => `export function ${name}(): i32 { ${body} }`;
  await assertBudgetBoundary(
    [file('exact.zry', fn('value', '{} return 1;'))],
    [file('overflow.zry', fn('value', '{} {} return 1;'))],
    { ZRYNA_TEST_BLOCKS_PER_FUNCTION: '2' },
    /function exceeds the lexical-block limit/,
  );
  await assertBudgetBoundary(
    [file('exact.zry', `${fn('a', 'return 1;')} ${fn('b', 'return 1;')}`)],
    [file('overflow.zry', `${fn('a', 'return 1;')} ${fn('b', 'return 1;')} ${fn('c', 'return 1;')}`)],
    { ZRYNA_TEST_BLOCKS_PER_PROJECT: '2' },
    /project exceeds the lexical-block limit/,
  );
  await assertBudgetBoundary(
    [file('exact.zry', fn('value', 'return 1; return 2;'))],
    [file('overflow.zry', fn('value', 'return 1; return 2; return 3;'))],
    { ZRYNA_TEST_STATEMENTS_PER_FUNCTION: '2' },
    /function exceeds the statement limit/,
  );
  await assertBudgetBoundary(
    [file('exact.zry', `${fn('a', 'return 1;')} ${fn('b', 'return 1;')}`)],
    [file('overflow.zry', `${fn('a', 'return 1;')} ${fn('b', 'return 1;')} ${fn('c', 'return 1;')}`)],
    { ZRYNA_TEST_STATEMENTS_PER_PROJECT: '2' },
    /project exceeds the statement limit/,
  );
});

test('expression and local limits enforce exact and plus-one boundaries', async () => {
  const file = (name, text) => ({ path: name, text });
  const fn = (name, body) => `export function ${name}(): i32 { ${body} }`;
  await assertBudgetBoundary(
    [file('exact.zry', fn('value', 'return call(1);'))],
    [file('overflow.zry', fn('value', 'return call(1, 2);'))],
    { ZRYNA_TEST_EXPRESSIONS_PER_FUNCTION: '2' },
    /function exceeds the expression limit/,
  );
  await assertBudgetBoundary(
    [file('exact.zry', `${fn('a', 'return 1;')} ${fn('b', 'return 1;')}`)],
    [file('overflow.zry', `${fn('a', 'return 1;')} ${fn('b', 'return 1;')} ${fn('c', 'return 1;')}`)],
    { ZRYNA_TEST_EXPRESSIONS_PER_PROJECT: '2' },
    /project exceeds the expression limit/,
  );
  await assertBudgetBoundary(
    [file('exact.zry', fn('value', 'const a: i32 = 1; const b: i32 = 2; return a;'))],
    [file('overflow.zry', fn('value', 'const a: i32 = 1; const b: i32 = 2; const c: i32 = 3; return a;'))],
    { ZRYNA_TEST_LOCALS_PER_FUNCTION: '2' },
    /function exceeds the local limit/,
  );
  await assertBudgetBoundary(
    [file('exact.zry', `${fn('a', 'const x: i32 = 1; return x;')} ${fn('b', 'const x: i32 = 1; return x;')}`)],
    [file('overflow.zry', `${fn('a', 'const x: i32 = 1; return x;')} ${fn('b', 'const x: i32 = 1; return x;')} ${fn('c', 'const x: i32 = 1; return x;')}`)],
    { ZRYNA_TEST_LOCALS_PER_PROJECT: '2' },
    /project exceeds the local limit/,
  );
});

test('call-argument and nesting limits enforce exact and plus-one boundaries', async () => {
  const file = (name, text) => [{ path: name, text }];
  await assertBudgetBoundary(
    file('exact.zry', 'export function value(): i32 { return call(1, 2); }'),
    file('overflow.zry', 'export function value(): i32 { return call(1, 2, 3); }'),
    { ZRYNA_TEST_CALL_ARGUMENTS: '2' },
    /call exceeds the argument limit/,
  );
  await assertBudgetBoundary(
    file('exact.zry', 'export function value(): i32 { {} return 1; }'),
    file('overflow.zry', 'export function value(): i32 { {{}} return 1; }'),
    { ZRYNA_TEST_NESTING: '2' },
    /source exceeds the nesting limit/,
  );
});

test('malformed requests, duplicate fields, v2 requests, and path collisions are rejected', async () => {
  const responses = await exchange([
    { id: 1, method: 'handshake', extra: true },
    { id: 2, method: 'analyze', params: { schema_version: 2, files: [] } },
    analyze(3, [{ path: 'A.zry', text: '' }, { path: 'a.zry', text: '' }]),
    analyze(4, [{ path: 'CON.zry', text: '' }]),
    { id: -1, method: 'handshake' },
    { id: 5, method: 'analyze' },
    { id: 6, method: 'analyze', params: { schema_version: 3, files: [{ path: 'extra.zry', text: '', extra: true }] } },
    analyze(7, [{ path: 'surrogate.zry', text: '\ud800' }]),
  ]);
  assert.ok(responses.every((response) => response.error.code === 'ZRYNA-F1001'));
  assert.equal(responses[4].id, null);
  const raw = [
    '{"id":8,"method":"handshake","method":"analyze"}',
    '{"id":9,"method":"analyze","params":{"schema_version":3,"schema_version":3,"files":[]}}',
    '{"id":10,"method":"analyze","params":{"schema_version":3,"files":[{"path":"a.zry","path":"b.zry","text":""}]}}',
    '{"id":11,"method":"handshake"}',
  ];
  const duplicateResponses = await exchangeRaw(`${raw.join('\n')}\n`);
  assert.ok(duplicateResponses.slice(0, 3).every((response) => response.error.code === 'ZRYNA-F1001'));
  assert.ok(duplicateResponses.slice(0, 3).every((response) => /duplicate object field/.test(response.error.message)));
  assert.equal(duplicateResponses[3].result.protocol_version, 3);
  const [invalidUtf8, utf8Recovery] = await exchangeRaw(Buffer.concat([
    Buffer.from([0xff, 0x0a]),
    Buffer.from('{"id":12,"method":"handshake"}\n'),
  ]));
  assert.equal(invalidUtf8.error.code, 'ZRYNA-F1001');
  assert.equal(utf8Recovery.result.protocol_version, 3);
});
