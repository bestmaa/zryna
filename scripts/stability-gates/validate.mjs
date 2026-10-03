import { readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { parse, readSafe, reject } from './input.mjs';
import { ROOT, hostName, loadRegistry } from './registry.mjs';
import { snapshot, unchanged } from './source.mjs';
import { validateReceipt } from './evidence.mjs';

export function validateDirectory(root) {
  const source = snapshot(ROOT);
  const registry = loadRegistry();
  const receipt = parse(readSafe(root, 'receipt.json'));
  validateReceipt(receipt, { root, source, registry, host: hostName() });
  const expected = receipt.results.flatMap(result => result.attempts.flatMap(attempt =>
    [attempt.stdout.path.slice(5), attempt.stderr.path.slice(5)])).sort();
  const logs = readdirSync(resolve(root, 'logs'), { withFileTypes: true });
  if (logs.some(entry => !entry.isFile())) reject('regular registered logs required');
  if (JSON.stringify(logs.map(entry => entry.name).sort()) !== JSON.stringify(expected)) reject('unregistered or missing logs');
  if (JSON.stringify(readdirSync(root).sort()) !== JSON.stringify(['logs', 'receipt.json'])) reject('unregistered evidence entry');
  unchanged(source, snapshot(ROOT));
  return receipt;
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  try {
    if (process.argv.length !== 3) reject('usage: validate.mjs /absolute/evidence-directory');
    const receipt = validateDirectory(resolve(process.argv[2]));
    console.log(`Exact-revision evidence is consistent for ${receipt.source.commit}; M7 closure remains blocked.`);
    if (receipt.results.length === 0 || receipt.results.some(result => result.assessment.status !== 'passed')) process.exitCode = 1;
  } catch (error) {
    console.error(error.message);
    process.exitCode = 1;
  }
}
