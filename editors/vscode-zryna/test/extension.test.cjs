'use strict';
const { test } = require('node:test');
const assert = require('node:assert/strict');
const { readFileSync } = require('node:fs');
const { resolve } = require('node:path');
const vm = require('node:vm');

function fixture({ trusted = true, capability, analysisProfile, editResult = [],
  installed = false, sourceCommit = 'a'.repeat(40), serverVersion = '0.5.0', setupFailure = false,
  savedProfile = 'i32-v1', deferInitialize = false } = {}) {
  const launched = [];
  const sent = [];
  const requests = [];
  const providers = {};
  const commands = {};
  const initialized = [];
  const connections = [];
  const published = [];
  const events = {};
  const output = { lines: [], shown: 0 };
  const status = { visible: false, show() { this.visible = true; }, hide() { this.visible = false; }, dispose() {} };
  const storage = { profile: savedProfile };
  let settingsPrefix = 'trusted';
  let releaseInitialize;
  const initializeGate = deferInitialize ? new Promise(resolveGate => { releaseInitialize = resolveGate; }) : null;
  const document = {
    uri: { scheme: 'file', toString: () => 'file:///project/main.zry' }, languageId: 'zryna', version: 1,
    getText: () => 'export function f():i32{return 1;}', lineCount: 1,
    lineAt: () => ({ text: 'export function f():i32{return 1;}' }),
  };
  const disposable = () => ({ dispose() {} });
  const vscode = {
    Range: class Range {
      constructor(startLine, startCharacter, endLine, endCharacter) {
        this.start = { line: startLine, character: startCharacter };
        this.end = { line: endLine, character: endCharacter };
      }
    },
    Location: class Location { constructor(uri, range) { this.uri = uri; this.range = range; } },
    workspace: {
      isTrusted: trusted,
      getWorkspaceFolder: () => ({ uri: { toString: () => 'file:///project', fsPath: resolve('trusted-project') } }),
      getConfiguration: () => ({ inspect: key => ({ globalValue: resolve(`${settingsPrefix}-${key}`), workspaceValue: 'hostile-workspace-command' }) }),
      onDidChangeTextDocument: callback => { events.change = callback; return disposable(); },
      onDidCloseTextDocument: disposable,
      onDidChangeConfiguration: callback => { events.configuration = callback; return disposable(); },
    },
    window: {
      onDidChangeActiveTextEditor: callback => { events.editor = callback; return disposable(); },
      showQuickPick: async items => items[1],
      createStatusBarItem: () => status,
      createOutputChannel: () => ({ ...disposable(), appendLine(line) { output.lines.push(line); },
        clear() { output.lines = []; }, show() { output.shown++; } }),
    },
    StatusBarAlignment: { Left: 1 },
    commands: { registerCommand: (name, callback) => { commands[name] = callback; return disposable(); } },
    languages: {
      createDiagnosticCollection: () => ({ ...disposable(), delete() {}, set(uri, values) { published.push([uri, values]); } }),
      registerDocumentFormattingEditProvider: (_, provider) => { providers.format = provider; return disposable(); },
      registerDocumentRangeFormattingEditProvider: (_, provider) => { providers.range = provider; return disposable(); },
      registerDefinitionProvider: (_, provider) => { providers.definition = provider; return disposable(); },
    },
  };
  class Connection {
    constructor(config, onNotification) { launched.push(config); this.onNotification = onNotification; connections.push(this); }
    async request(method, params) {
      requests.push(method);
      if (method === 'initialize') {
        initialized.push(params);
        if (initializeGate) await initializeGate;
        const m2 = params.initializationOptions?.zrynaProfile === 'control-flow-v1';
        const m3 = params.initializationOptions?.zrynaProfile === 'data-ownership-v1';
        return {
        serverInfo: { name: 'zryna-language-server', version: serverVersion },
        capabilities: { positionEncoding: 'utf-16', definitionProvider: !(m2 || m3), documentFormattingProvider: true,
          documentRangeFormattingProvider: true, experimental: { zrynaAnalysisProfile: analysisProfile ?? (m3 ? 'data-ownership-v1' : m2 ? 'control-flow-v1' : 'scalar-v2'),
            zrynaFormattingProfile: capability ?? (m3 ? 'data-ownership-format-v1' : m2 ? 'control-flow-format-v1' : 'scalar-format-v1'),
            zrynaInstallationProfile: 'portable-setup-v1', zrynaSourceCommit: sourceCommit } },
        };
      }
      return typeof editResult === 'function' ? editResult(document, method) : editResult;
    }
    notify(method, params) { sent.push({ method, params }); }
    fail() { this.closed = true; }
    async stop() { this.closed = true; }
  }
  const sandbox = { module: { exports: {} }, require: name => name === 'vscode' ? vscode
    : name === './installation.cjs' ? { configuredInstallation: () => {
      if (setupFailure) throw new Error('Setup identity differs.');
      return installed ? { installed: true, manifest: { sourceCommit: 'a'.repeat(40) } } : null;
    } }
    : name === './run-command.cjs' ? { registerRun() {} } : { Connection } };
  vm.runInNewContext(readFileSync(resolve(__dirname, '../src/extension.cjs'), 'utf8'), sandbox);
  sandbox.module.exports.activate({ subscriptions: [], workspaceState: {
    get: () => storage.profile, async update(_, value) { storage.profile = value; },
  } });
  return { document, launched, sent, requests, providers, initialized, connections, published, commands, storage, vscode,
    events, output, status,
    releaseInitialize, setSettingsPrefix: value => { settingsPrefix = value; },
    deactivate: sandbox.module.exports.deactivate };
}

