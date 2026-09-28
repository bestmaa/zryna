import { spawnSync } from 'node:child_process';
import { mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { createRequire } from 'node:module';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { canonicalVsix } from './portable-setup/vsix.mjs';

const require = createRequire(import.meta.url);
const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const args = process.argv.slice(2);
if (args.length !== 0 && (args.length !== 2 || args[0] !== '--out')) {
  throw new Error('usage: package-editor.mjs [--out <path>]');
}
const output = args.length ? resolve(args[1]) : join(root, '.zryna/out/zryna-0.5.0.vsix');
const manifest = require.resolve('@vscode/vsce/package.json');
const cli = join(dirname(manifest), JSON.parse(readFileSync(manifest, 'utf8')).bin.vsce);
const inventory = spawnSync(process.execPath, [cli, 'ls', '--no-dependencies'], {
  cwd: join(root, 'editors/vscode-zryna'), encoding: 'utf8', shell: false, windowsHide: true,
  timeout: 30000, maxBuffer: 16384,
});
const expected = ['CHANGELOG.md', 'LICENSE', 'README.md', 'package.json', 'src/connection.cjs', 'src/extension.cjs',
  'src/installation.cjs',
  'src/run-command.cjs', 'src/run-input.cjs', 'src/run-process.cjs', 'src/run-project.cjs', 'syntaxes/zryna.tmLanguage.json'];
if (inventory.error || inventory.status !== 0
  || JSON.stringify(inventory.stdout.trim().split(/\r?\n/).sort()) !== JSON.stringify(expected.sort())) {
  throw new Error('Editor package inventory differs from the reviewed source files.');
}
mkdirSync(dirname(output), { recursive: true });
const result = spawnSync(process.execPath, [cli, 'package', '--no-dependencies', '--no-gitHubIssueLinking', '--out', output], {
  cwd: join(root, 'editors/vscode-zryna'), stdio: 'inherit', shell: false, windowsHide: true,
});
if (result.error) throw result.error;
if (result.status !== 0) throw new Error(`Editor packaging failed: ${result.status}`);
writeFileSync(output, canonicalVsix(readFileSync(output)));
