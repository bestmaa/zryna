import assert from 'node:assert/strict';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { exact, readSafe, reject, sha256 } from './input.mjs';

export const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), '../..');
export const REGISTRY = 'tests/stability-gates-v1.json';
export const REGISTRY_SHA256 = 'f339c9edba6773d63a252908dbfa094875a5ecb7c66843dd8017142f547e787b';

export function loadRegistry(root = ROOT) {
  const bytes = readSafe(root, REGISTRY);
  if (sha256(bytes) !== REGISTRY_SHA256) reject('stability registry differs from frozen authority');
  const registry = JSON.parse(bytes);
  exact(registry, ['format', 'scope', 'hosts', 'toolchains', 'blockers', 'contracts', 'gates'], 'registry');
  assert.equal(registry.format, 'zryna.stability-gates.v1');
  assert.equal(registry.scope, 'current-support-preparation');
  const ids = new Set(registry.gates.map(gate => gate.id));
  assert.equal(ids.size, registry.gates.length, 'S418: duplicate gate');
  for (const contract of registry.contracts) {
    if (!ids.has(contract.gate)) reject('contract missing gate');
    readSafe(root, contract.authority, 2 * 1024 * 1024);
  }
  return registry;
}

export function select(registry, lane, host) {
  if (!['compatibility', 'security', 'performance', 'all'].includes(lane)) reject('unknown lane');
  if (!registry.hosts.includes(host)) reject('unsupported host');
  return registry.gates.filter(gate => (lane === 'all' || gate.lane === lane) && gate.hosts.includes(host));
}

export function hostName(platform = process.platform, arch = process.arch) {
  if (arch !== 'x64' || !['linux', 'win32'].includes(platform)) reject('unsupported host');
  return platform === 'linux' ? 'linux-x86_64' : 'windows-x86_64';
}
