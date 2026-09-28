'use strict';

const fs = require('node:fs');
const path = require('node:path');
const { spawnSync } = require('node:child_process');
function editorCli(editor) {
  const directory = path.dirname(editor);
  const legacy = path.join(directory, 'resources', 'app', 'out', 'cli.js');
  if (fs.existsSync(legacy)) return legacy;
  const candidates = fs.readdirSync(directory, { withFileTypes: true })
    .filter(entry => entry.isDirectory() && /^[a-f0-9]{10,40}$/i.test(entry.name))
    .map(entry => path.join(directory, entry.name, 'resources', 'app', 'out', 'cli.js'))
    .filter(file => fs.existsSync(file));
  if (candidates.length !== 1) throw new Error('Cannot identify a unique VS Code CLI beside the selected executable.');
  return candidates[0];
}

// This entry point belongs to an already authenticated extracted candidate. It never changes
// an existing editor profile or project. Workspace trust remains the editor's own decision.
function main() {
  const { verifyInstallation } = require('./installation.cjs');
  const args = process.argv.slice(2);
  if (![2, 6].includes(args.length) || args[0] !== '--digest'
    || (args.length === 6 && (args[2] !== '--editor' || args[4] !== '--profile'))) {
    throw new Error('Usage: install --digest <reviewed setup.json SHA256> [--editor <Code executable> --profile <new absolute directory>]');
  }
  const root = __dirname;
  const installation = verifyInstallation(root, args[1]);
  if (args.length === 2) {
    console.log(`Verified ${installation.manifest.candidate}; compiler 0.2.3; server/editor 0.5.0.`);
  } else {
    if (![args[3], args[5]].every(value => path.isAbsolute(value))) {
      throw new Error('Select absolute editor and profile paths.');
    }
    const editor = path.resolve(args[3]);
    const profile = path.resolve(args[5]);
    const cli = process.platform === 'win32' ? editorCli(editor) : null;
    if (/\.(cmd|bat)$/i.test(editor) || fs.existsSync(profile)
      || profile === root || profile.startsWith(`${root}${path.sep}`)) {
      throw new Error('Select an absolute editor executable and a new profile outside the installation.');
    }
    const parent = path.dirname(profile);
    if (fs.realpathSync.native(parent).toLowerCase() !== parent.toLowerCase()
      || !fs.statSync(parent).isDirectory()) throw new Error('Profile parent must be an existing ordinary directory.');
    fs.mkdirSync(profile, { mode: 0o700 });
    const userData = path.join(profile, 'data');
    const extensions = path.join(profile, 'extensions');
    fs.mkdirSync(path.join(userData, 'User'), { recursive: true });
    fs.mkdirSync(extensions);
    fs.writeFileSync(path.join(userData, 'User', 'settings.json'), `${JSON.stringify({
      'zryna.installationPath': root, 'zryna.installationDigest': args[1],
      '[zryna]': { 'editor.defaultFormatter': 'zryna.zryna' },
    }, null, 2)}\n`, { flag: 'wx' });
    const executableArgs = ['--user-data-dir', userData, '--extensions-dir', extensions,
      '--install-extension', path.join(root, 'editor', 'zryna-0.5.0.vsix')];
    const electron = process.platform === 'win32'
      ? [cli, ...executableArgs] : executableArgs;
    const result = spawnSync(editor, electron, { shell: false, windowsHide: true, stdio: 'inherit',
      timeout: 120000, env: { ...process.env, ...(process.platform === 'win32' ? { ELECTRON_RUN_AS_NODE: '1' } : {}) } });
    if (result.error || result.status !== 0) throw new Error('Editor installation failed. The new profile was retained for inspection.');
    console.log(`Installed isolated editor profile: ${profile}`);
    console.log('Open your project using the same --user-data-dir and --extensions-dir paths.');
  }
}

if (require.main === module) main();
module.exports = { editorCli };
