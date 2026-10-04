import { spawnSync } from 'node:child_process';
import { resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const SCRIPT_PATH = fileURLToPath(import.meta.url);
const ROOT = resolve(import.meta.dirname, '..');

export const NATIVE_RECIPE_TEST_FILES = Object.freeze([
  'tests/native-recipe-identity.test.mjs',
  'tests/native-recipe-materials.test.mjs',
  'tests/native-recipe-provenance.test.mjs',
]);

export const REQUIRED_NATIVE_RECIPE_TESTS = Object.freeze([
  'review identity uses independent canonical hashing and deeply immutable plan bindings',
  'unknown caller mode wins before malformed plan or package-selected authority',
  'malformed source authority and plan retain earlier #168/#361 rejection',
  'absent recipe, raw arguments, path executable, output escape and accepted status fail closed',
  'approval cannot be transplanted after any plan or invocation identity changes',
  'exact review and approval, including forged FFI/isolation claims, grant no execution',
  'exact typed input and tool allowlists compare bytes independently of Map enumeration',
  'missing, extra, substituted and non-byte material reject for every declared identity',
  'source checksum co-substitution and forged review cannot issue material authority',
  'changed executable bytes and adjacent approved plan require a fresh independent recipe approval',
  'undeclared typed source, native artifact and duplicate input do not become recipe reads',
  'denied recipes preserve real prior state in fresh roots and never start supplied tool bytes',
  'matching output hashes remain unauthenticated and cannot authorize publication or reuse',
  'missing, stale, cross-plan and permissive-mode provenance rejects at the binding boundary',
  'signed and self-reported isolation claims never become observed policy evidence',
  'output target/path/size/hash and extra authority are independently rejected',
  'output substitution or detached path inventory rejects instead of becoming an audited result',
]);

export function verifyNativeRecipeTestOutput(output) {
  const lines = output.split(/\r?\n/u);
  const results = lines.filter(line => /^(?:not )?ok /u.test(line));
  if (results.length !== 17 || REQUIRED_NATIVE_RECIPE_TESTS.length !== 17) {
    throw new Error('native recipe tests must report exactly 17 required cases');
  }
  if (results.some((line, index) => !line.startsWith(`ok ${index + 1} - `))) {
    throw new Error('native recipe tests must enumerate all 17 cases exactly once');
  }
  for (const name of REQUIRED_NATIVE_RECIPE_TESTS) {
    const reportedName = name.replaceAll('#', '\\#');
    if (results.filter(line => /^ok [1-9][0-9]* - /u.test(line)
      && line.replace(/^ok [1-9][0-9]* - /u, '') === reportedName).length !== 1) {
      throw new Error(`required native recipe test did not pass exactly once: ${name}`);
    }
  }
  for (const [prefix, summary] of [
    ['TAP version ', 'TAP version 13'], ['1..', '1..17'],
    ['# tests ', '# tests 17'], ['# suites ', '# suites 0'], ['# pass ', '# pass 17'],
    ['# fail ', '# fail 0'], ['# cancelled ', '# cancelled 0'],
    ['# skipped ', '# skipped 0'], ['# todo ', '# todo 0'],
  ]) {
    const reported = lines.filter(line => prefix === '1..'
      ? /^[0-9]+\.\./u.test(line) : line.startsWith(prefix));
    if (reported.length !== 1 || reported[0] !== summary) {
      throw new Error(`native recipe tests lack exact nonzero selection evidence: ${summary}`);
    }
  }
}

export function runNativeRecipeTests(spawn = spawnSync, output = process.stdout) {
  const result = spawn(process.execPath, ['--test', '--test-reporter=tap', ...NATIVE_RECIPE_TEST_FILES], {
    cwd: ROOT, encoding: 'utf8', maxBuffer: 4 * 1024 * 1024,
    shell: false, windowsHide: true,
  });
  if (result.stdout) output.write(result.stdout);
  if (result.stderr) output.write(result.stderr);
  if (result.error) throw result.error;
  if (result.status !== 0) throw new Error(`native recipe tests exited ${result.status ?? 'unknown'}`);
  verifyNativeRecipeTestOutput(result.stdout ?? '');
  output.write(`Native recipe tests passed on ${process.platform}/${process.arch}: `
    + '17/17 required cases from 3 exact files, 0 failed/cancelled/skipped/todo.\n');
}

if (process.argv[1] && resolve(process.argv[1]) === SCRIPT_PATH) {
  try {
    if (process.argv.length !== 2) throw new Error('usage: run-native-recipe-tests.mjs (no selection overrides)');
    runNativeRecipeTests();
  } catch (error) {
    console.error(error instanceof Error ? error.message : String(error));
    process.exitCode = 1;
  }
}
