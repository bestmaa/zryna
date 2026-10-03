import { spawnSync } from 'node:child_process';
import { resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const SCRIPT_PATH = fileURLToPath(import.meta.url);
const ROOT = resolve(import.meta.dirname, '..');
const PREFIX = 'module_closure::native_sources::tests::resources::';

export const REQUIRED_RESOURCE_TESTS = Object.freeze([
  'native_snapshot_exact_binding_edge_count_and_first_extra_are_enforced',
  'native_snapshot_exact_source_file_bytes_and_first_extra_are_enforced',
  'native_snapshot_exact_aggregate_bytes_and_first_extra_are_enforced',
  'native_snapshot_exact_file_count_and_first_extra_are_enforced',
].map((name) => `${PREFIX}${name}`));

export function verifyResourceTestOutput(output) {
  const lines = output.split(/\r?\n/u);
  for (const name of REQUIRED_RESOURCE_TESTS) {
    if (lines.filter((line) => line === `test ${name} ... ok`).length !== 1) {
      throw new Error(`required native source resource test did not pass: ${name}`);
    }
  }
  const summaries = [...output.matchAll(
    /^test result: ok\. ([0-9]+) passed; ([0-9]+) failed; ([0-9]+) ignored;/gmu,
  )];
  if (summaries.length !== 1
      || Number(summaries[0][1]) !== REQUIRED_RESOURCE_TESTS.length
      || Number(summaries[0][2]) !== 0 || Number(summaries[0][3]) !== 0) {
    throw new Error('native source resource command did not execute exactly its required tests');
  }
}

export function runNativeSourceResourceTests(spawn = spawnSync) {
  const result = spawn('cargo', [
    'test', '--locked', '-p', 'zryna-driver', '--lib', PREFIX,
    '--', '--ignored', '--test-threads=1',
  ], {
    cwd: ROOT,
    encoding: 'utf8',
    maxBuffer: 4 * 1024 * 1024,
    shell: false,
    windowsHide: true,
  });
  if (result.stdout) process.stdout.write(result.stdout);
  if (result.stderr) process.stderr.write(result.stderr);
  if (result.error) throw result.error;
  if (result.status !== 0 || result.signal) {
    throw new Error(`native source resource tests exited ${result.signal ?? result.status}`);
  }
  verifyResourceTestOutput(`${result.stdout ?? ''}\n${result.stderr ?? ''}`);
}

if (process.argv[1] && resolve(process.argv[1]) === SCRIPT_PATH) {
  try {
    runNativeSourceResourceTests();
  } catch (error) {
    console.error(error instanceof Error ? error.message : String(error));
    process.exitCode = 1;
  }
}
