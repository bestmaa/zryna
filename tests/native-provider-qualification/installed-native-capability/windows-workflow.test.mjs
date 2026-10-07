import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import test from 'node:test';
import { parseDocument } from 'yaml';

export const privateWindowsJob = JSON.parse(readFileSync(
  new URL('./windows-workflow-job.json', import.meta.url), 'utf8',
));
const parsed = parseDocument(readFileSync(new URL(
  '../../../.github/workflows/native-provider-activation.yml', import.meta.url,
), 'utf8'));
assert.deepEqual(parsed.errors, []);
const actual = parsed.toJS().jobs['private-installed-windows'];

test('Windows installation uses exact source, observed tool pins and independent admission', () => {
  assert.deepEqual(actual, privateWindowsJob);
  assert.equal(privateWindowsJob['timeout-minutes'], 60);
  assert.equal(privateWindowsJob['runs-on'], 'windows-2022');
  assert.equal(privateWindowsJob.steps[1].with.toolchain, '1.97.1');
  assert.match(privateWindowsJob.steps[3].run, /Python\\3\.12\.10\\x64\\python\.exe/u);
  assert.match(privateWindowsJob.steps[3].run, /rustup which --toolchain 1\.97\.1 cargo/u);
  assert.match(privateWindowsJob.steps[3].run, /rustup which --toolchain 1\.97\.1 rustc/u);
  assert.match(privateWindowsJob.steps[4].run, /admission_test\.py/u);
  assert.match(privateWindowsJob.steps[4].run, /admission\.py.*--live/u);
  assert.equal(privateWindowsJob.steps[5].if, 'always()');
  assert.match(privateWindowsJob.steps[5].with.name, /^unadmitted-/u);
  assert.equal(privateWindowsJob.steps[5].with['include-hidden-files'], true);
});

test('unknown job fields, omitted requirements and substituted host or tool pins reject', () => {
  const mutations = [
    (job) => { job.extra = 'unreviewed'; },
    (job) => { job['timeout-minutes'] = 90; },
    (job) => { job['runs-on'] = 'windows-latest'; },
    (job) => { job.steps[0].with.ref = 'main'; },
    (job) => { job.steps[1].with.toolchain = 'stable'; },
    (job) => { job.steps[3].run = job.steps[3].run.replace('3.12.10', '3.13'); },
    (job) => { job.steps[4].run = job.steps[4].run.replace('--live', ''); },
    (job) => { job.steps[5].if = 'success()'; },
    (job) => { job.steps[5].with['include-hidden-files'] = false; },
    (job) => { job.steps[5].with['if-no-files-found'] = 'ignore'; },
    ...privateWindowsJob.steps.map((_, index) => (job) => job.steps.splice(index, 1)),
  ];
  for (const mutate of mutations) {
    const changed = structuredClone(privateWindowsJob);
    mutate(changed);
    assert.throws(() => assert.deepEqual(changed, privateWindowsJob));
  }
});
