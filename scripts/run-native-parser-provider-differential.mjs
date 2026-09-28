import { spawnSync } from 'node:child_process';
import { resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const SCRIPT_PATH = fileURLToPath(import.meta.url);
const ROOT = resolve(import.meta.dirname, '..');

export const REQUIRED_DIFFERENTIAL_TEST =
  'pinned_provider_and_native_v4_parser_match_m3_corpus_and_rejections';

export function verifyDifferentialOutput(output) {
  if (!output.includes(`test ${REQUIRED_DIFFERENTIAL_TEST} ... ok`)) {
    throw new Error(`required native parser provider differential did not pass: ${REQUIRED_DIFFERENTIAL_TEST}`);
  }
  const summary = /test result: ok\. ([0-9]+) passed;/.exec(output);
  if (!summary || Number.parseInt(summary[1], 10) < 1) {
    throw new Error('native parser provider differential executed no required test');
  }
}

export function runNativeParserProviderDifferential(spawn = spawnSync) {
  const result = spawn('cargo', [
    'test', '--locked', '-p', 'zryna-frontend', '--test', 'native_parser_v4_provider',
    '--', '--ignored', '--exact', REQUIRED_DIFFERENTIAL_TEST,
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
  if (result.status !== 0) {
    throw new Error(`native parser provider differential exited ${result.status}`);
  }
  verifyDifferentialOutput(`${result.stdout}\n${result.stderr}`);
}

if (process.argv[1] && resolve(process.argv[1]) === SCRIPT_PATH) {
  try {
    runNativeParserProviderDifferential();
  } catch (error) {
    console.error(error instanceof Error ? error.message : String(error));
    process.exitCode = 1;
  }
}
