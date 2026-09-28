import { spawnSync } from 'node:child_process';
import { mkdirSync, readFileSync, writeFileSync, copyFileSync } from 'node:fs';
import { resolve, join, dirname, relative, isAbsolute } from 'node:path';
import { fileURLToPath } from 'node:url';
import { createHash } from 'node:crypto';

const source = resolve(dirname(fileURLToPath(import.meta.url)), '../..');
const output = resolve(process.argv[2] ?? '');
const outputRelative = relative(source, output);
if (process.argv.length !== 3 || !outputRelative
  || (!isAbsolute(outputRelative) && outputRelative.split(/[\\/]/)[0] !== '..')) {
  throw new Error('External evidence directory required.');
}
const digest = file => createHash('sha256').update(readFileSync(file)).digest('hex');
const target = process.platform === 'win32' ? 'x86_64-pc-windows-msvc' : 'x86_64-unknown-linux-gnu';
const suffix = process.platform === 'win32' ? '.exe' : '';
function run(executable, args, cwd = source) {
  const result = spawnSync(executable, args, { cwd, encoding: 'utf8', shell: false,
    windowsHide: true, maxBuffer: 32 * 1024 * 1024 });
  if (result.error || result.status !== 0) throw new Error(`${executable} failed:\n${result.stdout}\n${result.stderr}`);
  return result.stdout.trim();
}
mkdirSync(output, { recursive: false });
const head = run('git', ['rev-parse', 'HEAD']);
const second = join(output, 'replica-source');
run('git', ['clone', '--no-hardlinks', '--no-checkout', source, second]);
run('git', ['checkout', '--detach', head], second);
const binaries = [];
for (const [number, cwd] of [[1, source], [2, second]]) {
  const build = join(output, `build-${number}`);
  const log = run(process.execPath, [join(cwd, 'scripts/portable-setup/build-server.mjs'),
    '--source', cwd, '--output', build], cwd);
  writeFileSync(join(output, `build-${number}.log`), log);
  binaries.push(join(build, 'target', target, 'release', `zryna-language-server${suffix}`));
}
if (digest(binaries[0]) !== digest(binaries[1])) throw new Error('Server replicas differ.');
const vsix = join(output, 'zryna-0.5.0.vsix');
run(process.execPath, ['scripts/package-editor.mjs', '--out', vsix]);
const repeated = join(output, 'zryna-repeated.vsix');
run(process.execPath, ['scripts/package-editor.mjs', '--out', repeated]);
if (digest(vsix) !== digest(repeated)) throw new Error('VSIX replicas differ.');
const assets = join(output, 'release-assets');
run('gh', ['release', 'download', 'v0.2.3', '--repo', 'zryna/zryna', '--dir', assets]);
const cosign = process.env.ZRYNA_COSIGN;
if (!cosign) throw new Error('ZRYNA_COSIGN must identify the installed verifier.');
const packaged = join(output, 'packaged');
run(process.execPath, ['scripts/portable-setup/build.mjs', '--release', assets,
  '--cosign', cosign, '--server', binaries[0], '--vsix', vsix, '--output', packaged]);
const receipt = JSON.parse(readFileSync(join(packaged, 'candidate-receipt.json')));
const distribution = join(output, 'handoff');
mkdirSync(distribution);
copyFileSync(join(packaged, receipt.archive.filename), join(distribution, receipt.archive.filename));
copyFileSync(join(packaged, 'candidate-receipt.json'), join(distribution, 'candidate-receipt.json'));
writeFileSync(join(distribution, 'reproduction.json'), `${JSON.stringify({
  sourceCommit: head, target, serverReplicas: binaries.map(digest), vsixReplicas: [digest(vsix), digest(repeated)],
  status: 'review-candidate', productionAdmission: 'forbidden',
}, null, 2)}\n`);
console.log(distribution);