test('untrusted, virtual and closed documents cannot launch a compiler', async () => {
  for (const kind of ['untrusted', 'virtual', 'closed']) {
    const f = fixture({ trusted: kind !== 'untrusted' });
    if (kind === 'virtual') f.document.uri.scheme = 'https';
    if (kind === 'closed') f.document.isClosed = true;
    await assert.rejects(f.providers.format.provideDocumentFormattingEdits(f.document, {}), /trusted local/);
    assert.equal(f.launched.length, 0);
  }
});

test('workspace executable overrides are ignored and incompatible servers receive no document contents', async () => {
  const old = fixture({ capability: 'old-scalar-server' });
  await assert.rejects(old.providers.format.provideDocumentFormattingEdits(old.document, {}), /matching Zryna 0.5.0/);
  assert.equal(old.sent.length, 0);
  assert.equal(old.launched[0].serverPath, resolve('trusted-serverPath'));
  assert.equal(old.initialized[0].initializationOptions, undefined);
  await old.deactivate();
});

test('matching server gets only the explicit document and stale versions cannot return edits', async () => {
  const f = fixture({ editResult: document => { document.version++; return []; } });
  await assert.rejects(f.providers.format.provideDocumentFormattingEdits(f.document, {}), /document changed/);
  assert.deepEqual(f.sent.map(item => item.method), ['initialized', 'textDocument/didOpen']);
  assert.equal(f.sent[1].params.textDocument.text, f.document.getText());
  await f.deactivate();
});

test('settings changes reconnect the active document and ignore unrelated changes', async () => {
  const f = fixture();
  f.vscode.window.activeTextEditor = { document: f.document };
  await f.providers.format.provideDocumentFormattingEdits(f.document, {});
  const first = f.connections[0];
  f.events.configuration({ affectsConfiguration: section => section === 'other' });
  await new Promise(resolveImmediate => setImmediate(resolveImmediate));
  assert.equal(f.launched.length, 1);
  assert.equal(first.closed, undefined);
  f.setSettingsPrefix('changed');
  f.events.configuration({ affectsConfiguration: section => section === 'zryna' });
  await new Promise(resolveImmediate => setImmediate(resolveImmediate));
  assert.equal(first.closed, true);
  assert.equal(f.launched.length, 2);
  assert.equal(f.launched[1].serverPath, resolve('changed-serverPath'));
  assert.equal(f.sent.filter(item => item.method === 'textDocument/didOpen').length, 2);
  await f.deactivate();
});

