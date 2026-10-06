import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import test from 'node:test';
import './native-provider-qualification/verified-ir/workflow.test.mjs';
import { toolProvenanceJob } from './native-provider-qualification/verified-ir/tool-provenance-workflow.test.mjs';
import { parseDocument } from 'yaml';

const CONSUMER_SHA = '6c0f3f64a1278a53a61e2d732ef5e43246ab826f';
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
const privateCliJob = JSON.parse(readFileSync(new URL(
  './native-cli-smoke/workflow-job.json', import.meta.url,
), 'utf8'));

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
        'scripts/run-native-cli-smoke.py',
        'tests/native-cli-smoke/**',
        'tests/native-provider-qualification/**',
        'apps/zryna/src/**',
        'apps/zryna/Cargo.toml',
        'crates/**',
        'Cargo.toml',
        'Cargo.lock',
        'rust-toolchain.toml',
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
    'private-cli': privateCliJob,
    'tool-provenance': toolProvenanceJob,
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

test('current CLI proof binds both hosts and exact source separately from immutable consumer', () => {
  verify(workflow);
  const job = workflow.jobs['private-cli'];
  assert.deepEqual(job.env, { CLI_SOURCE_SHA: HEAD });
  assert.deepEqual(job.strategy, {
    'fail-fast': false, matrix: { os: ['ubuntu-latest', 'windows-latest'] },
  });
  assert.equal(job['runs-on'], expression('matrix.os'));
  assert.equal(job['continue-on-error'], undefined);
  assert.equal(job.if, undefined);
  assert.equal(job.steps[0].with.ref, expression('env.CLI_SOURCE_SHA'));
  const proof = job.steps.findIndex((step) => step.name === 'Build both current CLIs and run exact private smoke');
  assert(proof > 0);
  assert.match(job.steps[proof].run, /rustup which cargo/u);
  assert.match(job.steps[proof].run, /rustup which rustc/u);
  assert.match(job.steps[proof].run, /if \(\$LASTEXITCODE -ne 0\)/u);
  assert.equal(job.steps[proof + 1].name, 'Require independently verified current CLI receipt');
  assert.match(job.steps[proof + 1].run, /verify_ci_receipts\.py/u);
  assert.match(job.steps[proof + 1].run, /--head "\$env:CLI_SOURCE_SHA"/u);
  for (const step of job.steps) {
    assert.equal(step['continue-on-error'], undefined);
    if (step.uses?.startsWith('actions/upload-artifact@')) {
      assert.equal(step.if, step.name === 'Preserve exact real physical allocation proof' ? "always() && matrix.os == 'ubuntu-latest'" : 'always()');
      assert(step.with.name.includes(expression('env.CLI_SOURCE_SHA')));
      assert(step.with.name.includes(expression('github.run_id')));
      assert(step.with.name.includes(expression('github.run_attempt')));
      assert.equal(step.with['if-no-files-found'], 'error');
    } else assert.equal(step.if, ['Verify independent physical allocation controls', 'Run handles real physical allocation group', 'Require independent real physical allocation observations'].includes(step.name) ? "matrix.os == 'ubuntu-latest'" : undefined);
  }
  rejectMutations([
    ['current proof uses historical source', (w) => { w.jobs['private-cli'].env.CLI_SOURCE_SHA = PIN; }],
    ['current checkout uses consumer', (w) => { w.jobs['private-cli'].steps[0].with.ref = PIN; }],
    ['historical checkout uses current proof', (w) => { w.jobs.smoke.steps[1].with.ref = HEAD; }],
    ['private Windows omitted', (w) => { w.jobs['private-cli'].strategy.matrix.os.pop(); }],
    ['private proof skipped', (w) => { w.jobs['private-cli'].if = 'false'; }],
    ['private proof failure waived', (w) => { w.jobs['private-cli']['continue-on-error'] = true; }],
    ['CLI post-verification omitted', (w) => { w.jobs['private-cli'].steps.splice(proof + 1, 1); }],
    ['receipt admission precedes proof', (w) => {
      const steps = w.jobs['private-cli'].steps;
      [steps[proof], steps[proof + 1]] = [steps[proof + 1], steps[proof]];
    }],
    ['current proof claims consumer artifacts', (w) => {
      w.jobs['private-cli'].steps.at(-2).with.name = w.jobs.smoke.steps.at(-1).with.name;
    }],
    ['native exit masked', (w) => {
      w.jobs['private-cli'].steps[proof].run = w.jobs['private-cli'].steps[proof].run
        .replace("if ($LASTEXITCODE -ne 0) { throw 'Private CLI proof failed' }", '');
    }],
  ]);
});

