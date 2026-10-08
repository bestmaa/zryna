'use strict';
const { readFileSync } = require('node:fs');
const { resolve } = require('node:path');
const vm = require('node:vm');

function fixture({ trusted = true, capability, analysisProfile, editResult = [],
  installed = false, sourceCommit = 'a'.repeat(40), serverVersion = '0.5.0', setupFailure = false,
  savedProfile = 'i32-v1', deferInitialize = false,
  text = 'export function f():i32{return 1;}' } = {}) {
  const launched = [];
  const sent = [];
  const requests = [];
  const requestParams = [];
  const providers = {};
  const commands = {};
  const initialized = [];
  const connections = [];
  const published = [];
  const currentDiagnostics = new Map();
  const events = {};
  const output = { lines: [], shown: 0 };
  const status = { visible: false, show() { this.visible = true; }, hide() { this.visible = false; }, dispose() {} };
  const storage = { profile: savedProfile };
  let settingsPrefix = 'trusted';
  let releaseInitialize;
  const initializeGate = deferInitialize ? new Promise(resolveGate => { releaseInitialize = resolveGate; }) : null;
  const document = {
    uri: { scheme: 'file', toString: () => 'file:///project/main.zry' }, languageId: 'zryna', version: 1,
    getText: () => text,
    get lineCount() { return text.split(/\r\n|\n/).length; },
    lineAt: line => ({ text: text.split(/\r\n|\n/)[line] }),
    offsetAt: position => {
      const lines = text.match(/[^\n]*\n|[^\n]+$/g) ?? [];
      return lines.slice(0, position.line).reduce((offset, line) => offset + line.length, 0) + position.character;
    },
  };
  const disposable = () => ({ dispose() {} });
  const vscode = {
    Range: class Range {
      constructor(startLine, startCharacter, endLine, endCharacter) {
        const start = { line: startLine, character: startCharacter };
        const end = { line: endLine, character: endCharacter };
        [this.start, this.end] = compare(start, end) <= 0 ? [start, end] : [end, start];
      }
      contains(value) {
        return compare(this.start, value.start ?? value) <= 0
          && compare(value.end ?? value, this.end) <= 0;
      }
    },
    TextEdit: { replace: (range, newText) => ({ range, newText }) },
    Diagnostic: class Diagnostic {
      constructor(range, message, severity) { Object.assign(this, { range, message, severity }); }
    },
    DiagnosticSeverity: { Error: 0, Warning: 1, Information: 2, Hint: 3 },
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
      createDiagnosticCollection: () => ({ ...disposable(),
        delete(uri) { currentDiagnostics.delete(uri.toString()); },
        set(uri, values) { published.push([uri, values]); currentDiagnostics.set(uri.toString(), values); },
      }),
      registerDocumentFormattingEditProvider: (_, provider) => { providers.format = provider; return disposable(); },
      registerDocumentRangeFormattingEditProvider: (_, provider) => { providers.range = provider; return disposable(); },
      registerDefinitionProvider: (_, provider) => { providers.definition = provider; return disposable(); },
    },
  };
  class Connection {
    constructor(config, onNotification) { launched.push(config); this.onNotification = onNotification; connections.push(this); }
    async request(method, params) {
      requests.push(method);
      requestParams.push({ method, params });
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
  return { document, launched, sent, requests, requestParams, providers, initialized, connections, published, currentDiagnostics, commands, storage, vscode,
    events, output, status,
    releaseInitialize, setSettingsPrefix: value => { settingsPrefix = value; },
    deactivate: sandbox.module.exports.deactivate };
}

function compare(left, right) {
  return left.line - right.line || left.character - right.character;
}

module.exports = { fixture };
