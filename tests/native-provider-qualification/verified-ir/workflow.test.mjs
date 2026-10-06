import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import test from 'node:test';
import { parseDocument } from 'yaml';

const root = fileURLToPath(new URL('../../../', import.meta.url));
const parsed = parseDocument(readFileSync(new URL(
  '../../../.github/workflows/native-provider-activation.yml', import.meta.url,
), 'utf8'));
assert.deepEqual(parsed.errors, []);
const workflow = parsed.toJS();
const baseline = JSON.parse(execFileSync('git', [
  'show', 'ec0cab5b4669dedd3e41fddda45449db34f73ce0:tests/native-cli-smoke/workflow-job.json',
], { cwd: root, encoding: 'utf8' }));
const block = (lines) => [...lines, ''].join('\n');
const steps = [
  {
    name: 'Verify complete IR workflow and archive controls', shell: 'pwsh',
    run: block([
      'node --test tests/native-provider-activation-workflow.test.mjs',
      "if ($LASTEXITCODE -ne 0) { throw 'Complete IR workflow contract failed' }",
      'python -B tests/native-provider-qualification/verified-ir/archive_test.py',
      "if ($LASTEXITCODE -ne 0) { throw 'Complete IR archive controls failed' }",
    ]),
  },
  {
    name: 'Run complete sealed IR observations', shell: 'pwsh',
    run: block([
      '$cargo = (Get-Command cargo).Source',
      '$rustup = (Get-Command rustup).Source',
      '$node = (Get-Command node).Source',
      'python -B tests/native-provider-qualification/verified-ir/run.py --evidence-dir "$env:RUNNER_TEMP/private-verified-ir-proof" --baseline-dir "$env:GITHUB_WORKSPACE/tests/native-provider-qualification/verified-ir/baseline-H4" --cargo "$cargo" --rustup "$rustup" --node "$node" --target-dir "$env:RUNNER_TEMP/private-verified-ir-target"',
      "if ($LASTEXITCODE -ne 0) { throw 'Complete sealed IR proof failed' }",
    ]),
  },
  {
    name: 'Verify current sealed IR admission controls', shell: 'pwsh',
    run: block([
      "$platform = if ($IsWindows) { 'win32' } else { 'linux' }",
      'python -B tests/native-provider-qualification/verified-ir/admission_test.py --root "$env:GITHUB_WORKSPACE" --head "$env:CLI_SOURCE_SHA" --platform "$platform" --proof "$env:RUNNER_TEMP/private-verified-ir-proof" --run-id "$env:GITHUB_RUN_ID" --run-attempt "$env:GITHUB_RUN_ATTEMPT"',
      "if ($LASTEXITCODE -ne 0) { throw 'Complete sealed IR admission controls failed' }",
    ]),
  },
  {
    name: 'Require independent current sealed IR admission', shell: 'pwsh',
    run: block([
      "$platform = if ($IsWindows) { 'win32' } else { 'linux' }",
      'python -B tests/native-provider-qualification/verified-ir/admission.py --root "$env:GITHUB_WORKSPACE" --head "$env:CLI_SOURCE_SHA" --platform "$platform" --proof "$env:RUNNER_TEMP/private-verified-ir-proof" --live --run-id "$env:GITHUB_RUN_ID" --run-attempt "$env:GITHUB_RUN_ATTEMPT"',
      "if ($LASTEXITCODE -ne 0) { throw 'Complete sealed IR admission failed' }",
    ]),
  },
  {
    name: 'Package complete sealed IR archive', shell: 'pwsh',
    run: block([
      "$platform = if ($IsWindows) { 'win32' } else { 'linux' }",
      'python -B tests/native-provider-qualification/verified-ir/archive.py pack --root "$env:GITHUB_WORKSPACE" --head "$env:CLI_SOURCE_SHA" --platform "$platform" --proof "$env:RUNNER_TEMP/private-verified-ir-proof" --archive "$env:RUNNER_TEMP/private-verified-ir-proof.zip"',
      "if ($LASTEXITCODE -ne 0) { throw 'Complete sealed IR archive failed' }",
    ]),
  },
  {
    name: 'Preserve complete sealed IR observations', if: 'always()',
    uses: 'actions/upload-artifact@043fb46d1a93c77aae656e7c1c64a875d1fc6a0a',
    with: {
      name: 'private-verified-ir-${{ matrix.os }}-${{ env.CLI_SOURCE_SHA }}-${{ github.run_id }}-${{ github.run_attempt }}',
      path: '${{ runner.temp }}/private-verified-ir-proof.zip',
      'if-no-files-found': 'error', 'retention-days': 7,
    },
  },
];
const expectedCli = structuredClone(baseline);
expectedCli.steps.find((step) => step.uses?.startsWith('dtolnay/rust-toolchain@'))
  .with.components = 'rustfmt, clippy';