test('cold qualification requires real fault execution and independent admission on both hosts', () => {
  const steps = workflow.jobs['private-cli'].steps;
  const execute = steps.findIndex((step) => step.name === 'Run current cold BUILD and retained Vec fault execution');
  assert(execute > 0);
  assert.equal(steps[execute - 1].name, 'Verify cold qualification hostile receipt controls');
  assert.equal(steps[execute + 1].name, 'Require independent cold BUILD and frozen fault receipts');
  assert.match(steps[execute].run, /native-provider-qualification\/run\.py/u);
  assert.match(steps[execute].run, /--head "\$env:CLI_SOURCE_SHA"/u);
  assert.match(steps[execute].run, /--cold-root "\$env:RUNNER_TEMP\/private-cold-source"/u);
  assert.match(steps[execute + 1].run, /native-provider-qualification\/verify\.py/u);
  assert.match(steps[execute + 1].run, /'win32'/u);
  rejectMutations([
    ['cold execution omitted', (w) => { w.jobs['private-cli'].steps.splice(execute, 1); }],
    ['cold receipt skipped', (w) => { w.jobs['private-cli'].steps[execute + 1].if = 'false'; }],
    ['cold failures ignored', (w) => { w.jobs['private-cli'].steps[execute].run += ' || true'; }],
    ['cold scope made native execution', (w) => { w.jobs['private-cli'].steps[execute].run += ' --windows-native-run'; }],
  ]);
});

test('remaining frozen fault corpus has separate exact-head execution, admission and archive', () => {
  const steps = workflow.jobs['private-cli'].steps;
  const cold = steps.findIndex((step) => step.name === 'Require independent cold BUILD and frozen fault receipts');
  const controls = steps.findIndex((step) => step.name === 'Verify independent finite fault corpus controls');
  const execute = steps.findIndex((step) => step.name === 'Run remaining frozen M3 fault groups');
  const admit = steps.findIndex((step) => step.name === 'Require independent remaining frozen fault observations');
  const archive = steps.findIndex((step) => step.name === 'Preserve exact remaining frozen fault corpus');
  assert.ok(cold < controls && controls < execute && execute < admit && admit < archive);
  assert.equal(steps[controls].run, 'python -B tests/native-provider-qualification/corpus/verify_test.py');
  assert.match(steps[execute].run, /corpus\/run\.py --root "\$env:GITHUB_WORKSPACE" --head "\$env:CLI_SOURCE_SHA"/u);
  assert.match(steps[execute].run, /--target "\$env:RUNNER_TEMP\/private-cli-target"/u);
  assert.match(steps[execute].run, /--output "\$env:RUNNER_TEMP\/private-fault-corpus-proof"/u);
  assert.match(steps[admit].run, /corpus\/verify\.py/u);
  assert.match(steps[admit].run, /throw 'Finite retained fault corpus admission failed'/u);
  assert.equal(steps[archive].if, 'always()');
  assert.equal(steps[archive].with.path, expression('runner.temp') + '/private-fault-corpus-proof');
  assert.equal(steps[archive].with.name,
    'private-native-fault-corpus-' + expression('matrix.os') + '-' + expression('env.CLI_SOURCE_SHA')
      + '-' + expression('github.run_id') + '-' + expression('github.run_attempt'));
});

