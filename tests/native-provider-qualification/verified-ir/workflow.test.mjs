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
const expected = structuredClone(baseline);
expected.steps.find((step) => step.uses?.startsWith('dtolnay/rust-toolchain@'))
  .with.components = 'rustfmt, clippy';
const insertion = expected.steps.findIndex((step) => step.name === 'Check private feature Clippy on current host');
assert.ok(insertion > 0);
expected.steps.splice(insertion, 0, ...steps);
const verify = (job) => assert.deepEqual(job, expected);

test('complete IR registration preserves the entire qualified H5 job and proof ordering', () => {
  verify(workflow.jobs['private-cli']);
  const registered = JSON.parse(readFileSync(new URL(
    '../../native-cli-smoke/workflow-job.json', import.meta.url,
  ), 'utf8'));
  verify(registered);
});

test('complete IR registration rejects omitted proof, host, pin and original-archive requirements', () => {
  const mutations = [
    (job) => job.steps.splice(insertion, 1),
    (job) => { job.steps[insertion + 1].run = job.steps[insertion + 1].run.replace('--rustup "$rustup"', ''); },
    (job) => { job.steps[insertion + 1].if = "matrix.os == 'ubuntu-latest'"; },
    (job) => { job.steps[insertion + 3].run = job.steps[insertion + 3].run.replace(' --live', ''); },
    (job) => { job.steps[insertion + 3].run = job.steps[insertion + 3].run.replace(' --run-attempt "$env:GITHUB_RUN_ATTEMPT"', ''); },
    (job) => { job.steps[insertion + 5].with.path = '${{ runner.temp }}/private-verified-ir-proof'; },
    (job) => { job.steps[insertion + 5].uses = 'actions/upload-artifact@main'; },
    (job) => { job.steps[insertion + 5].with['if-no-files-found'] = 'ignore'; },
    (job) => { job.steps[insertion + 5].if = 'success()'; },
    (job) => { job.steps[1].with.components = 'clippy'; },
    (job) => { job.strategy.matrix.os = ['ubuntu-latest']; },
  ];
  for (const mutate of mutations) {
    const changed = structuredClone(expected);
    mutate(changed);
    assert.throws(() => verify(changed));
  }
});