const setup = structuredClone(expectedCli.steps.slice(0, 6));
setup[0].name = 'Checkout exact current IR source';
setup[5].name = 'Fetch locked current IR dependencies';
const capabilityAdmission = {
  name: 'Verify current tool capability admission controls', shell: 'pwsh',
  run: block([
    "$platform = if ($IsWindows) { 'win32' } else { 'linux' }",
    'python -B tests/native-provider-qualification/verified-ir/tool_capability_admission_test.py --root "$env:GITHUB_WORKSPACE" --head "$env:CLI_SOURCE_SHA" --platform "$platform" --proof "$env:RUNNER_TEMP/private-verified-ir-proof" --run-id "$env:GITHUB_RUN_ID" --run-attempt "$env:GITHUB_RUN_ATTEMPT"',
    "if ($LASTEXITCODE -ne 0) { throw 'Tool capability admission controls failed' }",
  ]),
};
export const currentIrJob = {
  name: 'private complete IR (${{ matrix.os }})',
  'timeout-minutes': expectedCli['timeout-minutes'],
  strategy: structuredClone(expectedCli.strategy),
  'runs-on': expectedCli['runs-on'],
  env: structuredClone(expectedCli.env),
  steps: [...setup, {
    name: 'Verify current tool capability controls',
    run: 'python -B tests/native-provider-qualification/verified-ir/tool_capability_test.py',
  }, ...steps.slice(0, 3), capabilityAdmission, ...steps.slice(3)],
};
const verify = (job) => assert.deepEqual(job, currentIrJob);

test('dedicated IR job retains every original proof step and host requirement', () => {
  verify(workflow.jobs['private-ir']);
  verify(JSON.parse(readFileSync(new URL('./current-ir-workflow-job.json', import.meta.url), 'utf8')));
});

test('CLI job preserves every qualified H5 command and its original time bound', () => {
  assert.deepEqual(workflow.jobs['private-cli'], expectedCli);
  assert.deepEqual(JSON.parse(readFileSync(new URL('../../native-cli-smoke/workflow-job.json', import.meta.url), 'utf8')), expectedCli);
});

test('dedicated IR job rejects omitted proof, controls, host, pin and archive requirements', () => {
  const insertion = setup.length + 1;
  const mutations = [
    (job) => job.steps.splice(insertion, 1),
    (job) => job.steps.splice(setup.length, 1),
    (job) => { job.steps[insertion + 1].run = job.steps[insertion + 1].run.replace('--rustup "$rustup"', ''); },
    (job) => { job.steps[insertion + 1].if = "matrix.os == 'ubuntu-latest'"; },
    (job) => job.steps.splice(insertion + 3, 1),
    (job) => { job.steps[insertion + 4].run = job.steps[insertion + 4].run.replace(' --live', ''); },
    (job) => { job.steps[insertion + 4].run = job.steps[insertion + 4].run.replace(' --run-attempt "$env:GITHUB_RUN_ATTEMPT"', ''); },
    (job) => { job.steps[insertion + 6].with.path = '${{ runner.temp }}/private-verified-ir-proof'; },
    (job) => { job.steps[insertion + 6].uses = 'actions/upload-artifact@main'; },
    (job) => { job.steps[insertion + 6].with['if-no-files-found'] = 'ignore'; },
    (job) => { job.steps[insertion + 6].if = 'success()'; },
    (job) => { job.steps[1].with.components = 'clippy'; },
    (job) => { job.steps[2].with['node-version'] = '22'; },
    (job) => { job.steps[3].with.version = 'latest'; },
    (job) => { job.strategy.matrix.os = ['ubuntu-latest']; },
    (job) => { job['timeout-minutes'] = 90; },
    (job) => { job.steps[4].run = 'pnpm install'; },
  ];
  for (const mutate of mutations) {
    const changed = structuredClone(currentIrJob);
    mutate(changed);
    assert.throws(() => verify(changed));
  }
});