test('rapid settings changes suppress stale reconnects during an unfinished handshake', async () => {
  const f = fixture({ deferInitialize: true });
  f.vscode.window.activeTextEditor = { document: f.document };
  const first = f.providers.format.provideDocumentFormattingEdits(f.document, {});
  await new Promise(resolveImmediate => setImmediate(resolveImmediate));
  assert.equal(f.launched.length, 1);
  f.events.configuration({ affectsConfiguration: () => true });
  f.events.configuration({ affectsConfiguration: () => true });
  f.releaseInitialize();
  await assert.rejects(first, /document changed/);
  await new Promise(resolveImmediate => setImmediate(resolveImmediate));
  assert.equal(f.launched.length, 2);
  assert.equal(f.sent.filter(item => item.method === 'textDocument/didOpen').length, 1);
  await f.deactivate();
});

test('deactivation cancels a settings reconnect queued behind an unfinished handshake', async () => {
  const f = fixture({ deferInitialize: true });
  f.vscode.window.activeTextEditor = { document: f.document };
  const first = f.providers.format.provideDocumentFormattingEdits(f.document, {});
  await new Promise(resolveImmediate => setImmediate(resolveImmediate));
  f.events.configuration({ affectsConfiguration: () => true });
  await new Promise(resolveImmediate => setImmediate(resolveImmediate));
  await f.deactivate();
  f.releaseInitialize();
  await assert.rejects(first, /document changed/);
  await new Promise(resolveImmediate => setImmediate(resolveImmediate));
  assert.equal(f.launched.length, 1);
  assert.equal(f.sent.length, 0);
});

test('revoked workspace trust prevents an unfinished handshake from receiving source', async () => {
  const f = fixture({ deferInitialize: true });
  const first = f.providers.format.provideDocumentFormattingEdits(f.document, {});
  await new Promise(resolveImmediate => setImmediate(resolveImmediate));
  f.vscode.workspace.isTrusted = false;
  f.releaseInitialize();
  await assert.rejects(first, /document changed/);
  assert.equal(f.sent.length, 0);
  await f.deactivate();
});

test('settings changes do not launch for ineligible documents or leak failed handshakes', async () => {
  for (const kind of ['untrusted', 'virtual', 'closed']) {
    const f = fixture({ trusted: kind !== 'untrusted' });
    if (kind === 'virtual') f.document.uri.scheme = 'https';
    if (kind === 'closed') f.document.isClosed = true;
    f.vscode.window.activeTextEditor = { document: f.document };
    f.events.configuration({ affectsConfiguration: () => true });
    await new Promise(resolveImmediate => setImmediate(resolveImmediate));
    assert.equal(f.launched.length, 0);
    await f.deactivate();
  }
  const mismatch = fixture({ capability: 'wrong-format' });
  mismatch.vscode.window.activeTextEditor = { document: mismatch.document };
  mismatch.events.configuration({ affectsConfiguration: () => true });
  await new Promise(resolveImmediate => setImmediate(resolveImmediate));
  assert.equal(mismatch.launched.length, 1);
  assert.equal(mismatch.sent.length, 0);
  await mismatch.deactivate();
});

test('foreign definition locations cannot redirect the editor', async () => {
  const f = fixture({ editResult: { uri: 'file:///other/private.zry', range: {} } });
  await assert.rejects(f.providers.definition.provideDefinition(f.document, { line: 0, character: 0 }), /Foreign/);
  await f.deactivate();
});

test('installed server revision/version mismatches receive no source and invalid setups never launch', async () => {
  for (const option of [{ sourceCommit: 'b'.repeat(40) }, { serverVersion: '0.3.0' }, { setupFailure: true }]) {
    const f = fixture({ installed: true, ...option });
    await assert.rejects(f.providers.format.provideDocumentFormattingEdits(f.document, {}));
    assert.equal(f.sent.length, 0);
    if (option.setupFailure) assert.equal(f.launched.length, 0);
    await f.deactivate();
  }
});

