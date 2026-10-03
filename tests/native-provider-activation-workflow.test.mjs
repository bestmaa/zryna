import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import test from 'node:test';
import { parseDocument } from 'yaml';

const CONSUMER_SHA = 'f09f5abb69fe7e05352221c3a0302138ccde4984';
const CHECKOUT = 'actions/checkout@3d3c42e5aac5ba805825da76410c181273ba90b1';
const RUST = 'dtolnay/rust-toolchain@4360b52568e2003a75bf9bc1d59f33a8e3fc893c';
const UPLOAD = 'actions/upload-artifact@043fb46d1a93c77aae656e7c1c64a875d1fc6a0a';
const expression = (value) => '$' + '{{ ' + value + ' }}';
const HEAD = expression('github.event.pull_request.head.sha || github.sha');
const PIN = expression('env.ACTIVATION_CONSUMER_SHA');
const parsed = parseDocument(readFileSync(
  new URL('../.github/workflows/native-provider-activation.yml', import.meta.url), 'utf8',
));
assert.deepEqual(parsed.errors, []);
const workflow = parsed.toJS();

const expectedSteps = [
  {
    name: 'Checkout exact CI tooling', uses: CHECKOUT,
    with: { 'fetch-depth': 0, path: 'coverage', ref: HEAD },
  },
  {
    name: 'Checkout pinned activation consumer', uses: CHECKOUT,
    with: { 'fetch-depth': 0, path: 'consumer', ref: PIN },
  },
  { uses: RUST, with: { toolchain: '1.97.1', components: 'rustfmt, clippy' } },
  {
    name: 'Verify exact checkouts and Python requirement', shell: 'pwsh',
    env: { COVERAGE_HEAD: HEAD },
    run: [
      'python -c "import sys; assert sys.version_info >= (3, 11); print(sys.version)"',
      "if ($LASTEXITCODE -ne 0) { throw 'Python requirement failed' }",
      "foreach ($checkoutPath in @('coverage', 'consumer')) {",
      "  $expected = if ($checkoutPath -eq 'coverage') { $env:COVERAGE_HEAD } else { $env:ACTIVATION_CONSUMER_SHA }",
      '  $actual = git -C $checkoutPath rev-parse HEAD',
      "  if ($LASTEXITCODE -ne 0 -or $actual -ne $expected) { throw 'Exact checkout differs' }",
      '  $status = git -C $checkoutPath status --porcelain',
      "  if ($LASTEXITCODE -ne 0 -or $status) { throw 'Checkout is not clean' }",
      '}',
      '',
    ].join('\n'),
  },
  {
    name: 'Verify independent CI receipt controls',
    run: 'python coverage/tests/native-provider-activation-ci/verify_receipts_test.py',
  },
  {
    name: 'Fetch locked consumer dependencies', 'working-directory': 'consumer',
    run: 'cargo fetch --locked',
  },
  {
    name: 'Verify consumer runner controls', 'working-directory': 'consumer',
    run: 'python tests/native-provider-activation/runner_test.py',
  },
  {
    name: 'Run relocated native frontend smoke', 'working-directory': 'consumer', shell: 'pwsh',
    run: 'python scripts/run-native-provider-activation.py --evidence-dir "$env:RUNNER_TEMP/activation-frontend" --target-dir "$env:RUNNER_TEMP/activation-target"',
  },
  {
    name: 'Require exact frontend receipt', shell: 'pwsh',
    run: 'python coverage/tests/native-provider-activation-ci/verify_receipts.py --consumer consumer --evidence-dir "$env:RUNNER_TEMP/activation-frontend" --target-dir "$env:RUNNER_TEMP/activation-target"',
  },
  {
    name: 'Run relocated retained-source smoke', 'working-directory': 'consumer', shell: 'pwsh',
    run: 'python scripts/run-native-provider-activation.py --retained --evidence-dir "$env:RUNNER_TEMP/activation-retained" --target-dir "$env:RUNNER_TEMP/activation-target"',
  },
  {
    name: 'Require exact retained receipt', shell: 'pwsh',
    run: 'python coverage/tests/native-provider-activation-ci/verify_receipts.py --retained --consumer consumer --evidence-dir "$env:RUNNER_TEMP/activation-retained" --target-dir "$env:RUNNER_TEMP/activation-target"',
  },
  {
    name: 'Preserve exact consumer receipts and logs', if: 'always()', uses: UPLOAD,
    with: {
      name: 'native-activation-' + expression('matrix.os') + '-' + PIN,
      path: expression('runner.temp') + '/activation-frontend\n'
        + expression('runner.temp') + '/activation-retained\n',
      'if-no-files-found': 'error', 'retention-days': 7,
    },
  },
];
const expected = {
  name: 'Pinned native activation harness',
  on: {
    pull_request: {
      paths: [
        '.github/workflows/native-provider-activation.yml',
        'tests/native-provider-activation-ci/**',
        'tests/native-provider-activation-workflow.test.mjs',
        'tests/workflow-routing.test.mjs',
      ],
    },
    workflow_dispatch: null,
  },
  concurrency: {
    group: 'native-activation-' + expression('github.event.pull_request.number || github.ref'),
    'cancel-in-progress': true,
  },
  permissions: { contents: 'read' },
  env: { ACTIVATION_CONSUMER_SHA: CONSUMER_SHA },
  jobs: {
    smoke: {
      name: 'pinned native activation (' + expression('matrix.os') + ')',
      'timeout-minutes': expression("matrix.os == 'windows-latest' && 60 || 40"),
      strategy: { 'fail-fast': false, matrix: { os: ['ubuntu-latest', 'windows-latest'] } },
      'runs-on': expression('matrix.os'),
      steps: expectedSteps,
    },
  },
};

