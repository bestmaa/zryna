import { spawnSync } from 'node:child_process';
import { readFileSync, mkdirSync, writeFileSync, lstatSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { createRequire } from 'node:module';
import { verifySignedRelease } from '../distribution-release/verify-signed-release.mjs';
import { verifyArchive } from '../distribution/verify.mjs';
import { encodeZip } from '../distribution/archive-zip.mjs';
import { encodeTar } from '../distribution/archive-tar.mjs';
import { extractVerifiedProductionFiles } from '../distribution-release/run-installed-acceptance.mjs';
import { canonicalVsix, vsixEntries } from './vsix.mjs';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '../..');
const require = createRequire(import.meta.url);
const { hash, verifyInstallation } = require('../../editors/vscode-zryna/src/installation.cjs');
const candidate = '0.1.0-candidate.3';
const target = process.platform === 'win32' ? 'x86_64-pc-windows-msvc' : 'x86_64-unknown-linux-gnu';
const suffix = process.platform === 'win32' ? '.exe' : '';

export function verifyEditorManifest(entries) {
  const packaged = entries.find(entry => entry.name === 'extension/package.json');
  if (!packaged) throw new Error('VSIX package manifest is missing.');
  const metadata = JSON.parse(packaged.data);
  const compatibility = metadata.zrynaCompatibility;
  if (metadata.name !== 'zryna' || metadata.version !== '0.5.0'
    || compatibility?.compilerVersion !== '0.2.3' || compatibility.serverVersion !== '0.5.0'
    || compatibility.installationCapability !== 'portable-setup-v1'
    || compatibility.requiredCapabilities?.['scalar-v2'] !== 'scalar-format-v1'
    || compatibility.requiredCapabilities?.['control-flow-v1'] !== 'control-flow-format-v1'
    || compatibility.requiredCapabilities?.['data-ownership-v1'] !== 'data-ownership-format-v1'
    || JSON.stringify(compatibility.profiles) !== JSON.stringify(['scalar-v2', 'control-flow-v1', 'data-ownership-v1'])
    || compatibility.sourceBuildRequired !== false || compatibility.releasedCompilerCompatible !== true) {
    throw new Error('VSIX component compatibility differs from the candidate.');
  }
}

function run(executable, args) {
  const result = spawnSync(executable, args, { cwd: root, encoding: 'utf8', shell: false,
    windowsHide: true, timeout: 120000, maxBuffer: 1024 * 1024 });
  if (result.error || result.status !== 0) throw new Error(`Candidate input command failed: ${result.stderr}`);
  return result.stdout.trim();
}