test('seven accepted BUILD closures require both-host controls, execution, live admission and archive', () => {
  const steps = workflow.jobs['private-cli'].steps;
  const index = steps.findIndex((s) => s.name === 'Verify independent seven-source manifest controls');
  assert(index > 0);
  assert.equal(steps[index - 1].name, 'Require independently verified current CLI receipt');
  assert.equal(steps[index].run, 'python -B tests/native-provider-qualification/production-manifests/verify_test.py');
  assert.equal(steps[index + 1].name, 'Verify independent production archive controls');
  assert.equal(steps[index + 1].run, 'python -B tests/native-provider-qualification/production-manifests/archive_test.py');
  assert.equal(steps[index + 2].name, 'Run seven accepted production manifest pairs');
  assert.match(steps[index + 2].run, /production-manifests\/run\.py/u);
  assert.match(steps[index + 2].run, /--cli-proof "\$env:RUNNER_TEMP\/private-cli-proof"/u);
  assert.equal(steps[index + 3].name, 'Require independent seven-source manifest admission');
  assert.match(steps[index + 3].run, /--platform "\$platform".*--live/u);
  assert.match(steps[index + 3].run, /'win32'/u);
  assert.equal(steps[index + 4].name, 'Package complete production manifest archive');
  assert.match(steps[index + 4].run, /production-manifests\/archive\.py pack/u);
  assert.match(steps[index + 4].run, /--archive "\$env:RUNNER_TEMP\/private-production-manifests\.zip"/u);
  assert.equal(steps[index + 5].name, 'Preserve seven accepted production manifest pairs');
  assert.equal(steps[index + 5].with.path, expression('runner.temp') + '/private-production-manifests.zip');
  assert.equal(steps[index + 5].with.name, 'private-production-manifests-'
    + expression('matrix.os') + '-' + expression('env.CLI_SOURCE_SHA') + '-'
    + expression('github.run_id') + '-' + expression('github.run_attempt'));
  for (let offset = 0; offset < 5; offset++) assert.equal(steps[index + offset].if, undefined);
  assert.equal(steps[index + 5].if, 'always()');
  rejectMutations([
    ['seven-source Windows skipped', (w) => { w.jobs['private-cli'].steps[index + 2].if = "matrix.os == 'ubuntu-latest'"; }],
    ['live source binding omitted', (w) => {
      w.jobs['private-cli'].steps[index + 3].run = w.jobs['private-cli'].steps[index + 3].run.replace(' --live', '');
    }],
    ['seven-source admission omitted', (w) => { w.jobs['private-cli'].steps.splice(index + 3, 1); }],
    ['archive controls omitted', (w) => { w.jobs['private-cli'].steps.splice(index + 1, 1); }],
    ['archive packaging omitted', (w) => { w.jobs['private-cli'].steps.splice(index + 4, 1); }],
    ['empty directory lost in directory upload', (w) => {
      w.jobs['private-cli'].steps[index + 5].with.path = expression('runner.temp') + '/private-production-manifests';
    }],
    ['existing nine-source proof replaced', (w) => { w.jobs['private-cli'].steps.splice(index - 1, 1); }],
    ['seven-source failure masked', (w) => { w.jobs['private-cli'].steps[index + 2].run += ' || true'; }],
  ]);
});

