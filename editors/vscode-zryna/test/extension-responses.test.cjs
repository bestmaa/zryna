'use strict';
const { test } = require('node:test');
const assert = require('node:assert/strict');
const { fixture } = require('./extension-fixture.cjs');

const options = { tabSize: 2, insertSpaces: true };
const span = (startLine, startCharacter, endLine, endCharacter) => ({
  start: { line: startLine, character: startCharacter },
  end: { line: endLine, character: endCharacter },
});
const plain = value => JSON.parse(JSON.stringify(value));
const format = f => f.providers.format.provideDocumentFormattingEdits(f.document, options);
const define = f => f.providers.definition.provideDefinition(f.document, { line: 0, character: 16 });

function publish(f, diagnostics, overrides = {}) {
  f.connections.at(-1).onNotification('textDocument/publishDiagnostics', {
    uri: f.document.uri.toString(), version: f.document.version, diagnostics, ...overrides,
  });
}

test('source diagnostics preserve UTF-16 ranges, severity, code and inert message text', async t => {
  const f = fixture({ text: '// 😀\nexport function f():i32{return missing;}' });
  t.after(() => f.deactivate());
  await format(f);
  publish(f, [
    { range: span(0, 3, 0, 5), message: '$(command:untrusted) 😀', severity: 2, code: 'ZRYNA-TEST' },
    { range: span(1, 31, 1, 38), message: 'Unknown name', severity: 1, code: 'ZRYNA-M2004' },
  ]);
  const values = f.currentDiagnostics.get(f.document.uri.toString());
  assert.equal(values.length, 2);
  assert.equal(values[0].severity, f.vscode.DiagnosticSeverity.Warning);
  assert.equal(values[1].severity, f.vscode.DiagnosticSeverity.Error);
  assert.equal(values[0].source, 'Zryna');
  assert.equal(values[0].code, 'ZRYNA-TEST');
  assert.equal(values[0].message, '$(command:untrusted) 😀');
  assert.deepEqual(plain(values[0].range), span(0, 3, 0, 5));
});

test('source diagnostics recover after edits and ignore old versions and foreign documents', async t => {
  const f = fixture();
  t.after(() => f.deactivate());
  await format(f);
  const diagnostic = { range: span(0, 16, 0, 17), message: 'Unknown name', severity: 1 };
  publish(f, [diagnostic]);
  assert.equal(f.currentDiagnostics.get(f.document.uri.toString()).length, 1);
  f.document.version++;
  f.events.change({ document: f.document, contentChanges: [{}] });
  assert.equal(f.currentDiagnostics.has(f.document.uri.toString()), false);
  const count = f.published.length;
  publish(f, [diagnostic], { version: 1 });
  publish(f, [diagnostic], { uri: 'file:///other/private.zry' });
  assert.equal(f.published.length, count);
  publish(f, [diagnostic]);
  assert.equal(f.currentDiagnostics.get(f.document.uri.toString()).length, 1);
  publish(f, []);
  assert.equal(f.currentDiagnostics.get(f.document.uri.toString()).length, 0);
});

test('invalid source diagnostics clear the collection without publishing partial results', async t => {
  const f = fixture();
  t.after(() => f.deactivate());
  await format(f);
  const valid = { range: span(0, 16, 0, 17), message: 'Unknown name', severity: 1 };
  for (const invalid of [
    { ...valid, range: span(0, 17, 0, 16) },
    { ...valid, range: span(0, 0, 1, 0) },
    { ...valid, range: span(0, 0, 0, 1000) },
    { ...valid, message: null },
  ]) {
    publish(f, [valid]);
    const count = f.published.length;
    publish(f, [valid, invalid]);
    assert.equal(f.published.length, count);
    assert.equal(f.currentDiagnostics.has(f.document.uri.toString()), false);
  }
});

