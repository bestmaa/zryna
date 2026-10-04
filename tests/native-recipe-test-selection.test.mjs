import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import test from 'node:test';
import { parseDocument } from 'yaml';

import {
  NATIVE_RECIPE_TEST_FILES, REQUIRED_NATIVE_RECIPE_TESTS,
  runNativeRecipeTests, verifyNativeRecipeTestOutput,
} from '../scripts/run-native-recipe-tests.mjs';

const requiredStep = Object.freeze({
  name: 'Verify native recipe policy hostile cases',
  run: 'node scripts/run-native-recipe-tests.mjs',
});

export function withoutNativeRecipeTests(candidate) {
  const original = structuredClone(candidate);
  const job = original.jobs.rust;
  assert.equal(job.if, undefined);
  assert.equal(job['continue-on-error'], undefined);
  assert.deepEqual(job.strategy.matrix, { os: ['ubuntu-latest', 'windows-latest'] });
  assert.equal(job.strategy['fail-fast'], false);
  assert.equal(job['runs-on'], '${{ matrix.os }}');
  const matches = job.steps.flatMap((step, index) => step.name === requiredStep.name ? [index] : []);
  assert.equal(matches.length, 1);
  const index = matches[0];
  assert.deepEqual(job.steps[index], requiredStep);
  assert.deepEqual(job.steps[index - 1], { run: 'pnpm install --frozen-lockfile' });
  assert.deepEqual(job.steps[index + 1], {
    name: 'Fetch locked Rust dependencies', run: 'cargo fetch --locked',
  });
  assert(original.jobs.m0.needs.includes('rust'));
  assert.equal(original.jobs.m0.steps.at(-1).env.RUST_RESULT, '${{ needs.rust.result }}');
  assert.match(original.jobs.m0.steps.at(-1).run, /test "\$RUST_RESULT" = success/u);
  job.steps.splice(index, 1);
  return original;
}

function passedOutput() {
  return ['TAP version 13', ...REQUIRED_NATIVE_RECIPE_TESTS.map((name, index) =>
    `ok ${index + 1} - ${name.replaceAll('#', '\\#')}`),
    '1..17', '# tests 17', '# suites 0', '# pass 17', '# fail 0',
    '# cancelled 0', '# skipped 0', '# todo 0', ''].join('\n');
}

test('recipe CI selects only the three existing files and all 17 distinct cases', () => {
  assert.deepEqual(NATIVE_RECIPE_TEST_FILES, [
    'tests/native-recipe-identity.test.mjs', 'tests/native-recipe-materials.test.mjs',
    'tests/native-recipe-provenance.test.mjs',
  ]);
  assert.equal(REQUIRED_NATIVE_RECIPE_TESTS.length, 17);
  assert.equal(new Set(REQUIRED_NATIVE_RECIPE_TESTS).size, 17);
  assert(Object.isFrozen(NATIVE_RECIPE_TEST_FILES));
  assert(Object.isFrozen(REQUIRED_NATIVE_RECIPE_TESTS));
  const declared = NATIVE_RECIPE_TEST_FILES.flatMap(file => [
    ...readFileSync(new URL(`../${file}`, import.meta.url), 'utf8').matchAll(/^test\('([^']+)',/gmu),
  ].map(match => match[1]));
  assert.deepEqual(declared, REQUIRED_NATIVE_RECIPE_TESTS);
  const registry = readFileSync(new URL('preflight.test.mjs', import.meta.url), 'utf8');
  assert.match(registry, /^import '\.\/native-recipe-test-selection\.test\.mjs';$/mu);
});

test('recipe selection evidence rejects omitted, renamed, duplicated and unexpected cases', () => {
  const output = passedOutput();
  assert.doesNotThrow(() => verifyNativeRecipeTestOutput(output));
  for (const name of REQUIRED_NATIVE_RECIPE_TESTS) {
    const reportedName = name.replaceAll('#', '\\#');
    const line = output.split('\n').find(value => value.endsWith(` - ${reportedName}`));
    for (const changed of [
      output.replace(`${line}\n`, ''), output.replace(line, `${line}\n${line}`),
      output.replace(line, line.replace(reportedName, 'unregistered case')),
      output.replace(line, line.replace(/^ok/u, 'not ok')),
      output.replace(line, `${line} # SKIP omitted`), output.replace(line, `${line} # TODO omitted`),
    ]) assert.throws(() => verifyNativeRecipeTestOutput(changed));
  }
  assert.throws(() => verifyNativeRecipeTestOutput(`${output}ok 18 - unregistered case\n`));
  assert.throws(() => verifyNativeRecipeTestOutput(output.replace('ok 2 - ', 'ok 1 - ')));
});

