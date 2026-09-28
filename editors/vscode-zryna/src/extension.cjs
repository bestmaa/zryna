'use strict';

const vscode = require('vscode');
const { Connection } = require('./connection.cjs');
const { registerRun } = require('./run-command.cjs');
const { configuredInstallation } = require('./installation.cjs');

let active;
let starting = Promise.resolve();
let profileSwitching = Promise.resolve();
let diagnostics;
let globalStatus;
let globalOutput;
let editorProfile = 'i32-v1';
let configurationRevision = 0;

function configuration() {
  const installation = configuredInstallation(vscode);
  if (installation) return installation;
  const config = vscode.workspace.getConfiguration('zryna');
  // Executables are selected only through explicit user configuration, never workspace settings.
  return Object.fromEntries(['serverPath', 'compilerRoot', 'nodePath'].map(key =>
    [key, config.inspect(key)?.globalValue ?? '']));
}

function range(value, document) {
  if (!value || !validPosition(value.start, document) || !validPosition(value.end, document)) {
    throw new Error('Invalid Zryna source range.');
  }
  const result = new vscode.Range(value.start.line, value.start.character, value.end.line, value.end.character);
  if (result.start.line !== value.start.line || result.start.character !== value.start.character) {
    throw new Error('Reversed Zryna source range.');
  }
  return result;
}

function validPosition(position, document) {
  return position && Number.isSafeInteger(position.line) && Number.isSafeInteger(position.character)
    && position.line >= 0 && position.line < document.lineCount && position.character >= 0
    && position.character <= document.lineAt(position.line).text.length;
}

function publish(state, method, params) {
  if (method === 'zryna/publishDiagnostics') {
    if (active !== state || state.profile !== editorProfile
      || !Array.isArray(params?.documents) || params.documents.length > 10000
      || !params.documents.some(item => item?.uri === state.uri && item.version === state.document.version)
      || params.report?.schema_version !== 2 || !Array.isArray(params.report.diagnostics)
      || params.report.diagnostics.length > 10000) return;
    const messages = params.report.diagnostics.filter(item => item?.location?.kind === 'global'
      && item.severity === 'error'
      && typeof item.code === 'string' && item.code.length <= 80
      && typeof item.message === 'string' && item.message.length <= 1000)
      .map(item => item.message.startsWith(item.code) ? item.message : `${item.code}: ${item.message}`);
    state.globalMessages = messages;
    globalOutput.clear();
    if (messages.length) {
      globalOutput.appendLine(`Zryna project diagnostics for ${state.uri} (version ${state.document.version}):`);
      for (const message of messages) globalOutput.appendLine(message);
    }
    updateGlobalStatus();
    return;
  }
  if (method !== 'textDocument/publishDiagnostics' || active !== state
    || params?.uri !== state.uri || params.version !== state.document.version) return;
  if (!Array.isArray(params.diagnostics) || params.diagnostics.length > 10000) return;
  try {
    const values = params.diagnostics.map(item => {
      if (typeof item.message !== 'string') throw new Error('Invalid diagnostic.');
      const diagnostic = new vscode.Diagnostic(range(item.range, state.document), item.message,
        [undefined, vscode.DiagnosticSeverity.Error, vscode.DiagnosticSeverity.Warning,
          vscode.DiagnosticSeverity.Information, vscode.DiagnosticSeverity.Hint][item.severity]
          ?? vscode.DiagnosticSeverity.Error);
      diagnostic.source = 'Zryna';
      diagnostic.code = item.code;
      return diagnostic;
    });
    diagnostics.set(state.document.uri, values);
  } catch { diagnostics.delete(state.document.uri); }
}

function updateGlobalStatus() {
  const messages = active?.globalMessages;
  if (!messages?.length || vscode.window.activeTextEditor?.document !== active.document) {
    globalStatus?.hide();
    return;
  }
  globalStatus.text = `$(error) Zryna: ${messages.length} project error${messages.length === 1 ? '' : 's'}`;
  globalStatus.tooltip = messages.join('\n');
  globalStatus.show();
}

