import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import test from 'node:test';
import { parseDocument } from 'yaml';
import {
  STABILITY_TEST_FILES, REQUIRED_STABILITY_TESTS, runStabilityTests, verifyStabilityTestOutput,
} from '../scripts/stability-gates/check-tests.mjs';

const requiredStep = Object.freeze({
  name: 'Verify stability evidence validator hostile cases',
  run: 'node scripts/stability-gates/check-tests.mjs',
});
const parsed = parseDocument(readFileSync(new URL('../.github/workflows/ci.yml', import.meta.url), 'utf8'));
assert.deepEqual(parsed.errors, []);
const workflow = parsed.toJS();

// Validate this finite addition before removing it for the existing whole-workflow digest.
// Other owners' steps are retained, including #405's separate Rust insertion point.
export function withoutStabilityTests(candidate) {
  const original = structuredClone(candidate);
  const job = original.jobs['adapter-platform'];
  assert.equal(job.if, undefined);
  assert.equal(job.needs, undefined);
  assert.equal(job['continue-on-error'], undefined);
  assert.equal(job.strategy['fail-fast'], false);
  assert.deepEqual(job.strategy.matrix, { os: ['ubuntu-latest', 'windows-latest'] });
  assert.equal(job['runs-on'], '${{ matrix.os }}');
  const indices = job.steps.flatMap((step, index) => step.name === requiredStep.name ? [index] : []);
  assert.equal(indices.length, 1);
  const index = indices[0];
  assert.deepEqual(job.steps[index], requiredStep);
  assert.deepEqual(job.steps[index - 1], { run: 'pnpm install --frozen-lockfile' });
  assert.deepEqual(job.steps[index + 1], { run: 'pnpm adapter:check' });
  assert.deepEqual(original.jobs.adapter, {
    name: 'adapter', if: 'always()', needs: ['fast-contracts', 'preflight', 'adapter-platform'],
    'runs-on': 'ubuntu-latest', steps: [{ name: 'Verify adapter platforms',
      env: { FAST_CONTRACTS_RESULT: '${{ needs.fast-contracts.result }}',
        PREFLIGHT_RESULT: '${{ needs.preflight.result }}', PLATFORM_RESULT: '${{ needs.adapter-platform.result }}' },
      run: 'test "$FAST_CONTRACTS_RESULT" = success && test "$PREFLIGHT_RESULT" = success && test "$PLATFORM_RESULT" = success',
    }],
  });
  const aggregate = original.jobs.m0;
  assert.equal(aggregate.if, 'always()');
  assert.equal(aggregate['continue-on-error'], undefined);
  assert(aggregate.needs.includes('adapter'));
  const last = aggregate.steps.at(-1);
  assert.equal(last.if, undefined);
  assert.equal(last['continue-on-error'], undefined);
  assert.equal(last.env.ADAPTER_RESULT, '${{ needs.adapter.result }}');
  assert.equal(last.run, 'node scripts/verify-provider-v4-ci-result.mjs && test "$FAST_CONTRACTS_RESULT" = success && test "$OWNED_DATA_QUICK_RESULT" = success && test "$PREFLIGHT_RESULT" = success && test "$RUST_RESULT" = success && test "$ADAPTER_RESULT" = success && test "$ROUTING_RESULT" = success');
  job.steps.splice(index, 1);
  assert(!/Verify stability evidence validator hostile cases|stability-gates\/check-tests\.mjs/u.test(JSON.stringify(original)));
  return original;
}

function passedOutput() {
  return ['TAP version 13', ...REQUIRED_STABILITY_TESTS.map((name, index) =>
    `ok ${index + 1} - ${name.replaceAll('#', '\\#')}`), '1..27', '# tests 27', '# suites 0',
  '# pass 27', '# fail 0', '# cancelled 0', '# skipped 0', '# todo 0', ''].join('\n');
}