// A reviewed workflow is the authority: unknown fields, commands and jobs also reject.
function verify(candidate) {
  assert.deepEqual(candidate, expected);
}

function verifyOraclePin(candidate, source) {
  verify(candidate);
  const pins = [...source.matchAll(/^CONSUMER_SHA = "([0-9a-f]{40})"$/gmu)];
  assert.equal(pins.length, 1, 'CI oracle must declare one exact consumer');
  assert.equal(pins[0][1], candidate.env.ACTIVATION_CONSUMER_SHA);
}

function rejectMutations(mutations) {
  for (const [reason, mutate] of mutations) {
    const changed = structuredClone(workflow);
    mutate(changed);
    assert.throws(() => verify(changed), reason);
  }
}

test('native activation workflow freezes source provenance, both platforms and exact proof order', () => {
  verify(workflow);
  assert.match(CONSUMER_SHA, /^[0-9a-f]{40}$/u);
  const steps = workflow.jobs.smoke.steps;
  assert.equal(steps[8].run.includes('--retained'), false);
  assert.equal(steps[10].run.includes('--retained'), true);
  assert.equal(steps[8].run.includes('coverage/tests/'), true);
  assert.equal(steps[10].run.includes('coverage/tests/'), true);
  assert.equal(steps[7]['working-directory'], 'consumer');
  assert.equal(steps[9]['working-directory'], 'consumer');
  assert.equal(steps[8].name, 'Require exact frontend receipt');
  assert.equal(steps[9].name, 'Run relocated retained-source smoke',
    'frontend binary must be verified before retained compilation reuses its target');
});

test('independent CI oracle and workflow bind the same immutable consumer', () => {
  const source = readFileSync(new URL(
    './native-provider-activation-ci/verify_receipts.py', import.meta.url,
  ), 'utf8');
  verifyOraclePin(workflow, source);
  for (const changed of [
    source.replace(CONSUMER_SHA, 'a'.repeat(40)),
    source.replace(/^CONSUMER_SHA = .*$/mu, ''),
    source + '\nCONSUMER_SHA = "' + CONSUMER_SHA + '"\n',
    source.replace(CONSUMER_SHA, 'main'),
  ]) assert.throws(() => verifyOraclePin(workflow, changed));
});

test('floating source, incorrect checkout identity and substituted toolchains reject', () => {
  rejectMutations([
    ['floating consumer', (w) => { w.env.ACTIVATION_CONSUMER_SHA = 'main'; }],
    ['different exact consumer', (w) => { w.env.ACTIVATION_CONSUMER_SHA = 'a'.repeat(40); }],
    ['missing consumer identity', (w) => { delete w.env.ACTIVATION_CONSUMER_SHA; }],
    ['coverage executes consumer tooling', (w) => { w.jobs.smoke.steps[0].with.ref = PIN; }],
    ['consumer follows PR head', (w) => { w.jobs.smoke.steps[1].with.ref = HEAD; }],
    ['shallow history', (w) => { w.jobs.smoke.steps[1].with['fetch-depth'] = 1; }],
    ['checkouts collide', (w) => { w.jobs.smoke.steps[1].with.path = 'coverage'; }],
    ['checkout floats', (w) => { w.jobs.smoke.steps[0].uses = 'actions/checkout@main'; }],
    ['Rust floats', (w) => { w.jobs.smoke.steps[2].with.toolchain = 'stable'; }],
    ['Rust action floats', (w) => { w.jobs.smoke.steps[2].uses = 'dtolnay/rust-toolchain@stable'; }],
    ['Clippy omitted', (w) => { w.jobs.smoke.steps[2].with.components = 'rustfmt'; }],
    ['Python check suppressed', (w) => { w.jobs.smoke.steps[3].run = 'python --version'; }],
    ['wrong comparison identity', (w) => { w.jobs.smoke.steps[3].env.COVERAGE_HEAD = PIN; }],
    ['masked Python exit', (w) => {
      w.jobs.smoke.steps[3].run = w.jobs.smoke.steps[3].run.replace(
        "if ($LASTEXITCODE -ne 0) { throw 'Python requirement failed' }\n", '',
      );
    }],
  ]);
});