test('explicit M2 selection negotiates before source and preserves the scalar default', async () => {
  const f = fixture();
  await f.providers.format.provideDocumentFormattingEdits(f.document, {});
  assert.equal(f.initialized[0].initializationOptions, undefined);
  assert.equal(f.storage.profile, 'i32-v1');
  f.vscode.window.activeTextEditor = { document: f.document };
  await f.commands['zryna.selectEditorProfile']();
  assert.equal(f.storage.profile, 'control-flow-v1');
  assert.equal(f.initialized[1].initializationOptions.zrynaProfile, 'control-flow-v1');
  assert.equal(f.sent.filter(item => item.method === 'textDocument/didOpen').length, 2);
  assert.equal(await f.providers.definition.provideDefinition(f.document, { line: 0, character: 0 }), null);
  await assert.rejects(f.commands['zryna.selectEditorProfile']('unknown'), /Invalid Zryna editor profile/);
  assert.equal(f.storage.profile, 'control-flow-v1');
  await f.deactivate();
});

test('M2 capability mismatch sends no source and does not return edits', async () => {
  for (const option of [
    { capability: 'scalar-format-v1' }, { analysisProfile: 'scalar-v2' },
    { serverVersion: '0.3.0' }, { sourceCommit: 'b'.repeat(40), installed: true },
  ]) {
    const f = fixture({ savedProfile: 'control-flow-v1', ...option });
    await assert.rejects(f.providers.format.provideDocumentFormattingEdits(f.document, {}), /matching Zryna 0.5.0/);
    assert.equal(f.sent.length, 0);
    assert.equal(f.initialized[0].initializationOptions.zrynaProfile, 'control-flow-v1');
    await f.deactivate();
  }
});

test('M3 selection passes only the trusted folder at startup and checks capability before source', async () => {
  const f = fixture({ savedProfile: 'data-ownership-v1' });
  await f.providers.format.provideDocumentFormattingEdits(f.document, {});
  assert.equal(f.launched[0].workspaceRoot, resolve('trusted-project'));
  assert.equal(f.initialized[0].rootUri, 'file:///project');
  assert.equal(f.initialized[0].initializationOptions.zrynaProfile, 'data-ownership-v1');
  assert.equal(await f.providers.definition.provideDefinition(f.document, { line: 0, character: 0 }), null);
  await f.deactivate();

  for (const option of [{ capability: 'control-flow-format-v1' }, { serverVersion: '0.4.0' }]) {
    const incompatible = fixture({ savedProfile: 'data-ownership-v1', ...option });
    await assert.rejects(incompatible.providers.format.provideDocumentFormattingEdits(
      incompatible.document, {}), /matching Zryna 0.5.0/);
    assert.equal(incompatible.sent.length, 0);
    await incompatible.deactivate();
  }
});

test('same profile selection retries failed handshakes and picker cancellation preserves state', async () => {
  const f = fixture({ savedProfile: 'control-flow-v1', capability: 'wrong-format' });
  f.vscode.window.activeTextEditor = { document: f.document };
  await assert.rejects(f.commands['zryna.selectEditorProfile']('control-flow-v1'), /matching Zryna 0.5.0/);
  assert.equal(f.sent.length, 0);
  assert.equal(f.initialized.length, 1);
  f.vscode.window.showQuickPick = async () => undefined;
  await f.commands['zryna.selectEditorProfile']();
  assert.equal(f.storage.profile, 'control-flow-v1');
  assert.equal(f.initialized.length, 1);
  await f.deactivate();
});

test('profile switch suppresses old diagnostics and restores scalar definition', async () => {
  const f = fixture({ editResult: (_, method) => method === 'textDocument/definition'
    ? { uri: 'file:///project/main.zry', range: {
      start: { line: 0, character: 16 }, end: { line: 0, character: 17 },
    } } : [] });
  f.vscode.window.activeTextEditor = { document: f.document };
  await f.providers.format.provideDocumentFormattingEdits(f.document, {});
  const scalar = f.connections[0];
  await f.commands['zryna.selectEditorProfile']('control-flow-v1');
  scalar.onNotification('textDocument/publishDiagnostics', {
    uri: f.document.uri.toString(), version: 1, diagnostics: [],
  });
  assert.equal(f.published.length, 0);
  assert.equal(await f.providers.definition.provideDefinition(f.document, { line: 0, character: 16 }), null);
  assert.equal(f.requests.filter(method => method === 'textDocument/definition').length, 0);
  await f.commands['zryna.selectEditorProfile']('i32-v1');
  const location = await f.providers.definition.provideDefinition(f.document, { line: 0, character: 16 });
  assert.equal(location.range.start.character, 16);
  assert.equal(f.requests.filter(method => method === 'textDocument/definition').length, 1);
  await f.deactivate();
});