test('physical allocation proof is Linux-only with independent execution, admission and archive', () => {
  const steps = workflow.jobs['private-cli'].steps;
  const index = steps.findIndex((s) => s.name === 'Verify independent physical allocation controls');
  assert(index > 0);
  assert.match(steps[index + 1].run, /physical\/run\.py/u);
  assert.match(steps[index + 2].run, /physical\/verify\.py/u);
  assert.match(steps[index + 2].run, /--platform linux/u);
  assert.equal(steps[index + 3].name, 'Preserve exact real physical allocation proof');
  for (let i = index; i < index + 3; i++) assert.equal(steps[i].if, "matrix.os == 'ubuntu-latest'");
  rejectMutations([
    ['Windows physical execution claimed', (w) => { delete w.jobs['private-cli'].steps[index + 1].if; }],
    ['physical selector bypassed', (w) => { w.jobs['private-cli'].steps[index + 1].run += ' --list'; }],
    ['physical admission omitted', (w) => { w.jobs['private-cli'].steps.splice(index + 2, 1); }],
  ]);
});
test('complete source corpus freezes 33 BUILD and 36 diagnostic pairs on both hosts', () => {
  const steps = workflow.jobs['private-cli'].steps;
  const index = steps.findIndex((s) => s.name === 'Verify independent complete source corpus controls');
  assert.ok(index > 0);
  const base = 'tests/native-provider-qualification/production-corpus/';
  assert.equal(steps[index].run, 'python -B ' + base + 'verify_test.py');
  assert.equal(steps[index + 1].name, 'Verify independent complete corpus archive controls');
  assert.equal(steps[index + 1].run, 'python -B ' + base + 'archive_test.py');
  assert.equal(steps[index + 2].name, 'Run 33 BUILD and 36 rejection corpus pairs');
  assert.match(steps[index + 2].run, /production-corpus\/run\.py/u);
  assert.match(steps[index + 2].run, /--cli-proof "\$env:RUNNER_TEMP\/private-cli-proof"/u);
  assert.equal(steps[index + 3].name, 'Require independent complete source corpus admission');
  assert.match(steps[index + 3].run, /production-corpus\/verify\.py/u);
  assert.match(steps[index + 3].run, /--live/u);
  assert.equal(steps[index + 4].name, 'Package complete source corpus archive');
  assert.match(steps[index + 4].run, /production-corpus\/archive\.py pack/u);
  assert.equal(steps[index + 5].name, 'Preserve complete source corpus pairs');
  assert.equal(steps[index + 5].with.path, expression('runner.temp') + '/private-production-corpus.zip');
  assert.equal(steps[index + 5].with.name, 'private-production-corpus-'
    + expression('matrix.os') + '-' + expression('env.CLI_SOURCE_SHA') + '-' + expression('github.run_id')
    + '-' + expression('github.run_attempt'));
  for (const step of steps.slice(index, index + 5)) assert.equal(step.if, undefined);
  rejectMutations([
    ['Windows corpus skipped', (w) => { w.jobs['private-cli'].steps[index + 2].if = "matrix.os == 'ubuntu-latest'"; }],
    ['controls omitted', (w) => { w.jobs['private-cli'].steps.splice(index, 1); }],
    ['admission omitted', (w) => { w.jobs['private-cli'].steps.splice(index + 3, 1); }],
    ['archive failure masked', (w) => { w.jobs['private-cli'].steps[index + 4].run += ' || true'; }],
    ['unreviewed loose archive', (w) => {
      w.jobs['private-cli'].steps[index + 5].with.path = expression('runner.temp') + '/private-production-corpus';
    }],
  ]);
});

test('three remaining M2 roots retain both-host controls, BUILD, admission and original ZIP', () => {
  const steps = workflow.jobs['private-cli'].steps;
  const index = steps.findIndex((s) => s.name === 'Verify independent three-root M2 manifest controls');
  assert.ok(index > 0);
  const base = 'tests/native-provider-qualification/production-m2-roots/';
  assert.equal(steps[index].run, 'python -B ' + base + 'verify_test.py');
  assert.equal(steps[index + 1].run, 'python -B ' + base + 'archive_test.py');
  assert.match(steps[index + 2].run, /production-m2-roots\/run\.py/u);
  assert.match(steps[index + 2].run, /--cli-proof "\$env:RUNNER_TEMP\/private-cli-proof"/u);
  assert.match(steps[index + 3].run, /--platform "\$platform".*--live/u);
  assert.match(steps[index + 4].run, /production-m2-roots\/archive\.py pack/u);
  assert.equal(steps[index + 5].if, 'always()');
  assert.equal(steps[index + 5].with.path, expression('runner.temp') + '/private-production-m2-roots.zip');
  assert.equal(steps[index + 5].with.name, 'private-production-m2-roots-'
    + expression('matrix.os') + '-' + expression('env.CLI_SOURCE_SHA') + '-'
    + expression('github.run_id') + '-' + expression('github.run_attempt'));
  for (const step of steps.slice(index, index + 5)) assert.equal(step.if, undefined);
  rejectMutations([
    ['Windows M2 roots skipped', (w) => { w.jobs['private-cli'].steps[index + 2].if = "matrix.os == 'ubuntu-latest'"; }],
    ['M2 root controls omitted', (w) => { w.jobs['private-cli'].steps.splice(index, 1); }],
    ['M2 root admission omitted', (w) => { w.jobs['private-cli'].steps.splice(index + 3, 1); }],
    ['M2 archive failure masked', (w) => { w.jobs['private-cli'].steps[index + 4].run += ' || true'; }],
    ['M2 original directory evidence lost', (w) => {
      w.jobs['private-cli'].steps[index + 5].with.path = expression('runner.temp') + '/private-production-m2-roots';
    }],
  ]);
});