test('document formatting returns validated nonempty edits and forwards options', async t => {
  const edits = [{ range: span(0, 0, 0, 32), newText: 'export function f(): i32 {\n  return 1;\n}\n' }];
  const f = fixture({ editResult: edits });
  t.after(() => f.deactivate());
  assert.deepEqual(plain(await format(f)), edits);
  const request = f.requestParams.at(-1);
  assert.equal(request.method, 'textDocument/formatting');
  assert.deepEqual(plain(request.params), { textDocument: { uri: f.document.uri.toString() }, options });
});

test('range formatting returns multiple contained edits without expanding the selection', async t => {
  const edits = [
    { range: span(0, 18, 0, 19), newText: ': ' },
    { range: span(0, 22, 0, 23), newText: ' {\n  ' },
  ];
  const f = fixture({ editResult: edits });
  t.after(() => f.deactivate());
  const selection = new f.vscode.Range(0, 0, 0, 32);
  const result = await f.providers.range.provideDocumentRangeFormattingEdits(f.document, selection, options);
  assert.deepEqual(plain(result), edits);
  assert.equal(f.requestParams.at(-1).method, 'textDocument/rangeFormatting');
  assert.deepEqual(plain(f.requestParams.at(-1).params.range), span(0, 0, 0, 32));
});

test('document formatting rejects malformed, reversed, out-of-bounds and overlapping edits', async t => {
  let response;
  const f = fixture({ editResult: () => response });
  t.after(() => f.deactivate());
  for (const invalid of [
    null,
    [{ range: span(0, 0, 0, 1), newText: 42 }],
    [{ range: span(0, 2, 0, 1), newText: '' }],
    [{ range: span(0, 0, 1, 0), newText: '' }],
    [{ range: span(0, 0, 0, 1000), newText: '' }],
    [{ range: span(0, 0, 0, 4), newText: '' }, { range: span(0, 3, 0, 5), newText: '' }],
  ]) {
    response = invalid;
    await assert.rejects(format(f), /Invalid Zryna|Reversed Zryna/);
  }
});

test('range formatting rejects edits outside the requested selection', async t => {
  const f = fixture({ editResult: [{ range: span(0, 0, 0, 20), newText: '' }] });
  t.after(() => f.deactivate());
  const selection = new f.vscode.Range(0, 10, 0, 20);
  await assert.rejects(f.providers.range.provideDocumentRangeFormattingEdits(f.document, selection, options), /Invalid Zryna edits/);
});

test('nonempty formatting and definition responses are discarded after a document revision', async t => {
  for (const method of ['document', 'range', 'definition']) {
    const f = fixture({ editResult: document => {
      document.version++;
      return method === 'definition' ? { uri: document.uri.toString(), range: span(0, 16, 0, 17) }
        : [{ range: span(0, 0, 0, 32), newText: 'stale replacement' }];
    } });
    t.after(() => f.deactivate());
    const request = method === 'definition' ? define(f) : method === 'document' ? format(f)
      : f.providers.range.provideDocumentRangeFormattingEdits(f.document, new f.vscode.Range(0, 0, 0, 32), options);
    await assert.rejects(request, /document changed/);
  }
});

test('scalar definitions preserve valid locations and reject foreign or invalid ranges', async t => {
  let response = { uri: 'file:///project/main.zry', range: span(0, 16, 0, 17) };
  const f = fixture({ editResult: () => response });
  t.after(() => f.deactivate());
  const location = await define(f);
  assert.equal(location.uri, f.document.uri);
  assert.deepEqual(plain(location.range), response.range);
  response = { ...response, uri: 'file:///other/private.zry' };
  await assert.rejects(define(f), /Foreign definition/);
  for (const range of [span(0, 17, 0, 16), span(0, 0, 1, 0), span(0, 0, 0, 1000)]) {
    response = { uri: f.document.uri.toString(), range };
    await assert.rejects(define(f), /Invalid Zryna|Reversed Zryna/);
  }
});