// The signed immutable compiler release is verified afresh. The outer artifact is a review
// candidate, not a production release; its exact archive/manifest hashes require reviewer delivery.
export async function buildSetup({ release, cosign, server, vsix, output }) {
  if (process.version !== 'v22.22.1' || process.arch !== 'x64'
    || !['win32', 'linux'].includes(process.platform)) throw new Error('Use the pinned supported build host.');
  for (const filename of [server, vsix]) {
    const stat = lstatSync(filename);
    if (!stat.isFile() || stat.isSymbolicLink()) throw new Error('Ordinary candidate input required.');
  }
  const sourceCommit = run('git', ['rev-parse', 'HEAD']);
  const sourceTree = run('git', ['show', '-s', '--format=%T', 'HEAD']);
  const epoch = Number(run('git', ['show', '-s', '--format=%ct', 'HEAD']));
  if (run('git', ['status', '--porcelain', '--untracked-files=all'])) throw new Error('Commit the reviewed candidate before packaging.');
  if (run(server, ['--version']) !== `zryna-language-server 0.5.0 portable-setup-v1 ${sourceCommit}`) {
    throw new Error('Server source identity differs from candidate.');
  }
  const vsixBytes = readFileSync(vsix);
  if (!canonicalVsix(vsixBytes).equals(vsixBytes)) throw new Error('VSIX timestamps must be canonical.');
  const entries = vsixEntries(vsixBytes);
  verifyEditorManifest(entries);
  const expected = ['CHANGELOG.md', 'LICENSE', 'README.md', 'package.json', 'src/connection.cjs',
    'src/extension.cjs', 'src/installation.cjs', 'src/run-command.cjs', 'src/run-input.cjs',
    'src/run-process.cjs', 'src/run-project.cjs', 'syntaxes/zryna.tmLanguage.json'];
  if (entries.length !== expected.length + 2) throw new Error('VSIX inventory differs.');
  for (const path of expected) {
    const packaged = { 'CHANGELOG.md': 'changelog.md', 'README.md': 'readme.md', LICENSE: 'LICENSE.txt' }[path] ?? path;
    if (!entries.find(entry => entry.name === `extension/${packaged}`)?.data
      .equals(readFileSync(join(root, 'editors/vscode-zryna', path)))) throw new Error(`VSIX source differs: ${path}`);
  }
  const envelope = await verifySignedRelease({ directory: release, cosign });
  if (envelope.version !== '0.2.3' || envelope.tag !== 'v0.2.3'
    || envelope.channel !== 'developer-preview') throw new Error('Expected immutable compiler v0.2.3 Developer Preview.');
  const subject = envelope.subjects.find(value => value.target === target);
  const verified = await verifyArchive(readFileSync(join(release, subject.archive.path)), {
    ...subject.archive, filename: subject.archive.path, version: envelope.version,
    source: { repository: envelope.source.repository, ref: envelope.source.ref,
      commit: envelope.source.commit, tree: envelope.source.tree, sourceDateEpoch: envelope.source.sourceDateEpoch },
    target: { triple: target, archiveFormat: suffix ? 'zip' : 'tar-gzip', platformBaseline: subject.platformBaseline },
    recipe: envelope.recipe,
  });
  const files = verified.files.map(file => ({ ...file, path: `compiler/${file.path}` }));
  const add = (path, data, mode = 0o644) => files.push({ path, data: Buffer.from(data), mode });
  add(`bin/zryna-language-server${suffix}`, readFileSync(server), 0o755);
  add('editor/zryna-0.5.0.vsix', readFileSync(vsix));
  add('installation.cjs', readFileSync(join(root, 'editors/vscode-zryna/src/installation.cjs')));
  add('setup.cjs', readFileSync(join(root, 'scripts/portable-setup/setup.cjs')));
  add('README.md', readFileSync(join(root, 'docs/PORTABLE_SETUP.md')));
  add('examples/main.zry', 'export function add(x:i32,y:i32):i32{return x+y;}\nexport function double(x:i32):i32{return x+x;}\n');
  add('examples/control-flow.zry', 'function twice(value:i32):i32{return value+value;}\nexport function accumulate(enabled:bool,count:i32):i32{const step:i32=twice(1);let i:i32=0;let total:i32=0;while(i<count){i=i+1;total=total+step;}if(enabled){return total;}else{return -total;}}\n');
  add('examples/m3/main.zry', 'import{double}from"./math.zry";export function score(value:i32):i32{return double(value);}');
  add('examples/m3/math.zry', 'export function double(value:i32):i32{return value+value;}');
  add('install.cmd', '@echo off\r\n"%~dp0compiler\\runtime\\node\\node.exe" "%~dp0setup.cjs" %*\r\n');
  add('install.sh', '#!/bin/sh\nset -eu\nroot=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)\nexec "$root/compiler/runtime/node/bin/node" "$root/setup.cjs" "$@"\n', 0o755);
  for (const name of ['zryna-release-envelope-v1.json', 'zryna-release-envelope-v1.sigstore.json',
    subject.attestation.path, subject.provenance.path]) add(`provenance/${name}`, readFileSync(join(release, name)));
  files.sort((a, b) => a.path < b.path ? -1 : 1);
  const manifest = {
    format: 'zryna.portable-setup.v1', candidate, status: 'review-candidate', productionAdmission: 'forbidden',
    target, sourceCommit, sourceTree, sourceDateEpoch: epoch,
    compilerVersion: '0.2.3', serverVersion: '0.5.0', editorVersion: '0.5.0', capability: 'portable-setup-v1',
    compilerArchive: subject.archive, compilerSource: envelope.source.commit,
    files: files.map(({ path, data, mode }) => ({ path, size: data.length, sha256: hash(data), mode })),
  };
  const manifestBytes = Buffer.from(`${JSON.stringify(manifest, null, 2)}\n`);
  add('setup.json', manifestBytes);
  files.sort((a, b) => a.path < b.path ? -1 : 1);
  const name = `zryna-setup-${candidate}-${target}`;
  mkdirSync(output, { recursive: false });
  const installation = join(output, name);
  extractVerifiedProductionFiles(installation, files);
  verifyInstallation(installation, hash(manifestBytes));
  const archive = suffix ? encodeZip(name, files) : await encodeTar(name, files, epoch);
  const filename = `${name}.${suffix ? 'zip' : 'tar.gz'}`;
  writeFileSync(join(output, filename), archive, { flag: 'wx' });
  const receipt = { format: 'zryna.portable-setup-receipt.v1', status: 'review-candidate',
    productionAdmission: 'forbidden', sourceCommit, sourceTree, target,
    archive: { filename, size: archive.length, sha256: hash(archive) }, manifestSha256: hash(manifestBytes),
    serverSha256: hash(readFileSync(server)), vsixSha256: hash(readFileSync(vsix)),
    compilerArchive: subject.archive };
  writeFileSync(join(output, 'candidate-receipt.json'), `${JSON.stringify(receipt, null, 2)}\n`, { flag: 'wx' });
  return receipt;
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const args = process.argv.slice(2);
  if (args.length !== 10 || ['--release', '--cosign', '--server', '--vsix', '--output']
    .some((key, index) => args[index * 2] !== key)) throw new Error('Expected --release --cosign --server --vsix --output paths.');
  console.log(JSON.stringify(await buildSetup(Object.fromEntries(
    ['release', 'cosign', 'server', 'vsix', 'output'].map((key, index) => [key, resolve(args[index * 2 + 1])]),
  )), null, 2));
}
