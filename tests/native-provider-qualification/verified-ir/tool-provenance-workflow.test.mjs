import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import test from 'node:test';
import { parseDocument } from 'yaml';

const expression = (value) => '$' + '{{ ' + value + ' }}';
const block = (lines) => [...lines, ''].join('\n');
export const toolProvenanceJob = {
  name: 'unqualified tool path diagnostic (' + expression('matrix.os') + ')',
  'timeout-minutes': 10,
  strategy: { 'fail-fast': false, matrix: { os: ['ubuntu-latest', 'windows-latest'] } },
  'runs-on': expression('matrix.os'),
  env: { CLI_SOURCE_SHA: expression('github.event.pull_request.head.sha || github.sha') },
  steps: [
    {
      name: 'Checkout exact diagnostic source',
      uses: 'actions/checkout@3d3c42e5aac5ba805825da76410c181273ba90b1',
      with: { 'fetch-depth': 0, ref: expression('env.CLI_SOURCE_SHA') },
    },
    {
      uses: 'dtolnay/rust-toolchain@4360b52568e2003a75bf9bc1d59f33a8e3fc893c',
      with: { toolchain: '1.97.1', components: 'rustfmt, clippy' },
    },
    {
      uses: 'actions/setup-node@820762786026740c76f36085b0efc47a31fe5020',
      with: { 'node-version': '22.22.1' },
    },
    {
      uses: 'pnpm/action-setup@0977fd99725f1db4007ccb2928dbb4e90d06cc86',
      with: { version: '11.18.0' },
    },
    {
      name: 'Verify exact tool diagnostic source and controls', shell: 'pwsh',
      run: block([
        'python -c "import sys; assert sys.version_info >= (3, 11); print(sys.version)"',
        "if ($LASTEXITCODE -ne 0) { throw 'Python requirement failed' }",
        '$actual = git rev-parse HEAD',
        "if ($LASTEXITCODE -ne 0 -or $actual -ne $env:CLI_SOURCE_SHA) { throw 'Exact diagnostic checkout differs' }",
        '$status = git status --porcelain',
        "if ($LASTEXITCODE -ne 0 -or $status) { throw 'Diagnostic checkout is not clean' }",
        'python -B tests/native-provider-qualification/verified-ir/tool_provenance_test.py',
        "if ($LASTEXITCODE -ne 0) { throw 'Tool diagnostic controls failed' }",
      ]),
    },
    {
      name: 'Record unqualified original tool path provenance', shell: 'pwsh',
      run: block([
        '$cargo = (Get-Command cargo).Source',
        '$rustup = (Get-Command rustup).Source',
        '$node = (Get-Command node).Source',
        'python -B tests/native-provider-qualification/verified-ir/tool_provenance.py --cargo "$cargo" --rustup "$rustup" --node "$node" --output "$env:RUNNER_TEMP/unqualified-tool-provenance.json"',
        "if ($LASTEXITCODE -ne 0) { throw 'Tool provenance capture failed' }",
      ]),
    },
    {
      name: 'Preserve unqualified tool path diagnosis', if: 'always()',
      uses: 'actions/upload-artifact@043fb46d1a93c77aae656e7c1c64a875d1fc6a0a',
      with: {
        name: 'unqualified-tool-provenance-${{ matrix.os }}-${{ env.CLI_SOURCE_SHA }}-${{ github.run_id }}-${{ github.run_attempt }}',
        path: '${{ runner.temp }}/unqualified-tool-provenance.json',
        'if-no-files-found': 'error', 'retention-days': 7,
      },
    },
  ],
};
const verify = (job) => assert.deepEqual(job, toolProvenanceJob);

test('tool provenance diagnostic has exact pinned host, source and unqualified retention scope', () => {
  const parsed = parseDocument(readFileSync(new URL(
    '../../../.github/workflows/native-provider-activation.yml', import.meta.url,
  ), 'utf8'));
  assert.deepEqual(parsed.errors, []);
  verify(parsed.toJS().jobs['tool-provenance']);
});

test('tool provenance diagnostic rejects suppressed, replaced or misclassified original observations', () => {
  const mutations = [
    (job) => { job.strategy['fail-fast'] = true; },
    (job) => { job.strategy.matrix.os = ['ubuntu-latest']; },
    (job) => { job.steps[0].with.ref = 'main'; },
    (job) => { job.steps[1].with.toolchain = 'stable'; },
    (job) => { job.steps[2].with['node-version'] = '22'; },
    (job) => { job.steps[3].with.version = 'latest'; },
    (job) => { job.steps[5].run = job.steps[5].run.replace('--cargo "$cargo"', ''); },
    (job) => { job.steps[5].run += 'exit 0\n'; },
    (job) => { job.steps[5]['continue-on-error'] = true; },
    (job) => { job.steps[6].if = 'success()'; },
    (job) => { job.steps[6].with['if-no-files-found'] = 'ignore'; },
    (job) => { job.steps[6].with.name = 'private-verified-ir'; },
  ];
  for (const mutate of mutations) {
    const changed = structuredClone(toolProvenanceJob);
    mutate(changed);
    assert.throws(() => verify(changed));
  }
});