test('recipe selection evidence rejects zero, partial, skipped and ambiguous summaries', () => {
  const output = passedOutput();
  for (const summary of ['TAP version 13', '1..17', '# tests 17', '# suites 0', '# pass 17',
    '# fail 0', '# cancelled 0', '# skipped 0', '# todo 0']) {
    for (const changed of [output.replace(`${summary}\n`, ''), `${output}${summary}\n`,
      output.replace(summary, summary.replace(/\d+/u, '999')),
      `${output}${summary.replace(/\d+/u, '999')}\n`]) {
      assert.throws(() => verifyNativeRecipeTestOutput(changed));
    }
  }
  for (const output of ['', 'TAP version 13\n1..0\n# tests 0\n# pass 0\n',
    'ok 1 - something else\n1..1\n# tests 1\n# pass 1\n']) {
    assert.throws(() => verifyNativeRecipeTestOutput(output));
  }
});

test('recipe runner propagates process failure and uses exact direct Node selection', () => {
  const sink = { write() {} };
  const spawn = (executable, args, options) => {
    assert.equal(executable, process.execPath);
    assert.deepEqual(args, ['--test', '--test-reporter=tap', ...NATIVE_RECIPE_TEST_FILES]);
    assert.equal(options.shell, false);
    assert.equal(options.windowsHide, true);
    return { status: 0, stdout: passedOutput() };
  };
  assert.doesNotThrow(() => runNativeRecipeTests(spawn, sink));
  for (const result of [{ status: 1, stdout: passedOutput() }, { status: null, stdout: passedOutput() },
    { status: 0, stdout: '' }, { status: 0, stdout: '', stderr: passedOutput() },
    { status: 0, stdout: passedOutput(), error: new Error('could not start') }]) {
    assert.throws(() => runNativeRecipeTests(() => result, sink));
  }
});

test('both existing OS authorities run recipe tests unconditionally before Rust security', () => {
  const document = parseDocument(readFileSync(new URL('../.github/workflows/ci.yml', import.meta.url), 'utf8'));
  assert.deepEqual(document.errors, []);
  assert.doesNotThrow(() => withoutNativeRecipeTests(document.toJS()));
});

test('recipe CI removal, bypass, duplication, relocation and OS omission fail closed', () => {
  const candidate = parseDocument(readFileSync(new URL('../.github/workflows/ci.yml', import.meta.url), 'utf8')).toJS();
  for (const mutate of [
    (w, index) => { w.jobs.rust.steps.splice(index, 1); },
    (w, index) => { w.jobs.rust.steps[index].if = "runner.os == 'Linux'"; },
    (w, index) => { w.jobs.rust.steps[index]['continue-on-error'] = true; },
    (w, index) => { w.jobs.rust.steps[index].run += ' || true'; },
    (w, index) => { w.jobs.rust.steps[index].run += ' --test-name-pattern=unknown'; },
    (w, index) => { w.jobs.rust.steps.push(w.jobs.rust.steps[index]); },
    (w, index) => { const step = w.jobs.rust.steps.splice(index, 1)[0]; w.jobs.rust.steps.push(step); },
    w => { w.jobs.rust.strategy.matrix.os.pop(); },
    w => { w.jobs.rust.if = 'false'; },
    w => { w.jobs.rust['continue-on-error'] = true; },
    w => { w.jobs.m0.needs = w.jobs.m0.needs.filter(id => id !== 'rust'); },
    w => { w.jobs.m0.steps.at(-1).run = 'true'; },
  ]) {
    const changed = structuredClone(candidate);
    mutate(changed, changed.jobs.rust.steps.findIndex(step => step.name === requiredStep.name));
    assert.throws(() => withoutNativeRecipeTests(changed));
  }
});
