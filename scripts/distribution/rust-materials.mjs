import { spawnSync } from 'node:child_process';
import { readFileSync } from 'node:fs';
import { isAbsolute, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { bytes, parseCanonical, requireValue, sha256 } from './canonical.mjs';
import { matchesCommandH1RustBinding } from './command-h1-rust-binding.mjs';

const SCRIPT_PATH = fileURLToPath(import.meta.url);
const LOCK = readFileSync(new URL('./materials-rust-v1.json', import.meta.url));
requireValue(sha256(LOCK) === '32b6343a6dfae19d7c3cdcc50658762c2c0ad08aca15124bdce85b28db38a84b',
  'Rust material recipe identity');
const RECORD = parseCanonical(LOCK);
const CARGO_LOCKS = Object.freeze({
  '0.2.1': 'fec1a746a6122a216255080e90ee39068578528feddec24c1e0d8bf9df3b8a38',
  '0.2.2': 'baf9267bada161b9e2ddd6abddac4b0f029b5bb23177c11599c341a68b2130f9',
  '0.2.3': RECORD.cargoLockSha256,
});
const TARGETS = ['x86_64-pc-windows-msvc', 'x86_64-unknown-linux-gnu'];
const MAX_METADATA = 64 * 1024 * 1024;

function command(spawn, executable, args, cwd, maximum = 65536) {
  const result = spawn(executable, args, { cwd, encoding: 'utf8', maxBuffer: maximum + 1,
    timeout: 2 * 60_000, windowsHide: true, shell: false });
  requireValue(!result.error && result.status === 0 && result.signal === null
    && typeof result.stdout === 'string' && typeof result.stderr === 'string'
    && Buffer.byteLength(result.stdout) + Buffer.byteLength(result.stderr) <= maximum,
  `qualification Rust closure ${args[0]} command`);
  return result.stdout.trim();
}

export function rustMaterials(target) {
  requireValue(Object.hasOwn(RECORD.targets, target), 'Rust material target');
  const selected = new Set(RECORD.targets[target]);
  return RECORD.packages.filter(record => selected.has(`${record.name}-${record.version}`));
}

export function validateRustMaterials(entries, target, architectureReceipt, acceptedVersion = '0.2.3', {
  productionCandidate = false, distribution,
} = {}) {
  const lock = architectureReceipt.inputs.find(input => input.logicalPath === 'Cargo.lock');
  const releaseLock = Object.hasOwn(CARGO_LOCKS, acceptedVersion)
    && lock?.sha256 === CARGO_LOCKS[acceptedVersion];
  const commandCandidateLock = productionCandidate === true && acceptedVersion === '0.2.3'
    && matchesCommandH1RustBinding(architectureReceipt, distribution, target, sha256(LOCK));
  requireValue(releaseLock || commandCandidateLock, 'Rust material lockfile identity');
  const expected = rustMaterials(target).flatMap(record => record.files)
    .map(({ path, size, sha256: digest }) => ({ path, size, sha256: digest }))
    .sort((a, b) => a.path < b.path ? -1 : a.path > b.path ? 1 : 0);
  const actual = entries.filter(entry => entry.path.startsWith('licenses/rust/'))
    .map(({ path, size, sha256: digest }) => ({ path, size, sha256: digest }));
  requireValue(bytes(actual).equals(bytes(expected)), 'Rust notice inventory differs from approved recipe');
}

export function checkQualificationRustClosure({
  cwd = resolve('.'), spawn = spawnSync, materialRecords = rustMaterials,
} = {}) {
  const cargo = command(spawn, 'rustup', ['which', '--toolchain', '1.97.1', 'cargo'], cwd);
  requireValue(isAbsolute(cargo), 'qualification Rust closure Cargo path');
  requireValue(command(spawn, cargo, ['--version'], cwd)
    === 'cargo 1.97.1 (c980f4866 2026-06-30)', 'qualification Rust closure Cargo version');
  const counts = {};
  for (const target of TARGETS) {
    let metadata;
    try {
      metadata = JSON.parse(command(spawn, cargo, [
        'metadata', '--format-version=1', '--locked', '--offline', '--filter-platform', target,
      ], cwd, MAX_METADATA));
    } catch {
      requireValue(false, `qualification Rust closure ${target} metadata`);
    }
    const packages = new Map(metadata.packages?.map((entry) => [entry.id, entry]));
    const nodes = new Map(metadata.resolve?.nodes?.map((entry) => [entry.id, entry]));
    requireValue(packages.size === metadata.packages?.length
      && nodes.size === metadata.resolve?.nodes?.length, `qualification Rust closure ${target} graph`);
    const roots = metadata.packages.filter((entry) => entry.name === 'zryna'
      && metadata.workspace_members.includes(entry.id));
    requireValue(roots.length === 1 && roots[0].targets.some(({ name, kind }) =>
      name === 'zryna' && kind.includes('bin')), `qualification Rust closure ${target} root`);
    const pending = [roots[0].id];
    const selected = new Set();
    while (pending.length > 0) {
      const id = pending.pop();
      if (selected.has(id)) continue;
      selected.add(id);
      const node = nodes.get(id);
      requireValue(node && Array.isArray(node.deps), `qualification Rust closure ${target} node`);
      for (const dependency of node.deps) {
        requireValue(Array.isArray(dependency.dep_kinds),
          `qualification Rust closure ${target} dependency`);
        if (dependency.dep_kinds.some(({ kind }) => kind === null || kind === 'build')) {
          pending.push(dependency.pkg);
        }
      }
    }
    const actual = [...selected].map((id) => packages.get(id)).filter((entry) => {
      requireValue(entry, `qualification Rust closure ${target} package`);
      requireValue(entry.source === null || entry.source
        === 'registry+https://github.com/rust-lang/crates.io-index',
      `qualification Rust closure ${target} source`);
      return entry.source !== null;
    }).map(({ name, version }) => `${name}-${version}`).sort();
    const expected = materialRecords(target).map(({ name, version }) => `${name}-${version}`);
    requireValue(new Set(actual).size === actual.length && bytes(actual).equals(bytes(expected)),
      `qualification Rust closure ${target} material identities`);
    counts[target] = actual.length;
  }
  return counts;
}

if (process.argv[1] && resolve(process.argv[1]) === SCRIPT_PATH) {
  try {
    if (process.argv.length !== 3 || process.argv[2] !== '--check-qualification-closure') {
      requireValue(false, 'qualification Rust closure usage');
    }
    const counts = checkQualificationRustClosure();
    console.log(`Qualification Rust closure passed: ${counts[TARGETS[0]]} Windows and ${counts[TARGETS[1]]} Linux crates.`);
  } catch (error) {
    console.error(error instanceof Error ? error.message : String(error));
    process.exitCode = 1;
  }
}