async function connect(document, revision) {
  if (document.isClosed || !vscode.workspace.isTrusted || document.uri.scheme !== 'file' || document.languageId !== 'zryna') {
    throw new Error('Zryna requires a trusted local file workspace.');
  }
  const folder = vscode.workspace.getWorkspaceFolder(document.uri);
  if (!folder) throw new Error('Open the Zryna project folder first.');
  if (active?.uri === document.uri.toString() && active.profile === editorProfile && active.ready
    && !active.connection.closed) return active;
  await disconnect();
  if (revision !== configurationRevision) throw new Error('The Zryna configuration changed before connection.');
  if (document.isClosed || !vscode.workspace.isTrusted || document.uri.scheme !== 'file'
    || document.languageId !== 'zryna'
    || vscode.workspace.getWorkspaceFolder(document.uri)?.uri.toString() !== folder.uri.toString()) {
    throw new Error('The document changed before the Zryna connection was ready.');
  }
  const state = { document, uri: document.uri.toString(), profile: editorProfile };
  const version = document.version;
  const config = configuration();
  const m3 = state.profile === 'data-ownership-v1';
  state.connection = new Connection(m3 ? { ...config, workspaceRoot: folder.uri.fsPath } : config,
    (method, params) => publish(state, method, params), () => {
    diagnostics.delete(document.uri);
    state.globalMessages = [];
    if (active === state) globalOutput.clear();
    updateGlobalStatus();
  });
  active = state;
  try {
    const result = await state.connection.request('initialize', {
      rootUri: folder.uri.toString(), capabilities: { general: { positionEncodings: ['utf-16'] } },
      ...(state.profile !== 'i32-v1' ? { initializationOptions: { zrynaProfile: state.profile } } : {}),
    });
    const cap = result?.capabilities;
    const m2 = state.profile === 'control-flow-v1';
    const analysisProfile = m3 ? 'data-ownership-v1' : m2 ? 'control-flow-v1' : 'scalar-v2';
    const formattingProfile = m3 ? 'data-ownership-format-v1'
      : m2 ? 'control-flow-format-v1' : 'scalar-format-v1';
    if (result?.serverInfo?.name !== 'zryna-language-server' || result.serverInfo.version !== '0.5.0'
      || cap?.experimental?.zrynaInstallationProfile !== 'portable-setup-v1'
      || (config.installed && cap?.experimental?.zrynaSourceCommit !== config.manifest.sourceCommit)
      || cap?.experimental?.zrynaAnalysisProfile !== analysisProfile
      || cap?.experimental?.zrynaFormattingProfile !== formattingProfile
      || cap.positionEncoding !== 'utf-16' || cap.definitionProvider !== !(m2 || m3)
      || !cap.documentFormattingProvider || !cap.documentRangeFormattingProvider) {
      throw new Error('This extension requires a matching Zryna 0.5.0 server and editor profile.');
    }
    if (active !== state || revision !== configurationRevision || editorProfile !== state.profile
      || document.version !== version || document.isClosed || !vscode.workspace.isTrusted
      || document.uri.scheme !== 'file' || document.languageId !== 'zryna'
      || vscode.workspace.getWorkspaceFolder(document.uri)?.uri.toString() !== folder.uri.toString()) {
      throw new Error('The document changed before the Zryna connection was ready.');
    }
    state.definitionProvider = cap.definitionProvider;
    state.connection.notify('initialized', {});
    state.connection.notify('textDocument/didOpen', { textDocument: {
      uri: state.uri, languageId: 'zryna', version: document.version, text: document.getText(),
    } });
    state.ready = true;
    return state;
  } catch (error) {
    state.connection.fail(error);
    if (active === state) active = undefined;
    throw error;
  }
}

function ensure(document) {
  const revision = configurationRevision;
  const next = starting.catch(() => {}).then(() => connect(document, revision));
  starting = next;
  return next;
}

async function query(document, method, params, token) {
  const state = await ensure(document);
  const version = document.version;
  const result = await state.connection.request(method, { textDocument: { uri: state.uri }, ...params }, token);
  if (active !== state || document.version !== version || document.isClosed || token?.isCancellationRequested) {
    throw new Error('The document changed before the Zryna response.');
  }
  return result;
}

async function format(document, selection, options, token) {
  const result = await query(document, selection ? 'textDocument/rangeFormatting' : 'textDocument/formatting', {
    options: { tabSize: options.tabSize, insertSpaces: options.insertSpaces },
    ...(selection ? { range: { start: selection.start, end: selection.end } } : {}),
  }, token);
  if (!Array.isArray(result) || result.length > 10000) throw new Error('Invalid Zryna edits.');
  let previousEnd = 0;
  return result.map(edit => {
    const selected = range(edit.range, document);
    if (typeof edit.newText !== 'string' || (selection && !selection.contains(selected))
      || document.offsetAt(selected.start) < previousEnd) throw new Error('Invalid Zryna edits.');
    previousEnd = document.offsetAt(selected.end);
    return vscode.TextEdit.replace(selected, edit.newText);
  });
}

async function disconnect() {
  const previous = active;
  active = undefined;
  if (previous) {
    diagnostics?.delete(previous.document.uri);
    globalStatus?.hide();
    globalOutput?.clear();
    await previous.connection.stop().catch(() => {});
  }
}