test('missing platform, weakened budgets and conditional or nonzero bypasses reject', () => {
  rejectMutations([
    ['Windows omitted', (w) => { w.jobs.smoke.strategy.matrix.os = ['ubuntu-latest']; }],
    ['unreviewed OS', (w) => { w.jobs.smoke.strategy.matrix.os[1] = 'windows-2022'; }],
    ['matrix not executed', (w) => { w.jobs.smoke['runs-on'] = 'ubuntu-latest'; }],
    ['fail-fast hides evidence', (w) => { w.jobs.smoke.strategy['fail-fast'] = true; }],
    ['unbounded job', (w) => { delete w.jobs.smoke['timeout-minutes']; }],
    ['Windows budget reduced', (w) => {
      w.jobs.smoke['timeout-minutes'] = expression("matrix.os == 'windows-latest' && 40 || 40");
    }],
    ['job skipped', (w) => { w.jobs.smoke.if = 'false'; }],
    ['job tolerates failure', (w) => { w.jobs.smoke['continue-on-error'] = true; }],
  ]);
  for (let index = 0; index < expectedSteps.length - 1; index++) {
    rejectMutations([
      ['step skipped ' + index, (w) => { w.jobs.smoke.steps[index].if = 'false'; }],
      ['step failure ignored ' + index, (w) => {
        w.jobs.smoke.steps[index]['continue-on-error'] = true;
      }],
      ['step removed ' + index, (w) => { w.jobs.smoke.steps.splice(index, 1); }],
      ['step duplicated ' + index, (w) => {
        w.jobs.smoke.steps.push(structuredClone(w.jobs.smoke.steps[index]));
      }],
    ]);
  }
});

test('each smoke requires independent exact receipt verification immediately afterward', () => {
  for (const [smoke, receipt] of [[7, 8], [9, 10]]) {
    rejectMutations([
      ['receipt checked before execution', (w) => {
        [w.jobs.smoke.steps[smoke], w.jobs.smoke.steps[receipt]]
          = [w.jobs.smoke.steps[receipt], w.jobs.smoke.steps[smoke]];
      }],
      ['receipt uses consumer-controlled verifier', (w) => {
        w.jobs.smoke.steps[receipt].run = w.jobs.smoke.steps[receipt].run
          .replace('coverage/tests/', 'consumer/tests/');
      }],
      ['receipt omission', (w) => { w.jobs.smoke.steps.splice(receipt, 1); }],
      ['receipt bypass', (w) => { w.jobs.smoke.steps[receipt].run += '; exit 0'; }],
      ['smoke lists instead of executes', (w) => { w.jobs.smoke.steps[smoke].run += ' --list'; }],
      ['evidence path substituted', (w) => {
        w.jobs.smoke.steps[receipt].run = w.jobs.smoke.steps[receipt].run
          .replace(/activation-(frontend|retained)/u, 'unrelated-evidence');
      }],
      ['binary target omitted', (w) => {
        w.jobs.smoke.steps[receipt].run = w.jobs.smoke.steps[receipt].run
          .replace(' --target-dir "$env:RUNNER_TEMP/activation-target"', '');
      }],
    ]);
  }
  rejectMutations([
    ['retained requirement omitted', (w) => {
      w.jobs.smoke.steps[10].run = w.jobs.smoke.steps[10].run.replace(' --retained', '');
    }],
    ['frontend requirement confused', (w) => {
      w.jobs.smoke.steps[8].run = w.jobs.smoke.steps[8].run.replace(' --consumer', ' --retained --consumer');
    }],
    ['dependencies unlocked', (w) => { w.jobs.smoke.steps[5].run = 'cargo fetch'; }],
    ['independent controls bypassed', (w) => { w.jobs.smoke.steps[4].run += ' || true'; }],
  ]);
});

test('publication, wider permissions and artifacts without exact consumer provenance reject', () => {
  rejectMutations([
    ['write permissions', (w) => { w.permissions.contents = 'write'; }],
    ['release permissions', (w) => { w.permissions['id-token'] = 'write'; }],
    ['secrets inherited', (w) => { w.jobs.smoke.secrets = 'inherit'; }],
    ['privileged trigger', (w) => { w.on.pull_request_target = w.on.pull_request; }],
    ['public activation requested', (w) => { w.env.PUBLIC_ACTIVATION = 'true'; }],
    ['public deployment added', (w) => { w.jobs.publish = { 'runs-on': 'ubuntu-latest', steps: [] }; }],
    ['superseded proof remains active', (w) => { w.concurrency['cancel-in-progress'] = false; }],
    ['artifact floats', (w) => { w.jobs.smoke.steps[11].uses = 'actions/upload-artifact@main'; }],
    ['failed proof evidence omitted', (w) => { delete w.jobs.smoke.steps[11].if; }],
    ['artifact claims PR source', (w) => {
      w.jobs.smoke.steps[11].with.name = 'native-activation-' + expression('matrix.os') + '-' + HEAD;
    }],
    ['one lane evidence omitted', (w) => {
      w.jobs.smoke.steps[11].with.path = expression('runner.temp') + '/activation-frontend\n';
    }],
    ['empty artifact accepted', (w) => { w.jobs.smoke.steps[11].with['if-no-files-found'] = 'ignore'; }],
    ['unreviewed retention', (w) => { w.jobs.smoke.steps[11].with['retention-days'] = 90; }],
    ['unrelated checkout uploaded', (w) => { w.jobs.smoke.steps[11].with.path += 'consumer\n'; }],
  ]);
});
