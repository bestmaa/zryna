'use strict';

// Loaded by VS Code's real extension-test host, not by the extension itself.
const assert = require('node:assert/strict');
const fs = require('node:fs/promises');
const vscode = require('vscode');

const scalar = 'export function main(left: i32, right: i32): i32 { return left + right; }\n';
const branch = 'export function main(positive: bool, value: i32): i32 { if (positive) { return value; } else { return -value; } }\n';
const invalid = 'export function main(value: i32): i32 { return value + missing; }\n';
const formatOptions = { tabSize: 2, insertSpaces: true };
const delay = ms => new Promise(resolve => setTimeout(resolve, ms));

async function until(check, description, timeoutMs = 15000) {
  const end = Date.now() + timeoutMs;
  while (Date.now() < end) {
    if (await check()) return;
    await delay(100);
  }
  throw new Error(`Timed out waiting for ${description}.`);
}

async function replace(document, text) {
  const edit = new vscode.WorkspaceEdit();
  edit.replace(document.uri, new vscode.Range(document.positionAt(0), document.positionAt(document.getText().length)), text);
  assert.equal(await vscode.workspace.applyEdit(edit), true);
  assert.equal(document.getText(), text);
}

async function format(document) {
  const edits = await vscode.commands.executeCommand('vscode.executeFormatDocumentProvider', document.uri, formatOptions);
  assert.ok(edits?.length, 'first Format Document must return edits');
  const edit = new vscode.WorkspaceEdit();
  edit.set(document.uri, edits);
  assert.equal(await vscode.workspace.applyEdit(edit), true);
  const once = document.getText();
  assert.notEqual(once, branch);
  const again = await vscode.commands.executeCommand('vscode.executeFormatDocumentProvider', document.uri, formatOptions);
  assert.equal(again?.length ?? 0, 0, 'second Format Document must be idempotent');
  assert.equal(document.getText(), once);
  assert.equal(await document.save(), true);
  return once;
}

async function invoke(document, profile, target, args, expected) {
  await vscode.window.showTextDocument(document);
  const result = await vscode.commands.executeCommand('zryna.run', { profile, name: 'main', target, args });
  assert.ok(result, `real ${target} Run must succeed`);
  assert.equal(result.target, target);
  assert.deepEqual(result.results.map(item => item.outcome), [{ kind: 'returned', type: 'i32', value: expected }]);
  const stat = await fs.stat(result.file);
  assert.ok(stat.size > 0);
  return result;
}

