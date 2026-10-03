import { spawnSync } from 'node:child_process';
import { resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const SCRIPT = fileURLToPath(import.meta.url);
const ROOT = resolve(import.meta.dirname, '../..');
export const STABILITY_TEST_FILES = Object.freeze([
  'tests/stability-gates-v1.test.mjs',
  'tests/stability-gates-source-process.test.mjs',
  'tests/stability-gates-interruption.test.mjs',
  'tests/stability-gates-test-selection.test.mjs',
]);
export const REQUIRED_STABILITY_TESTS = Object.freeze([
  'freezes compatibility inventory, current exclusions and all prerequisite blockers',
  'accepts internally consistent receipts while retaining blocked closure',
  'rejects stale sources, rewritten commands, missing/extra/duplicate gates and waived blockers',
  'rejects log tampering, cross-attempt substitution, fabricated counts and false success',
  'empty, skipped, cancelled, ignored and unexecuted proof never qualifies',
  'performance rejects ceiling and variance regressions; no baseline never passes',
  'performance receipt cannot label missing measurements or an unreviewed baseline passed',
  'measured performance receipts replay all six attempts and retain baseline blocker',
  'canonical bounded JSON rejects duplicate keys, unknown representations and cycles',
  'evidence paths reject escapes, Windows devices and persistent directory links',
  'source inventory binds a clean tree and detects hidden assume-unchanged mutations',
  'source inventory refuses untracked and staged input drift',
  'bounded process preserves literal argument vectors and actual failure codes',
  'deadline terminates a live process and output exhaustion fails closed',
  'interruption scope retains the first signal and removes handlers idempotently',
  'SIGINT terminates the owned child and grandchild with failed terminal evidence',
  'SIGTERM terminates descendants even in a separate Linux process group',
  'repeated interruptions preserve the first signal and do not leak cleanup handlers',
  'prior cancellation prevents process start and cannot qualify successful proof',
  'collector interruption preserves actual logs and never starts the later gates',
  'stability CI selects all 27 distinct cases from four exact files',
  'stability proof rejects omitted, renamed, duplicate and nonpassing cases',
  'stability proof rejects empty, partial and ambiguous TAP summaries',
  'stability runner uses bounded direct selection and propagates process failure',
  'both platform authorities require stability tests through the M0 aggregate',
  'stability CI omission, bypass, relocation and aggregate weakening reject',
  'stability CI preserves the independent native recipe insertion point',
]);

export function verifyStabilityTestOutput(output) {
  const lines = output.split(/\r?\n/u);
  const results = lines.filter(line => /^(?:not )?ok /u.test(line));
  const count = REQUIRED_STABILITY_TESTS.length;
  if (count !== 27 || results.length !== count
    || results.some((line, index) => !line.startsWith(`ok ${index + 1} - `))) {
    throw new Error('stability tests must pass all 27 cases exactly once');
  }
  for (const name of REQUIRED_STABILITY_TESTS) {
    if (results.filter(line => line.replace(/^ok [1-9][0-9]* - /u, '')
      === name.replaceAll('#', '\\#')).length !== 1) {
      throw new Error(`required stability case did not pass exactly once: ${name}`);
    }
  }
  for (const [prefix, expected] of [
    ['TAP version ', 'TAP version 13'], ['1..', '1..27'],
    ['# tests ', '# tests 27'], ['# suites ', '# suites 0'], ['# pass ', '# pass 27'],
    ['# fail ', '# fail 0'], ['# cancelled ', '# cancelled 0'],
    ['# skipped ', '# skipped 0'], ['# todo ', '# todo 0'],
  ]) {
    const values = lines.filter(line => prefix === '1..'
      ? /^[0-9]+\.\./u.test(line) : line.startsWith(prefix));
    if (values.length !== 1 || values[0] !== expected) {
      throw new Error(`stability tests lack exact nonzero proof: ${expected}`);
    }
  }
}

export function runStabilityTests(spawn = spawnSync, output = process.stdout) {
  if (!['linux', 'win32'].includes(process.platform) || process.arch !== 'x64') {
    throw new Error('stability test authority supports only Linux/Windows x86-64');
  }
  const result = spawn(process.execPath, ['--test', '--test-reporter=tap', ...STABILITY_TEST_FILES], {
    cwd: ROOT, encoding: 'utf8', shell: false, windowsHide: true,
    timeout: 120_000, maxBuffer: 4 * 1024 * 1024,
  });
  if (result.stdout) output.write(result.stdout);
  if (result.stderr) output.write(result.stderr);
  if (result.error) throw result.error;
  if (result.status !== 0 || result.signal) throw new Error('stability test process did not complete successfully');
  verifyStabilityTestOutput(result.stdout ?? '');
  output.write(`Stability evidence tests passed on ${process.platform}/${process.arch}: `
    + '27/27 required cases from 4 exact files, 0 failed/cancelled/skipped/todo; performance baseline remains blocked.\n');
}

if (process.argv[1] && resolve(process.argv[1]) === SCRIPT) {
  try {
    if (process.argv.length !== 2) throw new Error('usage: check-tests.mjs (no selection overrides)');
    runStabilityTests();
  } catch (error) {
    console.error(error instanceof Error ? error.message : String(error));
    process.exitCode = 1;
  }
}
