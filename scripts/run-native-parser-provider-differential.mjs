import { spawnSync } from 'node:child_process';
import { resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const SCRIPT_PATH = fileURLToPath(import.meta.url);
const ROOT = resolve(import.meta.dirname, '..');

export const REQUIRED_DIFFERENTIAL_TEST =
  'pinned_provider_and_native_v4_parser_match_m3_corpus_and_rejections';
export const REQUIRED_DIAGNOSTIC_TEST = 'pinned_provider_matches_frozen_diagnostic_corpus';
export const REQUIRED_RESOURCE_TEST = 'pinned_provider_matches_production_resource_boundaries';
export const REQUIRED_PROJECT_RESOURCE_TEST = 'pinned_provider_matches_project_resource_boundaries';

export function verifyDifferentialOutput(output) {
  for (const name of [REQUIRED_DIFFERENTIAL_TEST, REQUIRED_DIAGNOSTIC_TEST, REQUIRED_RESOURCE_TEST, REQUIRED_PROJECT_RESOURCE_TEST]) {
    if (!output.includes(`test ${name} ... ok`)) {
      throw new Error(`required native parser provider differential did not pass: ${name}`);
    }
  }
  const summary = /test result: ok\. ([0-9]+) passed;/.exec(output);
  if (!summary || Number.parseInt(summary[1], 10) < 1) {
    throw new Error('native parser provider differential executed no required test');
  }
}

export function runNativeParserProviderDifferential(spawn = spawnSync) {
  const result = spawn('cargo', [
    'test', '--locked', '-p', 'zryna-frontend', '--test', 'native_parser_v4_provider',
    '--test', 'native_parser_parity', '--', '--ignored',
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