async function run() {
  const config = JSON.parse(await fs.readFile(process.env.ZRYNA_EDITOR_HOST_CONFIG, 'utf8'));
  const result = { passed: false, editorVersion: vscode.version, platform: process.platform, checks: [] };
  const started = Date.now();
  try {
    assert.equal(vscode.workspace.workspaceFolders?.length, 1);
    if (!vscode.workspace.isTrusted) {
      assert.equal(config.prepareTrust, true,
        'Workspace is untrusted. Run once with --prepare-trust and trust the isolated folder.');
      await vscode.commands.executeCommand('workbench.trust.manage');
      await until(() => vscode.workspace.isTrusted, 'manual trust of isolated workspace', 120000);
    }
    assert.equal(vscode.workspace.isTrusted, true);
    result.checks.push('explicit workspace trust');
    const settings = vscode.workspace.getConfiguration('zryna');
    await settings.update('installationPath', config.setup, vscode.ConfigurationTarget.Global);
    await settings.update('installationDigest', config.digest, vscode.ConfigurationTarget.Global);
    const extension = vscode.extensions.getExtension('zryna.zryna');
    assert.ok(extension, 'development extension must load');
    await extension.activate();
    assert.equal(extension.packageJSON.version, '0.5.0');
    result.checks.push('extension activation and verified installation');

    const uri = vscode.Uri.joinPath(vscode.workspace.workspaceFolders[0].uri, 'main.zry');
    await vscode.workspace.fs.writeFile(uri, Buffer.from(scalar));
    const document = await vscode.workspace.openTextDocument(uri);
    await vscode.window.showTextDocument(document);
    await vscode.commands.executeCommand('zryna.selectEditorProfile', 'i32-v1');
    await until(() => vscode.languages.getDiagnostics(uri).length === 0, 'scalar diagnostics');
    const definitions = await vscode.commands.executeCommand('vscode.executeDefinitionProvider',
      uri, document.positionAt(document.getText().indexOf('left +')));
    assert.equal(definitions?.length, 1);
    result.checks.push('scalar profile and definition');

    const scalarRun = await invoke(document, 'i32-v1', 'javascript',
      [{ type: 'i32', value: '13' }, { type: 'i32', value: '-4' }], 9);
    result.checks.push('real scalar JavaScript result');
    await replace(document, branch);
    await vscode.commands.executeCommand('zryna.selectEditorProfile', 'control-flow-v1');
    await until(() => vscode.languages.getDiagnostics(uri).length === 0, 'M2 diagnostics');
    const formatted = await format(document);
    result.checks.push('M2 profile and idempotent formatting');

    const js = await invoke(document, 'control-flow-v1', 'javascript',
      [{ type: 'bool', value: 'true' }, { type: 'i32', value: '13' }], 13);
    await vscode.commands.executeCommand('zryna.openGeneratedJavaScript');
    await until(() => vscode.window.activeTextEditor?.document.uri.fsPath === js.file, 'generated JavaScript editor');
    assert.ok(vscode.window.activeTextEditor.document.getText().includes('export'));
    result.checks.push('real M2 JavaScript result and open output');

    const wasm = await invoke(document, 'control-flow-v1', 'webassembly',
      [{ type: 'bool', value: 'false' }, { type: 'i32', value: '13' }], -13);
    result.checks.push('real M2 WebAssembly result');
    await vscode.window.showTextDocument(document);
    await replace(document, invalid);
    await until(() => vscode.languages.getDiagnostics(uri).some(item => item.code === 'ZRYNA-M2004'), 'invalid M2 diagnostic');
    await replace(document, formatted);
    await until(() => vscode.languages.getDiagnostics(uri).length === 0, 'diagnostic recovery');
    await replace(document, invalid);
    await replace(document, formatted);
    await delay(500);
    assert.equal(vscode.languages.getDiagnostics(uri).length, 0, 'stale diagnostic must not return');
    result.checks.push('invalid-to-valid diagnostics and stale recovery');
    const mathUri = vscode.Uri.joinPath(vscode.workspace.workspaceFolders[0].uri, 'math.zry');
    const m3Uri = vscode.Uri.joinPath(vscode.workspace.workspaceFolders[0].uri, 'm3.zry');
    await vscode.workspace.fs.writeFile(mathUri,
      Buffer.from('export function double(value:i32):i32{return value+value;}'));
    await vscode.workspace.fs.writeFile(m3Uri, Buffer.from(
      'import{double}from"./math.zry";export function score(value:i32):i32{return double(value);}'));
    const m3Document = await vscode.workspace.openTextDocument(m3Uri);
    await vscode.window.showTextDocument(m3Document);
    await vscode.commands.executeCommand('zryna.selectEditorProfile', 'data-ownership-v1');
    await until(() => vscode.languages.getDiagnostics(m3Uri).length === 0, 'M3 diagnostics');
    const expectedM3 = 'import {\n  double\n}\nfrom "./math.zry";\nexport function score(value: i32): i32 {\n  return double(value);\n}\n';
    const newline = m3Document.eol === vscode.EndOfLine.CRLF ? '\r\n' : '\n';
    assert.equal(await format(m3Document), expectedM3.replaceAll('\n', newline));
    result.checks.push('M3 saved-import formatting and idempotence');
    result.outcomes = [scalarRun, js, wasm].map(item => ({ profile: item.selection.profile,
      target: item.target, outcome: item.results[0].outcome }));
    result.passed = true;
  } catch (error) {
    result.trustedAtFailure = vscode.workspace.isTrusted;
    result.error = error.stack ?? String(error);
    throw error;
  } finally {
    result.hostRunMs = Date.now() - started;
    await fs.writeFile(config.resultPath, JSON.stringify(result));
  }
}

module.exports = { run };
