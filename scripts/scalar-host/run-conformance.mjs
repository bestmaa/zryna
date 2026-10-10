import { spawnSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import {
  copyFileSync, createReadStream, existsSync, lstatSync, mkdirSync, readFileSync, writeFileSync,
} from 'node:fs';
import { isAbsolute, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { verifyBrowser } from '../../tests/scalar-host/browser-fixture.mjs';

const root = resolve(import.meta.dirname, '../..');
const proofs = [
  ['component', 'zryna', '--test', 'cli', 'browser_component_pinned_real_browser_and_node_agree'],
  ['scalar', 'zryna-driver', '--lib', null,
    'scalar_adapter_interface::tests::host_consumers::pinned_real_browser_and_node_execute_the_same_sealed_scalar_corpus'],
  ['transport', 'zryna-driver', '--lib', null,
    'scalar_adapter_interface::tests::host_consumers::retained_esm_transport_accepts_exact_ceiling_and_rejects_first_extra'],
];

export function requireExecutedProof(result, expected) {
  if (result.error || result.signal || result.status !== 0) {
    throw new Error(`${expected}: process failed (${result.error?.message ?? result.signal ?? result.status})`);
  }
  const text = result.stdout.replace(/\r\n/g, '\n');
  const summaries = text.match(/^test result: .*$/gm) ?? [];
  if (summaries.length !== 1 || !/^test result: ok\. 1 passed; 0 failed; 0 ignored;/.test(summaries[0]) ||
      !text.split('\n').includes(`test ${expected} ... ok`)) {
    throw new Error(`${expected}: expected exactly one executed, passing, non-ignored test`);
  }
  return { test: expected, passed: 1, failed: 0, ignored: 0 };
}

async function digestArchive(file) {
  const state = lstatSync(file);
  if (!state.isFile() || state.isSymbolicLink() || state.size > 256 * 1024 * 1024) {
    throw new Error('unsafe or oversized browser archive');
  }
  const hash = createHash('sha256');
  let bytes = 0;
  for await (const chunk of createReadStream(file)) {
    bytes += chunk.length;
    if (bytes > state.size) throw new Error('browser archive changed during verification');
    hash.update(chunk);
  }
  if (bytes !== state.size) throw new Error('browser archive length changed');
  return hash.digest('hex');
}

async function main(evidence) {
  if (process.argv.length !== 3 || !isAbsolute(evidence ?? '')) {
    throw new Error('one absolute, create-only evidence directory is required');
  }
  mkdirSync(evidence, { mode: 0o700 });
  const report = { kind: 'private-browser-conformance-evidence', host: `${process.platform}-${process.arch}`,
    node: process.version, nodeExecutable: process.execPath, proofs: [], passed: false };
  const record = () => writeFileSync(join(evidence, 'conformance.json'), `${JSON.stringify(report, null, 2)}\n`);
  try {
    if (process.version !== 'v22.22.1' || process.arch !== 'x64' ||
        !['linux', 'win32'].includes(process.platform)) throw new Error('unsupported pinned host');
    const pin = JSON.parse(readFileSync(join(root, 'tests/scalar-host/browser-pin.json'), 'utf8'));
    const selected = pin.platforms[report.host];
    if (!selected || ![selected.archiveSha256, selected.inventorySha256].every(value => /^[a-f0-9]{64}$/.test(value ?? ''))) {
      throw new Error('browser has no frozen platform hashes');
    }
    const cache = join(root, '.zryna/cache', `scalar-browser-${report.host}-${pin.browserVersion}`);
    const browserRoot = join(cache, 'browser');
    mkdirSync(join(evidence, 'config'), { mode: 0o700 });
    mkdirSync(join(evidence, 'data'), { mode: 0o700 });
    const environment = { ...process.env, CARGO_TERM_COLOR: 'never',
      ZRYNA_TEST_NODE: process.execPath, ZRYNA_TEST_BROWSER_ROOT: browserRoot,
      XDG_CONFIG_HOME: join(evidence, 'config'), XDG_DATA_HOME: join(evidence, 'data') };
    function run(label, executable, args, timeout) {
      const started = performance.now();
      const result = spawnSync(executable, args, { cwd: root, env: environment,
        encoding: 'utf8', shell: false, windowsHide: true, timeout, maxBuffer: 8 * 1024 * 1024 });
      writeFileSync(join(evidence, `${label}.stdout.log`), result.stdout ?? '', { flag: 'wx' });
      writeFileSync(join(evidence, `${label}.stderr.log`), result.stderr ?? '', { flag: 'wx' });
      result.elapsedMs = Math.round(performance.now() - started);
      return result;
    }
    const identity = run('identity', 'git', ['rev-parse', 'HEAD'], 10_000);
    if (identity.error || identity.signal || identity.status !== 0 || !/^[a-f0-9]{40}\s*$/.test(identity.stdout)) {
      throw new Error('cannot identify exact candidate commit');
    }
    report.commit = identity.stdout.trim();
    const state = run('working-tree', 'git', ['status', '--porcelain'], 10_000);
    if (state.error || state.signal || state.status !== 0 || state.stdout.trim()) {
      throw new Error('conformance requires a clean candidate checkout');
    }
    if (!existsSync(cache)) {
      const acquisition = run('acquisition', process.platform === 'win32' ? 'python' : 'python3',
        ['scripts/scalar-host/acquire-browser.py'], 130_000);
      if (acquisition.error || acquisition.signal || acquisition.status !== 0) {
        throw new Error('bounded pinned-browser acquisition failed; inspect acquisition logs');
      }
      report.acquisition = 'fresh';
    } else report.acquisition = 'reused-exact-pinned-fixture';
    for (const directory of [join(root, '.zryna'), join(root, '.zryna/cache'), cache]) {
      const metadata = lstatSync(directory);
      if (!metadata.isDirectory() || metadata.isSymbolicLink()) throw new Error('linked browser cache rejected');
    }
    const receiptPath = join(cache, 'receipt.json');
    const receiptState = lstatSync(receiptPath);
    if (!receiptState.isFile() || receiptState.isSymbolicLink() || receiptState.size > 64 * 1024) {
      throw new Error('unsafe acquisition receipt');
    }
    const receipt = JSON.parse(readFileSync(receiptPath, 'utf8'));
    if (receipt.url !== selected.url || receipt.finalUrl !== selected.publisherUrl ||
        receipt.archiveSha256 !== selected.archiveSha256 || receipt.inventorySha256 !== selected.inventorySha256 ||
        await digestArchive(join(cache, 'archive.zip')) !== selected.archiveSha256) {
      throw new Error('browser acquisition differs from frozen publisher/integrity pins');
    }
    await verifyBrowser(browserRoot);
    copyFileSync(receiptPath, join(evidence, 'acquisition.json'));
    copyFileSync(join(cache, 'inventory.json'), join(evidence, 'inventory.json'));
    report.browserVersion = pin.browserVersion;
    report.runnerVersion = pin.runnerVersion;
    report.archiveSha256 = selected.archiveSha256;
    report.inventorySha256 = selected.inventorySha256;
    record();
    for (const [label, packageName, target, targetName, expected] of proofs) {
      const args = ['test', '--locked', '-p', packageName, target];
      if (targetName) args.push(targetName);
      args.push(expected, '--', '--ignored', '--exact', '--nocapture');
      const result = run(label, process.platform === 'win32' ? 'cargo.exe' : 'cargo', args, 20 * 60_000);
      report.proofs.push({ ...requireExecutedProof(result, expected), command: ['cargo', ...args],
        elapsedMs: result.elapsedMs });
      record();
      console.log(`${label}: exactly one test executed and passed`);
    }
    await verifyBrowser(browserRoot);
    report.passed = true;
    record();
  } catch (error) {
    report.failure = String(error.message).slice(0, 2000);
    record();
    throw error;
  }
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  main(process.argv[2]).catch(error => { console.error(error.message); process.exitCode = 1; });
}