test('global errors show project status without inventing a source range and clear on revision or profile change', async () => {
  const f = fixture({ savedProfile: 'control-flow-v1' });
  f.vscode.window.activeTextEditor = { document: f.document };
  await f.providers.format.provideDocumentFormattingEdits(f.document, {});
  const m2 = f.connections[0];
  const report = version => ({ documents: [{ uri: f.document.uri.toString(), version }],
    report: { schema_version: 2, diagnostics: [{ code: 'ZRYNA-F1103', severity: 'error',
      message: 'ZRYNA-F1103: frontend worker rejected a protocol request', location: { kind: 'global' } }] } });
  m2.onNotification('zryna/publishDiagnostics', report(1));
  m2.onNotification('textDocument/publishDiagnostics', {
    uri: f.document.uri.toString(), version: 1, diagnostics: [],
  });
  assert.equal(f.status.visible, true);
  assert.match(f.status.text, /Zryna: 1 project error/);
  assert.match(f.status.tooltip, /ZRYNA-F1103/);
  assert.equal(f.published.at(-1)[1].length, 0);
  f.commands['zryna.showGlobalDiagnostics']();
  assert.equal(f.output.shown, 1);
  assert.equal(f.output.lines.filter(line => line.includes('ZRYNA-F1103')).length, 1);
  f.document.version = 2;
  f.events.change({ document: f.document, contentChanges: [{}] });
  assert.equal(f.status.visible, false);
  assert.equal(f.output.lines.length, 0);
  m2.onNotification('zryna/publishDiagnostics', report(1));
  assert.equal(f.status.visible, false);
  m2.onNotification('zryna/publishDiagnostics', report(2));
  assert.equal(f.status.visible, true);
  await f.commands['zryna.selectEditorProfile']('i32-v1');
  assert.equal(f.status.visible, false);
  m2.onNotification('zryna/publishDiagnostics', report(2));
  assert.equal(f.status.visible, false);
  await f.deactivate();
});

test('rapid profile selections settle in command order without stale source admission', async () => {
  const f = fixture();
  f.vscode.window.activeTextEditor = { document: f.document };
  await f.providers.format.provideDocumentFormattingEdits(f.document, {});
  const first = f.commands['zryna.selectEditorProfile']('control-flow-v1');
  const second = f.commands['zryna.selectEditorProfile']('i32-v1');
  await Promise.all([first, second]);
  assert.equal(f.storage.profile, 'i32-v1');
  assert.deepEqual(f.initialized.map(item => item.initializationOptions?.zrynaProfile),
    [undefined, 'control-flow-v1', undefined]);
  assert.equal(f.sent.filter(item => item.method === 'textDocument/didOpen').length, 3);
  await f.deactivate();
});

test('same profile command waits for an unfinished handshake before reporting success', async () => {
  const f = fixture({ deferInitialize: true });
  f.vscode.window.activeTextEditor = { document: f.document };
  const formatting = f.providers.format.provideDocumentFormattingEdits(f.document, {});
  let selected = false;
  const selection = f.commands['zryna.selectEditorProfile']('i32-v1').then(() => { selected = true; });
  await new Promise(resolveImmediate => setImmediate(resolveImmediate));
  assert.equal(selected, false);
  assert.equal(f.sent.length, 0);
  f.releaseInitialize();
  await Promise.all([formatting, selection]);
  assert.equal(f.sent.filter(item => item.method === 'textDocument/didOpen').length, 1);
  await f.deactivate();
});
