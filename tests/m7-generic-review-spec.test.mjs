import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { readFile } from 'node:fs/promises';
import test from 'node:test';
import Ajv from 'ajv';
import ts from '../adapters/typescript-6/node_modules/@typescript/typescript6/lib/typescript.js';

const root = new URL('../', import.meta.url);
const readJson = async (path) => JSON.parse(await readFile(new URL(path, root), 'utf8'));
const fixture = await readJson('spec/language/generic-review-v1-fixtures.json');
const schema = await readJson('spec/language/generic-review-v1.schema.json');
const validate = new Ajv({ strict: true, allErrors: true }).compile(schema);
const hash = (bytes) => createHash('sha256').update(bytes).digest('hex');
const parse = (source) => ts.createSourceFile('case.ts', source, ts.ScriptTarget.Latest, true);

test('review schema is closed and rejects activation, missing evidence and unknown fields', () => {
  assert.equal(validate(fixture), true, JSON.stringify(validate.errors));
  for (const mutate of [
    (value) => { value.status = 'accepted'; },
    (value) => { delete value.moduleCases; },
    (value) => { value.sourceCases[0].runtimePassed = true; },
    (value) => { value.sourceCases[0].code = 'ZRYNA-M7001'; },
    (value) => { value.dependencies[0].path = '../private.md'; },
    (value) => { value.limits.push(value.limits[0]); },
  ]) {
    const changed = structuredClone(fixture);
    mutate(changed);
    assert.equal(validate(changed), false);
  }
  for (const group of [fixture.sourceCases, fixture.moduleCases, fixture.observations]) {
    assert.equal(new Set(group.map(({ id }) => id)).size, group.length);
  }
});

test('pinned TypeScript parser and AST distinguish exact generic spellings without type checking', () => {
  assert.equal(ts.version, '6.0.3');
  for (const entry of fixture.sourceCases) {
    const file = parse(entry.source);
    assert.equal(file.parseDiagnostics.length, entry.parserErrors, entry.id);
    if (entry.primaryToken !== null) {
      let characterStart = -1;
      for (let index = 0; index <= entry.primaryOccurrence; index += 1) {
        characterStart = entry.source.indexOf(entry.primaryToken, characterStart + 1);
        assert.ok(characterStart >= 0, entry.id);
      }
      assert.ok(characterStart >= 0, entry.id);
      const start = Buffer.byteLength(entry.source.slice(0, characterStart));
      const end = start + Buffer.byteLength(entry.primaryToken);
      assert.equal(Buffer.from(entry.source).subarray(start, end).toString(), entry.primaryToken);
    }
  }
  const source = fixture.sourceCases.find(({ id }) => id === 'invalid-application-member');
  assert.deepEqual(parse(source.source).parseDiagnostics.map(({ code }) => code), [1477]);
  const call = parse('Option.some<i32>(7);').statements[0].expression;
  assert.equal(ts.isCallExpression(call), true);
  assert.equal(ts.isPropertyAccessExpression(call.expression), true);
  assert.equal(call.expression.expression.text, 'Option');
  assert.equal(call.expression.name.text, 'some');
  assert.equal(call.typeArguments.length, 1);
  assert.equal(call.typeArguments[0].typeName.text, 'i32');
  const comparison = parse('left < right;').statements[0].expression;
  assert.equal(ts.isBinaryExpression(comparison), true);
  assert.equal(comparison.operatorToken.kind, ts.SyntaxKind.LessThanToken);
  const nested = parse('const x: Option<Option<i32>> = value;').statements[0].declarationList.declarations[0].type;
  assert.equal(nested.typeName.text, 'Option');
  assert.equal(nested.typeArguments[0].typeName.text, 'Option');
  assert.equal(nested.typeArguments[0].typeArguments[0].typeName.text, 'i32');
});

test('cross-module fixed packet distinguishes source visibility from executable exports', () => {
  const entry = fixture.moduleCases[0];
  assert.deepEqual(entry.files.map(({ path }) => path), ['a.zry', 'b.zry', 'main.zry', 'values.zry']);
  for (const file of entry.files) assert.equal(parse(file.source).parseDiagnostics.length, 0, file.path);
  const declarations = parse(entry.files[3].source).statements;
  assert.equal(ts.isInterfaceDeclaration(declarations[0]), true);
  assert.equal(ts.isFunctionDeclaration(declarations[1]), true);
  for (const declaration of declarations) {
    assert.equal(declaration.typeParameters.length, 1);
    assert.ok(declaration.modifiers.some(({ kind }) => kind === ts.SyntaxKind.ExportKeyword));
  }
  // Independently encode authenticated module 3's two declaration kinds.
  const prefix = (tag) => Buffer.from([tag, 3, 0, 0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 1, 0, 0, 0, 1]);
  assert.deepEqual(entry.expectedFunctionKeys, [prefix(0x40).toString('hex')]);
  assert.deepEqual(entry.expectedDataKeys, [prefix(0x12).toString('hex')]);
  assert.deepEqual(entry.expectedExecutableExports, ['a.zry:left', 'b.zry:right', 'main.zry:score']);
  assert.deepEqual(entry.forbiddenEntrypoints, ['values.zry:identity', 'values.zry:identity<i32>']);
  assert.equal(entry.expectedObservation, 'i32:14');
  assert.equal(entry.rejectionCode, 'ZRYNA-M7005');
  // These are fixed #416 obligations; this test does not execute Zryna semantics.
});

test('inherited M3 and composition documents retain their independently pinned bytes', async () => {
  assert.deepEqual(fixture.dependencies.map(({ path }) => path), [
    'spec/language/DATA_OWNERSHIP_V1.md',
    'spec/memory-model/AGGREGATE_LAYOUT_V1.md',
    'spec/abi/OWNERSHIP_RUNTIME_V1.md',
    'spec/language/CROSS_TARGET_PROFILES_V1.md',
  ]);
  for (const entry of fixture.dependencies) {
    assert.equal(hash(await readFile(new URL(entry.path, root))), entry.sha256, entry.path);
  }
});

test('frozen observations retain all variants and failure/borrow cleanup obligations', () => {
  assert.deepEqual(fixture.observations.slice(0, 4).map(({ outcome }) => outcome), ['i32:0', 'i32:7', 'i32:7', 'i32:1']);
  const byId = new Map(fixture.observations.map((entry) => [entry.id, entry]));
  assert.deepEqual(byId.get('constructor-fault').trace, ['release:left', 'release:earlier-root']);
  assert.deepEqual(byId.get('match-arm-fault').trace, ['release:later-local', 'release:message']);
  assert.deepEqual(byId.get('payload-fault').trace, ['release:source']);
  assert.deepEqual(byId.get('borrowed-owner').trace, ['end-loan:payload', 'end-loan:scrutinee', 'release:message']);
  assert.deepEqual(byId.get('inactive-err').trace, []);
  assert.deepEqual(new Set(fixture.observations.filter(({ outcome }) => outcome.startsWith('zryna.trap.')).map(({ outcome }) => outcome)),
    new Set(['zryna.trap.allocation-v1', 'zryna.trap.bounds-v1', 'zryna.trap.capacity-v1', 'zryna.trap.refcount-v1', 'zryna.trap.utf8-v1']));
});