function activate(context) {
  const savedProfile = context.workspaceState?.get('zryna.editorProfile');
  editorProfile = ['control-flow-v1', 'data-ownership-v1'].includes(savedProfile) ? savedProfile : 'i32-v1';
  registerRun(vscode, context);
  diagnostics = vscode.languages.createDiagnosticCollection('zryna');
  globalOutput = vscode.window.createOutputChannel('Zryna Diagnostics');
  globalStatus = vscode.window.createStatusBarItem(vscode.StatusBarAlignment.Left);
  globalStatus.command = 'zryna.showGlobalDiagnostics';
  const selector = { scheme: 'file', language: 'zryna' };
  context.subscriptions.push(diagnostics, globalOutput, globalStatus,
    vscode.commands.registerCommand('zryna.showGlobalDiagnostics', () => globalOutput.show()),
    vscode.languages.registerDocumentFormattingEditProvider(selector, {
      provideDocumentFormattingEdits: (document, options, token) => format(document, null, options, token),
    }),
    vscode.languages.registerDocumentRangeFormattingEditProvider(selector, {
      provideDocumentRangeFormattingEdits: (document, selection, options, token) => format(document, selection, options, token),
    }),
    vscode.languages.registerDefinitionProvider(selector, {
      async provideDefinition(document, position, token) {
        const state = await ensure(document);
        if (!state.definitionProvider) return null;
        const result = await query(document, 'textDocument/definition', { position }, token);
        if (result === null) return null;
        if (result?.uri !== document.uri.toString()) throw new Error('Foreign definition result.');
        return new vscode.Location(document.uri, range(result.range, document));
      },
    }),
    vscode.commands.registerCommand('zryna.selectEditorProfile', selected => {
      const next = profileSwitching.catch(() => {}).then(async () => {
        if (selected === undefined) {
          const choice = await vscode.window.showQuickPick([
            { label: 'Scalar (i32-v1)', profile: 'i32-v1' },
            { label: 'M2 control flow (control-flow-v1)', profile: 'control-flow-v1' },
            { label: 'M3 data ownership (data-ownership-v1)', profile: 'data-ownership-v1' },
          ], { placeHolder: 'Select the Zryna editor profile' });
          if (!choice) return;
          selected = choice.profile;
        }
        if (!['i32-v1', 'control-flow-v1', 'data-ownership-v1'].includes(selected)) {
          throw new Error('Invalid Zryna editor profile.');
        }
        if (selected === editorProfile) {
          const document = vscode.window.activeTextEditor?.document;
          if (document?.languageId === 'zryna') await ensure(document);
          return;
        }
        await context.workspaceState.update('zryna.editorProfile', selected);
        editorProfile = selected;
        await disconnect();
        const document = vscode.window.activeTextEditor?.document;
        if (document?.languageId === 'zryna') await ensure(document);
      });
      profileSwitching = next;
      return next;
    }),
    vscode.workspace.onDidChangeTextDocument(event => {
      if (active?.ready && active.document === event.document && event.contentChanges.length) {
        try {
          active.connection.notify('textDocument/didChange', {
            textDocument: { uri: active.uri, version: event.document.version },
            contentChanges: [{ text: event.document.getText() }],
          });
          diagnostics.delete(event.document.uri);
          active.globalMessages = [];
          globalOutput.clear();
          updateGlobalStatus();
        } catch (error) { active.connection.fail(error); }
      }
    }),
    vscode.workspace.onDidCloseTextDocument(document => {
      if (active?.document === document) void disconnect();
    }),
    vscode.workspace.onDidChangeConfiguration(event => {
      if (!event.affectsConfiguration('zryna')) return;
      const revision = ++configurationRevision;
      void disconnect().then(() => {
        if (revision !== configurationRevision) return;
        const document = vscode.window.activeTextEditor?.document;
        if (document?.languageId === 'zryna' && document.uri.scheme === 'file'
          && !document.isClosed && vscode.workspace.isTrusted
          && vscode.workspace.getWorkspaceFolder(document.uri)) return ensure(document);
      }).catch(() => {});
    }),
    vscode.window.onDidChangeActiveTextEditor(editor => {
      globalStatus.hide();
      if (editor?.document.languageId === 'zryna') void ensure(editor.document).catch(() => {});
      updateGlobalStatus();
    }),
  );
  const document = vscode.window.activeTextEditor?.document;
  if (document?.languageId === 'zryna') {
    void ensure(document).catch(error => vscode.window.showErrorMessage(error.message));
  }
}

async function deactivate() {
  configurationRevision++;
  await disconnect();
}

module.exports = { activate, deactivate };
