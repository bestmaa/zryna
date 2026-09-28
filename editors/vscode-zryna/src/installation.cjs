'use strict';

const fs = require('node:fs');
const path = require('node:path');
const { createHash } = require('node:crypto');

const FORMAT = 'zryna.portable-setup.v1';
const CANDIDATE = '0.1.0-candidate.3';
const hash = bytes => createHash('sha256').update(bytes).digest('hex');
function reject() { throw new Error('Zryna setup identity or files differ. Restore the verified complete candidate.'); }

function ordinary(filename, directory = false) {
  try {
    const stat = fs.lstatSync(filename);
    if (stat.isSymbolicLink() || !(directory ? stat.isDirectory() : stat.isFile())
      || fs.realpathSync.native(filename).toLowerCase() !== path.resolve(filename).toLowerCase()) reject();
    return stat;
  } catch { reject(); }
}

function rootDirectory(root) {
  if (typeof root !== 'string' || !path.isAbsolute(root) || root !== path.resolve(root)) reject();
  let current = path.parse(root).root;
  ordinary(current, true);
  for (const part of root.slice(current.length).split(path.sep).filter(Boolean)) {
    current = path.join(current, part);
    ordinary(current, true);
  }
}

function verifyInstallation(root, digest, platform = process.platform) {
  rootDirectory(root);
  if (!/^[a-f0-9]{64}$/.test(digest ?? '')) reject();
  const manifestPath = path.join(root, 'setup.json');
  if (ordinary(manifestPath).size > 262144) reject();
  const bytes = fs.readFileSync(manifestPath);
  if (hash(bytes) !== digest) reject();
  const manifest = JSON.parse(bytes);
  const target = { win32: 'x86_64-pc-windows-msvc', linux: 'x86_64-unknown-linux-gnu' }[platform];
  if (process.arch !== 'x64' || !target || manifest.format !== FORMAT || manifest.candidate !== CANDIDATE
    || manifest.target !== target || manifest.compilerVersion !== '0.2.3' || manifest.serverVersion !== '0.5.0'
    || manifest.editorVersion !== '0.5.0' || manifest.capability !== 'portable-setup-v1'
    || !/^[a-f0-9]{40}$/.test(manifest.sourceCommit ?? '') || !Array.isArray(manifest.files)
    || manifest.files.length < 10 || manifest.files.length > 1024) reject();
  const expected = new Map();
  let total = 0;
  for (const file of manifest.files) {
    if (typeof file.path !== 'string' || file.path.length > 240
      || !file.path.split('/').every(part => /^[A-Za-z0-9@][A-Za-z0-9@+._-]*$/.test(part)
        && part !== '.' && part !== '..' && !part.endsWith('.'))
      || file.path === 'setup.json' || expected.has(file.path.toLowerCase())
      || !Number.isSafeInteger(file.size) || file.size < 1 || file.size > 256 * 1024 * 1024
      || !/^[a-f0-9]{64}$/.test(file.sha256 ?? '')) reject();
    total += file.size;
    if (total > 768 * 1024 * 1024) reject();
    expected.set(file.path.toLowerCase(), file);
  }
  let count = 0;
  const walk = (relative, depth = 0) => {
    if (depth > 16) reject();
    for (const name of fs.readdirSync(path.join(root, relative))) {
      const logical = relative ? `${relative}/${name}` : name;
      const filename = path.join(root, logical);
      const stat = fs.lstatSync(filename);
      if (++count > 4096 || stat.isSymbolicLink()) reject();
      if (stat.isDirectory()) {
        ordinary(filename, true);
        if (![...expected.values()].some(file => file.path.startsWith(`${logical}/`))) reject();
        walk(logical, depth + 1);
      } else if (logical !== 'setup.json') {
        ordinary(filename);
        const file = expected.get(logical.toLowerCase());
        if (!file || file.path !== logical || stat.size !== file.size || hash(fs.readFileSync(filename)) !== file.sha256) reject();
        expected.delete(logical.toLowerCase());
      }
    }
  };
  walk('');
  if (expected.size !== 0) reject();
  const suffix = platform === 'win32' ? '.exe' : '';
  const compilerRoot = path.join(root, 'compiler');
  const serverPath = path.join(root, 'bin', `zryna-language-server${suffix}`);
  const compilerPath = path.join(compilerRoot, 'bin', `zryna${suffix}`);
  const vsix = path.join(root, 'editor', 'zryna-0.5.0.vsix');
  for (const filename of [serverPath, compilerPath, vsix]) ordinary(filename);
  return { serverPath, compilerPath, compilerRoot, installed: true, manifest };
}

function configuredInstallation(vscode) {
  const config = vscode.workspace.getConfiguration('zryna');
  const root = config.inspect('installationPath')?.globalValue;
  if (!root) return null;
  return verifyInstallation(root, config.inspect('installationDigest')?.globalValue);
}

module.exports = { verifyInstallation, configuredInstallation, hash };
