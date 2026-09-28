'use strict';
const { test } = require('node:test');
const assert = require('node:assert/strict');
const fs = require('node:fs');
const os = require('node:os');
const path = require('node:path');
const { verifyInstallation, configuredInstallation, hash } = require('../src/installation.cjs');

function fixture(t) {
  // The native API expands Windows short-name aliases in hosted temporary directories.
  const parent = fs.realpathSync.native(fs.mkdtempSync(path.join(os.tmpdir(), 'zryna-setup-test-')));
  t.after(() => fs.rmSync(parent, { recursive: true, force: true }));
  const root = path.join(parent, 'setup');
  fs.mkdirSync(root);
  const suffix = process.platform === 'win32' ? '.exe' : '';
  const files = [`bin/zryna-language-server${suffix}`, `compiler/bin/zryna${suffix}`,
    'editor/zryna-0.5.0.vsix', 'README.md', 'LICENSE', 'NOTICE', 'a.txt', 'b.txt', 'c.txt', 'd.txt']
    .map(name => {
      fs.mkdirSync(path.dirname(path.join(root, name)), { recursive: true });
      fs.writeFileSync(path.join(root, name), name);
      return { path: name, size: Buffer.byteLength(name), sha256: hash(Buffer.from(name)) };
    });
  const manifest = { format: 'zryna.portable-setup.v1', candidate: '0.1.0-candidate.3',
    target: process.platform === 'win32' ? 'x86_64-pc-windows-msvc' : 'x86_64-unknown-linux-gnu',
    compilerVersion: '0.2.3', serverVersion: '0.5.0', editorVersion: '0.5.0',
    capability: 'portable-setup-v1', sourceCommit: 'a'.repeat(40), files };
  const write = () => {
    const bytes = Buffer.from(JSON.stringify(manifest));
    fs.writeFileSync(path.join(root, 'setup.json'), bytes);
    return hash(bytes);
  };
  return { root, parent, manifest, write, digest: write() };
}

test('complete setup relocates and derives only fixed executable paths', t => {
  const f = fixture(t);
  const old = verifyInstallation(f.root, f.digest);
  assert.equal(old.installed, true);
  const moved = path.join(f.parent, 'relocated with spaces');
  fs.renameSync(f.root, moved);
  assert.equal(verifyInstallation(moved, f.digest).compilerRoot, path.join(moved, 'compiler'));
});

test('manifest replacement cannot redefine the externally supplied digest', t => {
  const f = fixture(t);
  f.manifest.serverVersion = '0.2.3';
  f.write();
  assert.throws(() => verifyInstallation(f.root, f.digest), /identity/);
  assert.throws(() => verifyInstallation(f.root, f.write()), /identity/);
});

test('mixed candidate, compiler, server and editor identities reject even with a matching digest', t => {
  const f = fixture(t);
  for (const [field, value] of [
    ['candidate', '0.1.0-candidate.1'],
    ['compilerVersion', '0.4.0'],
    ['serverVersion', '0.3.0'],
    ['editorVersion', '0.3.0'],
    ['capability', 'scalar-format-v1'],
  ]) {
    const original = f.manifest[field];
    f.manifest[field] = value;
    assert.throws(() => verifyInstallation(f.root, f.write()), /identity/, field);
    f.manifest[field] = original;
  }
  assert.equal(verifyInstallation(f.root, f.write()).manifest.editorVersion, '0.5.0');
});

test('the exact current VSIX path is required even if an old package is inventoried', t => {
  const f = fixture(t);
  const current = path.join(f.root, 'editor', 'zryna-0.5.0.vsix');
  fs.renameSync(current, path.join(f.root, 'editor', 'zryna-0.3.0.vsix'));
  f.manifest.files.find(file => file.path === 'editor/zryna-0.5.0.vsix').path = 'editor/zryna-0.3.0.vsix';
  assert.throws(() => verifyInstallation(f.root, f.write()), /identity/);
});

test('changed, missing, extra and linked bytes are rejected', t => {
  const f = fixture(t);
  const filename = path.join(f.root, 'a.txt');
  fs.writeFileSync(filename, 'evil!');
  assert.throws(() => verifyInstallation(f.root, f.digest), /identity/);
  fs.unlinkSync(filename);
  assert.throws(() => verifyInstallation(f.root, f.digest), /identity/);
  fs.writeFileSync(filename, 'a.txt');
  fs.writeFileSync(path.join(f.root, 'extra'), 'extra');
  assert.throws(() => verifyInstallation(f.root, f.digest), /identity/);
  fs.unlinkSync(path.join(f.root, 'extra'));
  fs.symlinkSync(f.root, path.join(f.parent, 'linked'), process.platform === 'win32' ? 'junction' : 'dir');
  assert.throws(() => verifyInstallation(path.join(f.parent, 'linked'), f.digest), /identity/);
});

test('malformed inventories and unsupported hosts cannot select executable paths', t => {
  const f = fixture(t);
  for (const value of ['../outside', '/absolute', 'bin/../../outside', 'A.TXT']) {
    f.manifest.files[0].path = value;
    assert.throws(() => verifyInstallation(f.root, f.write()), /identity/);
  }
  assert.throws(() => verifyInstallation(f.root, f.digest, 'darwin'), /identity/);
});

test('workspace settings are ignored and configured invalid installations never fall back', () => {
  const vscode = { workspace: { getConfiguration: () => ({ inspect: () => ({ workspaceValue: '/hostile' }) }) } };
  assert.equal(configuredInstallation(vscode), null);
  vscode.workspace.getConfiguration = () => ({ inspect: () => ({ globalValue: '/missing-installed-setup' }) });
  assert.throws(() => configuredInstallation(vscode));
});