test('stability CI selects all 27 distinct cases from four exact files', () => {
  assert.deepEqual(STABILITY_TEST_FILES, ['tests/stability-gates-v1.test.mjs',
    'tests/stability-gates-source-process.test.mjs', 'tests/stability-gates-interruption.test.mjs',
    'tests/stability-gates-test-selection.test.mjs']);
  assert.equal(REQUIRED_STABILITY_TESTS.length, 27);
  assert.equal(new Set(REQUIRED_STABILITY_TESTS).size, 27);
  assert(Object.isFrozen(STABILITY_TEST_FILES));
  assert(Object.isFrozen(REQUIRED_STABILITY_TESTS));
  const declared = STABILITY_TEST_FILES.flatMap(file => [...readFileSync(new URL(`../${file}`, import.meta.url),
    'utf8').matchAll(/^test\('([^']+)',/gmu)].map(match => match[1]));
  assert.deepEqual(declared, REQUIRED_STABILITY_TESTS);
  assert.match(readFileSync(new URL('preflight.test.mjs', import.meta.url), 'utf8'),
    /^import '\.\/stability-gates-test-selection\.test\.mjs';$/mu);
});

test('stability proof rejects omitted, renamed, duplicate and nonpassing cases', () => {
  const output = passedOutput();
  verifyStabilityTestOutput(output);
  for (const name of REQUIRED_STABILITY_TESTS) {
    const line = output.split('\n').find(value => value.endsWith(` - ${name.replaceAll('#', '\\#')}`));
    for (const changed of [output.replace(`${line}\n`, ''), output.replace(line, `${line}\n${line}`),
      output.replace(line, line.replace(name, 'unexpected case')), output.replace(line, line.replace(/^ok/u, 'not ok')),
      output.replace(line, `${line} # SKIP omitted`), output.replace(line, `${line} # TODO omitted`),
    ]) assert.throws(() => verifyStabilityTestOutput(changed));
  }
  assert.throws(() => verifyStabilityTestOutput(`${output}ok 28 - unexpected case\n`));
  assert.throws(() => verifyStabilityTestOutput(output.replace('ok 2 - ', 'ok 1 - ')));
});

test('stability proof rejects empty, partial and ambiguous TAP summaries', () => {
  const output = passedOutput();
  for (const summary of ['TAP version 13', '1..27', '# tests 27', '# suites 0', '# pass 27',
    '# fail 0', '# cancelled 0', '# skipped 0', '# todo 0']) {
    for (const changed of [output.replace(`${summary}\n`, ''), `${output}${summary}\n`,
      output.replace(summary, summary.replace(/\d+/u, '999')), `${output}${summary.replace(/\d+/u, '999')}\n`,
    ]) assert.throws(() => verifyStabilityTestOutput(changed));
  }
  for (const empty of ['', 'TAP version 13\n1..0\n# tests 0\n# pass 0\n']) {
    assert.throws(() => verifyStabilityTestOutput(empty));
  }
});

test('stability runner uses bounded direct selection and propagates process failure', () => {
  const sink = { write() {} };
  runStabilityTests((executable, args, options) => {
    assert.equal(executable, process.execPath);
    assert.deepEqual(args, ['--test', '--test-reporter=tap', ...STABILITY_TEST_FILES]);
    assert.equal(options.shell, false);
    assert.equal(options.windowsHide, true);
    assert.equal(options.timeout, 120_000);
    assert.equal(options.maxBuffer, 4 * 1024 * 1024);
    return { status: 0, stdout: passedOutput() };
  }, sink);
  for (const result of [{ status: 1, stdout: passedOutput() }, { status: null, stdout: passedOutput() },
    { status: 0, signal: 'SIGTERM', stdout: passedOutput() }, { status: 0, stdout: '' },
    { status: 0, stderr: passedOutput() }, { status: 0, stdout: passedOutput(), error: new Error('ETIMEDOUT') },
  ]) assert.throws(() => runStabilityTests(() => result, sink));
});

test('both platform authorities require stability tests through the M0 aggregate', () => {
  assert.doesNotThrow(() => withoutStabilityTests(workflow));
});

test('stability CI omission, bypass, relocation and aggregate weakening reject', () => {
  for (const mutate of [
    (w, i) => { w.jobs['adapter-platform'].steps.splice(i, 1); },
    (w, i) => { w.jobs['adapter-platform'].steps[i].if = "runner.os == 'Linux'"; },
    (w, i) => { w.jobs['adapter-platform'].steps[i]['continue-on-error'] = true; },
    (w, i) => { w.jobs['adapter-platform'].steps[i].run += ' || true'; },
    (w, i) => { w.jobs['adapter-platform'].steps[i].run += ' --test-name-pattern=none'; },
    (w, i) => { w.jobs['adapter-platform'].steps.push(w.jobs['adapter-platform'].steps[i]); },
    (w, i) => { const step = w.jobs['adapter-platform'].steps.splice(i, 1)[0]; w.jobs['adapter-platform'].steps.push(step); },
    (w, i) => { w.jobs.rust.steps.push(w.jobs['adapter-platform'].steps[i]); },
    w => { w.jobs['adapter-platform'].strategy.matrix.os.pop(); },
    w => { w.jobs['adapter-platform'].if = 'false'; },
    w => { w.jobs['adapter-platform']['continue-on-error'] = true; },
    w => { w.jobs.adapter.needs = ['fast-contracts', 'preflight']; },
    w => { w.jobs.adapter.steps[0].run += ' || true'; },
    w => { w.jobs.m0.needs = w.jobs.m0.needs.filter(id => id !== 'adapter'); },
    w => { w.jobs.m0.steps.at(-1).env.ADAPTER_RESULT = 'success'; },
    w => { w.jobs.m0.steps.at(-1).run += ' || true'; },
    w => { w.jobs.m0.steps.at(-1)['continue-on-error'] = true; },
  ]) {
    const changed = structuredClone(workflow);
    mutate(changed, changed.jobs['adapter-platform'].steps.findIndex(step => step.name === requiredStep.name));
    assert.throws(() => withoutStabilityTests(changed));
  }
});

test('stability CI preserves the independent native recipe insertion point', () => {
  // Mirrors PR514's finite workflow addition, without importing its implementation or evidence.
  const combined = structuredClone(workflow);
  const recipe = { name: 'Verify native recipe policy hostile cases', run: 'node scripts/run-native-recipe-tests.mjs' };
  const steps = combined.jobs.rust.steps;
  let index = steps.findIndex(step => step.name === 'Fetch locked Rust dependencies');
  if (steps[index - 1].name === recipe.name) {
    assert.deepEqual(steps[index - 1], recipe);
    index -= 1;
  } else {
    steps.splice(index, 0, recipe);
  }
  assert.deepEqual(steps[index - 1], { run: 'pnpm install --frozen-lockfile' });
  const preserved = withoutStabilityTests(combined);
  assert.deepEqual(preserved.jobs.rust, combined.jobs.rust);
  assert.deepEqual(preserved.jobs.rust.steps[index - 1], { run: 'pnpm install --frozen-lockfile' });
  assert.deepEqual(preserved.jobs.rust.steps[index + 1], { name: 'Fetch locked Rust dependencies', run: 'cargo fetch --locked' });
});
