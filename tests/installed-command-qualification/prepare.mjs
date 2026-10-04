// Test-only candidate assembly. No signing, protected-main authority, release or publication.
import { spawnSync } from 'node:child_process';
import { readFileSync, writeFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { createRequire } from 'node:module';
import { bytes, sha256 } from '../../scripts/distribution/canonical.mjs';
import { preparePayload } from '../../scripts/distribution/payload.mjs';
import { rustMaterials } from '../../scripts/distribution/rust-materials.mjs';
import { NODE_TARGETS } from '../../scripts/distribution/materials.mjs';
import { inventoryBytes, checksumBytes, targetPaths } from '../../scripts/distribution/inventory.mjs';
import { verifyCompiledIdentity } from '../../scripts/distribution/binary-identity.mjs';
import { encodeTar } from '../../scripts/distribution/archive-tar.mjs';
import { encodeZip } from '../../scripts/distribution/archive-zip.mjs';
import { verifyArchive } from '../../scripts/distribution/verify.mjs';
import { extractVerifiedProductionFiles } from '../../scripts/distribution-release/run-installed-acceptance.mjs';

export function run(executable, args, cwd, env = process.env) {
  const result = spawnSync(executable, args, { cwd, env, shell: false, windowsHide: true,
    encoding: 'utf8', maxBuffer: 64 * 1024 * 1024 });
  if (result.error || result.status !== 0) {
    throw new Error(`${executable} ${args.join(' ')} failed: ${result.stdout}\n${result.stderr}`);
  }
  return result.stdout.trim();
}

export async function prepare(source, output) {
  if (process.version !== 'v22.22.1' || process.arch !== 'x64'
      || !['linux', 'win32'].includes(process.platform)) throw new Error('Pinned host required');
  const head = run('git', ['rev-parse', 'HEAD'], source);
  if (run('git', ['status', '--porcelain=v1'], source)) throw new Error('Commit candidate first');
  const tree = run('git', ['show', '-s', '--format=%T', 'HEAD'], source);
  const branch = run('git', ['rev-parse', '--abbrev-ref', 'HEAD'], source);
  const epoch = Number(run('git', ['show', '-s', '--format=%ct', 'HEAD'], source));
  const target = process.platform === 'win32' ? 'x86_64-pc-windows-msvc' : 'x86_64-unknown-linux-gnu';
  const paths = targetPaths(target);
  const tool = name => run('rustup', ['which', '--toolchain', '1.97.1', name], source);
  const cargo = tool('cargo');
  const rustc = tool('rustc');
  // This command really runs. No protected build/gate receipt is fabricated or submitted.
  const command = ['cargo', 'run', '--locked', '-p', 'zryna', '--', 'architecture', 'check', '--json'];
  const report = JSON.parse(run(cargo, command.slice(1), source));
  const receipt = bytes({ format: 'zryna.source-build-receipt.v1',
    source: { repository: 'https://github.com/zryna/zryna', commit: head, tree }, command,
    toolchain: { channel: '1.97.1', cargoVersion: run(cargo, ['--version'], source),
      cargoSha256: sha256(readFileSync(cargo)), rustcVersion: run(rustc, ['--version'], source),
      rustcSha256: sha256(readFileSync(rustc)) }, report,
    inputs: ['Cargo.lock', 'Cargo.toml', 'rust-toolchain.toml', 'zryna.workspace.json']
      .map(logicalPath => { const data = readFileSync(join(source, logicalPath));
        return { logicalPath, size: data.length, sha256: sha256(data) }; }) });
  writeFileSync(join(output, 'architecture-receipt.json'), receipt, { flag: 'wx' });
  const files = [];
  const add = (path, filename, mode = 0o644) => files.push({ path, mode, data: readFileSync(filename) });
  for (const path of ['LICENSE', 'NOTICE']) add(path, join(source, path));
  for (const path of ['limits-v3.mjs', 'limits-v4.mjs', 'worker-v3.mjs', 'worker-v4.mjs', 'worker.mjs']) {
    add(`lib/zryna/bootstrap/${path}`, join(source, 'adapters/typescript-6/src', path));
  }
  const require = createRequire(join(source, 'adapters/typescript-6/package.json'));
  const wrapper = dirname(require.resolve('@typescript/typescript6/package.json'));
  const old = dirname(createRequire(join(wrapper, 'package.json')).resolve('@typescript/old/package.json'));
  for (const [name, directory] of [['typescript6', wrapper], ['old', old]]) {
    for (const file of ['package.json', 'lib/typescript.js']) {
      add(`lib/zryna/bootstrap/node_modules/@typescript/${name}/${file}`, join(directory, file));
    }
  }
  add('licenses/typescript6-LICENSE.txt', join(wrapper, 'LICENSE.txt'));
  add('licenses/typescript-LICENSE.txt', join(old, 'LICENSE.txt'));
  add('licenses/typescript-ThirdPartyNoticeText.txt', join(old, 'ThirdPartyNoticeText.txt'));
  // An explicitly supplied upstream Node directory is verified against the immutable recipe.
  const nodeRoot = process.env.ZRYNA_QUALIFICATION_NODE_ROOT;
  if (!nodeRoot) throw new Error('ZRYNA_QUALIFICATION_NODE_ROOT is required');
  add(paths.node, join(nodeRoot, process.platform === 'win32' ? 'node.exe' : 'bin/node'), 0o755);
  add('licenses/node-LICENSE', join(nodeRoot, 'LICENSE'));
  const node = NODE_TARGETS[target];
  for (const [path, expected] of [[paths.node, node.executable], ['licenses/node-LICENSE', node.license]]) {
    const actual = files.find(file => file.path === path).data;
    if (actual.length !== expected[0] || sha256(actual) !== expected[1]) throw new Error('Node pin differs');
  }
  const metadata = JSON.parse(run(cargo, ['metadata', '--format-version=1', '--locked', '--offline'], source));
  for (const record of rustMaterials(target)) {
    const pkg = metadata.packages.find(pkg => pkg.name === record.name && pkg.version === record.version);
    if (!pkg) throw new Error(`Missing pinned Rust material ${record.name}-${record.version}`);
    for (const expected of record.files) {
      // Cargo's verified registry source contains the same pinned license, including Wasmtime.
      const materialPackage = expected.origin.startsWith('https://raw.githubusercontent.com/bytecodealliance/wasmtime/')
        ? metadata.packages.find(pkg => pkg.name === 'wasmtime' && pkg.version === '48.0.1') : pkg;
      const data = readFileSync(join(dirname(materialPackage.manifest_path), expected.sourcePath));
      if (data.length !== expected.size || sha256(data) !== expected.sha256) {
        throw new Error(`Rust notice pin differs: ${expected.path}`);
      }
      files.push({ path: expected.path, mode: 0o644, data });
    }
  }
  files.sort((a, b) => a.path < b.path ? -1 : 1);
  const identity = { version: '0.2.3',
    // The unchanged installation wire supports intended main candidate identity. The actual
    // observed ref is recorded separately below; this is never protected-main admission proof.
    source: { repository: 'https://github.com/zryna/zryna', ref: 'refs/heads/main', commit: head,
      tree, sourceDateEpoch: epoch },
    target: { triple: target, archiveFormat: paths.format, platformBaseline: process.platform === 'win32'
      ? { os: 'windows', product: 'windows-server', version: '2022', architecture: 'x86_64', runtime: 'operating-system-ucrt' }
      : { os: 'linux', distribution: 'ubuntu', version: '24.04', architecture: 'x86_64' } },
    recipe: { format: 'zryna.distribution-recipe.v1',
      sha256: sha256(readFileSync(join(source, 'scripts/distribution/release-recipe-v1.json'))) } };
  const prepared = preparePayload(identity, files, receipt, { productionCandidate: true });
  const digest = sha256(prepared.distribution);
  const env = { ...process.env, ZRYNA_DISTRIBUTION_SHA256: digest };
  run(cargo, ['build', '--locked', '-p', 'zryna', '--bin', 'zryna'], source, env);
  const buildRoot = process.env.CARGO_TARGET_DIR || join(source, 'target');
  const cli = readFileSync(join(buildRoot, 'debug', process.platform === 'win32' ? 'zryna.exe' : 'zryna'));
  verifyCompiledIdentity(cli, digest, target);
  const payload = [...prepared.payload, { path: paths.cli, mode: 0o755, data: cli },
    { path: 'metadata/distribution.json', mode: 0o644, data: prepared.distribution }].sort((a, b) => a.path < b.path ? -1 : 1);
  const entries = [...prepared.record.files,
    { path: paths.cli, size: cli.length, sha256: sha256(cli), role: 'cli', mode: 0o755, material: 'source',
      licenses: prepared.record.files.filter(file => file.role === 'license'
        && (file.path === 'LICENSE' || file.path.startsWith('licenses/rust/'))).map(file => file.path) },
    { path: 'metadata/distribution.json', size: prepared.distribution.length, sha256: digest,
      role: 'metadata', mode: 0o644, material: 'source', licenses: ['LICENSE'] }].sort((a, b) => a.path < b.path ? -1 : 1);
  payload.push({ path: 'metadata/inventory.json', mode: 0o644, data: inventoryBytes(entries, target) });
  payload.sort((a, b) => a.path < b.path ? -1 : 1);
  payload.push({ path: 'metadata/checksums.sha256', mode: 0o644, data: checksumBytes(payload) });
  payload.sort((a, b) => a.path < b.path ? -1 : 1);
  const name = `zryna-0.2.3-${target}`;
  const archive = process.platform === 'win32' ? encodeZip(name, payload) : await encodeTar(name, payload, epoch);
  const expected = { ...identity, filename: `${name}.${paths.extension}`, size: archive.length, sha256: sha256(archive) };
  const verified = await verifyArchive(archive, expected, { productionCandidate: true });
  writeFileSync(join(output, expected.filename), archive, { flag: 'wx' });
  const installation = join(output, 'fresh-install');
  extractVerifiedProductionFiles(installation, verified.files);
  const proof = { status: 'test-only-review-candidate', productionAdmission: 'forbidden',
    observedSource: { head, tree, branch }, intendedSourceRef: identity.source.ref,
    archive: expected, cliSha256: sha256(cli), distributionSha256: digest,
    recipeApplied: false, buildProfile: 'dev', signatureAuthentication: 'not-performed' };
  writeFileSync(join(output, 'candidate.json'), `${JSON.stringify(proof, null, 2)}\n`, { flag: 'wx' });
  return { installation, paths, proof };
}
