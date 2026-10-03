import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import test from 'node:test';
import { parseDocument } from 'yaml';
import {
  REQUIRED_RESOURCE_TESTS, runNativeSourceResourceTests, verifyResourceTestOutput,
} from '../scripts/run-native-source-resource-tests.mjs';

const step = {
  name: 'Verify native source resource limits',
  if: "runner.os == 'Windows'",
  run: 'node scripts/run-native-source-resource-tests.mjs',
};

export function withoutNativeSourceResources(candidate) {
  const original = structuredClone(candidate);
  const steps = original.jobs.rust.steps;
  const selected = steps.filter((entry) => entry.name === step.name || entry.run === step.run);
  assert.equal(selected.length, 1);
  assert.deepEqual(selected[0], step);
  const index = steps.indexOf(selected[0]);
  assert.equal(steps[index - 1].run, 'cargo test --locked -p zryna-driver --lib');
  assert.equal(steps[index + 1].name, 'Verify exact Windows directory lifecycle');
  steps.splice(index, 1);
  return original;
}

const parsed = parseDocument(readFileSync(new URL('../.github/workflows/ci.yml', import.meta.url), 'utf8'));
assert.deepEqual(parsed.errors, []);
const workflow = parsed.toJS();
const output = `${REQUIRED_RESOURCE_TESTS.map((name) => `test ${name} ... ok`).join('\n')}
test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 345 filtered out;`;

test('Windows Rust job adds mandatory resource proof after the ordinary driver suite', () => {
  assert.doesNotThrow(() => withoutNativeSourceResources(workflow));
  assert.deepEqual(workflow.jobs.rust.strategy.matrix.os, ['ubuntu-latest', 'windows-latest']);
  const source = readFileSync(new URL('../crates/zryna-driver/src/module_closure/native_sources/tests/resources.rs', import.meta.url), 'utf8');
  assert.equal([...source.matchAll(/^#\[ignore =/gmu)].length, 4);
  const names = [...source.matchAll(/^fn (\w+)\(/gmu)]
    .map(([, name]) => `module_closure::native_sources::tests::resources::${name}`);
  assert.deepEqual(names, REQUIRED_RESOURCE_TESTS);
});

test('Windows resource condition, placement, bypass and duplicate mutations reject', () => {
  for (const mutate of [
    (s, i) => { s.splice(i, 1); },
    (s, i) => { s.push(structuredClone(s[i])); },
    (s, i) => { s[i].if = "runner.os == 'Linux'"; },
    (s, i) => { s[i].if = 'false'; },
    (s, i) => { s[i].run += ' --list'; },
    (s, i) => { s[i]['continue-on-error'] = true; },
    (s, i) => { [s[i], s[i + 1]] = [s[i + 1], s[i]]; },
  ]) {
    const changed = structuredClone(workflow);
    const steps = changed.jobs.rust.steps;
    mutate(steps, steps.findIndex((entry) => entry.name === step.name));
    assert.throws(() => withoutNativeSourceResources(changed));
  }
});

test('resource output requires each exact test once and a four-pass zero-ignore summary', () => {
  assert.doesNotThrow(() => verifyResourceTestOutput(output));
  assert.doesNotThrow(() => verifyResourceTestOutput(output.replaceAll('\n', '\r\n')));
  for (const forged of [
    '', 'test result: ok. 0 passed; 0 failed; 4 ignored;',
    output.replace(REQUIRED_RESOURCE_TESTS[0], 'unrelated_test'),
    output.replace('... ok', '... ignored'),
    output.replace('4 passed', '3 passed'),
    output.replace('0 failed', '1 failed'),
    output.replace('0 ignored', '1 ignored'),
    `${output}\n${output}`,
  ]) assert.throws(() => verifyResourceTestOutput(forged));
});

test('resource runner executes the locked ignored cases serially without a shell', () => {
  let called = 0;
  runNativeSourceResourceTests((executable, args, options) => {
    called += 1;
    assert.equal(executable, 'cargo');
    assert.deepEqual(args, [
      'test', '--locked', '-p', 'zryna-driver', '--lib',
      'module_closure::native_sources::tests::resources::',
      '--', '--ignored', '--test-threads=1',
    ]);
    assert.equal(options.shell, false);
    assert.equal(options.maxBuffer, 4 * 1024 * 1024);
    return { status: 0, stdout: output, stderr: '' };
  });
  assert.equal(called, 1);
});

test('spawn errors, nonzero exits and signals cannot satisfy resource proof', () => {
  for (const failed of [
    { status: 101 }, { status: null, signal: 'SIGTERM' },
    { status: 0, error: new Error('spawn failed') },
  ]) assert.throws(() => runNativeSourceResourceTests(() => ({ stdout: '', stderr: '', ...failed })));
});
