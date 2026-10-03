import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { readFileSync } from 'node:fs';
import { test } from 'node:test';
import { parseDocument } from 'yaml';
import { withoutBootstrapTiming } from './npm-timing-workflow-cases.mjs';
import { withoutNativeSourceResources } from './native-source-resource-runner.test.mjs';
import { withoutStabilityTests } from './stability-gates-test-selection.test.mjs';

const document = parseDocument(readFileSync(new URL('../.github/workflows/ci.yml', import.meta.url), 'utf8'));
assert.deepEqual(document.errors, []);
const budgetWorkflow = withoutBootstrapTiming(withoutPortableSetup(withoutNativeSourceResources(withoutStabilityTests(document.toJS()))));
const packageDocument = JSON.parse(readFileSync(new URL('../package.json', import.meta.url), 'utf8'));
const bootstrapJobs = ['fast-contracts', 'owned-data-quick', 'preflight', 'rust', 'adapter-platform', 'm2-platform', 'm3-platform'];
const nodeStep = {
  uses: 'actions/setup-node@820762786026740c76f36085b0efc47a31fe5020',
  with: { 'node-version': '22.22.1' },
};
const pnpmStep = {
  uses: 'pnpm/action-setup@0977fd99725f1db4007ccb2928dbb4e90d06cc86',
  with: { version: '11.18.0' },
};
const workflow = withoutPreflightBudgets(budgetWorkflow);

function withoutPortableSetup(candidate) {
  const original = structuredClone(candidate);
  assert.deepEqual(original.jobs['portable-setup'], {
    name: 'portable setup', needs: ['route-contracts', 'm0'],
    if: "needs.route-contracts.outputs.distribution_release == 'true'",
    uses: './.github/workflows/portable-setup.yml',
  });
  delete original.jobs['portable-setup'];
  return original;
}

function withoutPreflightBudgets(candidate) {
  const original = structuredClone(candidate);
  const job = original.jobs.preflight;
  assert.deepEqual(Object.keys(job).sort(), ['name', 'runs-on', 'steps', 'timeout-minutes']);
  assert.equal(job['timeout-minutes'], 40);
  const bootstrap = job.steps.filter(step => step.uses?.startsWith('pnpm/action-setup@'));
  const execution = job.steps.filter(step => step.run === 'node scripts/run-preflight.mjs --compiler-contracts');
  assert.equal(bootstrap.length, 1);
  assert.equal(execution.length, 1);
  assert.deepEqual(bootstrap[0], { ...pnpmStep, 'timeout-minutes': 10 });
  assert.deepEqual(execution[0], { run: 'node scripts/run-preflight.mjs --compiler-contracts', 'timeout-minutes': 25 });
  assert.equal(job.steps.at(-1), execution[0]);
  assert(job.steps.indexOf(bootstrap[0]) < job.steps.indexOf(execution[0]));
  assert.equal(job['timeout-minutes'] - bootstrap[0]['timeout-minutes'] - execution[0]['timeout-minutes'], 5);
  delete bootstrap[0]['timeout-minutes'];
  delete execution[0]['timeout-minutes'];
  for (const other of Object.values(original.jobs)) {
    for (const step of other.steps) assert.equal(step['timeout-minutes'], undefined);
  }
  return original;
}

test('preflight separates bounded bootstrap and execution budgets without changing commands', () => {
  assert.doesNotThrow(() => withoutPreflightBudgets(budgetWorkflow));
  assert.equal(workflow.jobs.preflight['timeout-minutes'], 40);
});

test('preflight budget removal, relocation, bypass and ambiguous targets fail closed', () => {
  const targets = [
    w => w.jobs.preflight,
    w => w.jobs.preflight.steps.find(step => step.uses === pnpmStep.uses),
    w => w.jobs.preflight.steps.find(step => step.run === 'node scripts/run-preflight.mjs --compiler-contracts'),
  ];
  for (const target of targets) {
    for (const value of [undefined, null, true, 0, -1, 1.5, '25', '${{ 25 }}', 9, 11, 15, 20, 24, 26, 30, 35, 39, 41]) {
      const changed = structuredClone(budgetWorkflow);
      if (value === undefined) delete target(changed)['timeout-minutes'];
      else target(changed)['timeout-minutes'] = value;
      assert.throws(() => withoutPreflightBudgets(changed));
    }
    for (const property of ['if', 'continue-on-error']) {
      const changed = structuredClone(budgetWorkflow);
      target(changed)[property] = property === 'if' ? 'always()' : true;
      assert.throws(() => withoutPreflightBudgets(changed));
    }
  }
  for (const mutate of [
    steps => { steps.push(structuredClone(steps.find(step => step.uses === pnpmStep.uses))); },
    steps => { steps.push(structuredClone(steps.find(step => step.run === 'node scripts/run-preflight.mjs --compiler-contracts'))); },
    steps => { steps.splice(steps.findIndex(step => step.uses === pnpmStep.uses), 1); },
    steps => { steps.pop(); },
    steps => { const index = steps.findIndex(step => step.uses === pnpmStep.uses); [steps[index], steps[steps.length - 1]] = [steps.at(-1), steps[index]]; },
    steps => { steps.at(-1).run += ' || true'; },
    steps => { steps.at(-1).run = 'pnpm m3:owned:quick'; },
    steps => { steps[0]['timeout-minutes'] = steps.at(-1)['timeout-minutes']; delete steps.at(-1)['timeout-minutes']; },
  ]) {
    const changed = structuredClone(budgetWorkflow);
    mutate(changed.jobs.preflight.steps);
    assert.throws(() => withoutPreflightBudgets(changed));
  }
  for (const id of Object.keys(budgetWorkflow.jobs)) {
    const changed = structuredClone(budgetWorkflow);
    changed.jobs[id].steps[0]['timeout-minutes'] = 10;
    assert.throws(() => withoutPreflightBudgets(changed));
  }
});

function bootstrapOrder(candidate, metadata) {
  assert.equal(metadata.packageManager, 'pnpm@11.18.0');
  assert.equal(metadata.devEngines?.packageManager, undefined,
    'new package-manager metadata requires explicit automatic-cache review');
  const actualJobs = Object.entries(candidate.jobs).filter(([, job]) => job.steps.some(step =>
    /^pnpm\/action-setup@/.test(step.uses ?? '')) && !job.needs).map(([id]) => id);
  assert.deepEqual(actualJobs, bootstrapJobs);
  for (const id of bootstrapJobs) {
    const steps = candidate.jobs[id].steps;
    const pnpm = steps.flatMap((step, index) => /^pnpm\/action-setup@/.test(step.uses ?? '') ? [index] : []);
    const nodes = steps.flatMap((step, index) => /^actions\/setup-node@/.test(step.uses ?? '') ? [index] : []);
    assert.equal(pnpm.length, 1, id);
    const index = pnpm[0];
    assert.deepEqual(steps[index], pnpmStep, id);
    assert.deepEqual(steps[index - 1], nodeStep, id);
    if (id === 'owned-data-quick') {
      assert.deepEqual(nodes, [index - 1], id);
    } else {
      assert.deepEqual(nodes, [index - 1, index + 1], id);
      assert.deepEqual(steps[index + 1], {
        ...nodeStep, with: { ...nodeStep.with, cache: 'pnpm' },
      }, id);
      assert.deepEqual(steps[index + 2], { run: 'pnpm install --frozen-lockfile' }, id);
    }
  }
}

test('CI pins bootstrap Node before pnpm and restores stores only after pnpm exists', () => {
  bootstrapOrder(workflow, packageDocument);
});

test('bootstrap toolchain and cache-order mutations fail closed in every setup job', () => {
  for (const id of bootstrapJobs) {
    const mutations = [
      (steps, index) => { steps.splice(index - 1, 1); },
      (steps, index) => { [steps[index - 1], steps[index]] = [steps[index], steps[index - 1]]; },
      (steps, index) => { steps[index - 1].uses = 'actions/setup-node@main'; },
      (steps, index) => { steps[index - 1].with['node-version'] = '22'; },
      (steps, index) => { steps[index - 1].with.cache = 'pnpm'; },
      (steps, index) => { steps[index - 1].if = 'false'; },
      (steps, index) => { steps[index - 1]['continue-on-error'] = true; },
      (steps, index) => { steps[index].uses = 'pnpm/action-setup@main'; },
      (steps, index) => { steps[index].with.version = 'latest'; },
      (steps, index) => { steps[index].with.run_install = true; },
      (steps, index) => { steps[index].if = 'false'; },
      (steps, index) => { steps[index]['continue-on-error'] = true; },
      steps => { steps.push(structuredClone(nodeStep)); },
      steps => { steps.push(structuredClone(pnpmStep)); },
    ];
    if (id !== 'owned-data-quick') mutations.push(
      (steps, index) => { steps.splice(index + 1, 1); },
      (steps, index) => { [steps[index], steps[index + 1]] = [steps[index + 1], steps[index]]; },
      (steps, index) => { delete steps[index + 1].with.cache; },
      (steps, index) => { steps[index + 1].with.cache = 'npm'; },
      (steps, index) => { steps[index + 1].with['node-version'] = 'latest'; },
      (steps, index) => { steps[index + 1].uses = 'actions/setup-node@main'; },
      (steps, index) => { steps[index + 1].if = 'false'; },
      (steps, index) => { steps[index + 1]['continue-on-error'] = true; },
      (steps, index) => { steps[index + 2].run = 'pnpm install'; },
    );
    for (const mutate of mutations) {
      const changed = structuredClone(workflow);
      const steps = changed.jobs[id].steps;
      mutate(steps, steps.findIndex(step => step.uses === pnpmStep.uses));
      assert.throws(() => bootstrapOrder(changed, packageDocument), `${id}: mutation must fail`);
    }
  }
  for (const metadata of [
    { ...packageDocument, packageManager: 'npm@11' },
    { ...packageDocument, packageManager: undefined },
    { ...packageDocument, devEngines: { packageManager: { name: 'npm' } } },
    { ...packageDocument, devEngines: { packageManager: [{ name: 'npm' }] } },
  ]) assert.throws(() => bootstrapOrder(workflow, metadata));
});
const aggregateNeeds = {
  adapter: ['fast-contracts', 'preflight', 'adapter-platform'],
  m0: ['fast-contracts', 'owned-data-quick', 'preflight', 'rust', 'adapter', 'route-contracts', 'provider-conformance-v4'],
  m2: ['m0', 'm2-platform'],
};
const matrixJobs = ['owned-data-quick', 'rust', 'adapter-platform', 'm2-platform'];
const outcomes = ['success', 'failure', 'cancelled', 'skipped', '', 'unknown', undefined];

function keys(value, expected) {
  assert.deepEqual(Object.keys(value).sort(), [...expected].sort());
}

// Interpret only the workflow's exact conjunction grammar, never execute a shell.
function aggregatePredicate(job, expectedNeeds) {
  keys(job, ['name', 'if', 'needs', 'runs-on', 'steps']);
  assert.equal(job.if, 'always()');
  assert.equal(job['runs-on'], 'ubuntu-latest');
  assert.deepEqual(job.needs, expectedNeeds);
  const providerIndex = job.needs.indexOf('provider-conformance-v4');
  if (providerIndex !== -1) {
    assert.equal(job.steps.length, 2);
    assert.deepEqual(job.steps[0], {
      uses: 'actions/checkout@3d3c42e5aac5ba805825da76410c181273ba90b1',
      with: { 'fetch-depth': 0 },
    });
  } else {
    assert.equal(job.steps.length, 1);
  }
  const step = job.steps.at(-1);
  keys(step, ['name', 'env', 'run']);
  assert.equal(typeof step.run, 'string');
  const clauses = step.run.split(' && ');
  if (providerIndex !== -1) {
    assert.equal(clauses.shift(), 'node scripts/verify-provider-v4-ci-result.mjs');
    assert.equal(
      step.env.PROVIDER_V4_REQUIRED,
      '${{ needs.route-contracts.outputs.provider_v4 }}',
    );
    assert.equal(
      step.env.PROVIDER_V4_RESULT,
      '${{ needs.provider-conformance-v4.result }}',
    );
  }
  const variables = clauses.map(clause => {
    const match = /^test "\$([A-Z][A-Z0-9_]*)" = success$/.exec(clause);
    assert(match, `unsupported aggregate predicate: ${clause}`);
    return match[1];
  });
  assert.equal(new Set(variables).size, variables.length, 'duplicate predicate variable');
  keys(step.env, providerIndex === -1
    ? variables
    : [...variables, 'PROVIDER_V4_REQUIRED', 'PROVIDER_V4_RESULT']);
  const bindings = new Map(variables.map(variable => {
    const match = /^\$\{\{ needs\.([a-z][a-z0-9-]*)\.result \}\}$/.exec(step.env[variable]);
    assert(match, `unsupported result binding: ${variable}`);
    assert(job.needs.includes(match[1]), `undeclared dependency: ${match[1]}`);
    return [variable, match[1]];
  }));
  const shellNeeds = job.needs.filter(need => need !== 'provider-conformance-v4');
  assert.deepEqual([...bindings.values()].sort(), [...shellNeeds].sort(), 'every shell dependency checked once');
  return results => variables.every(variable => results[bindings.get(variable)] === 'success')
    && (providerIndex === -1 || results['provider-conformance-v4'] === 'success');
}

function* combinations(names, prefix = {}) {
  if (names.length === 0) {
    yield prefix;
    return;
  }
  for (const outcome of outcomes) {
    yield* combinations(names.slice(1), { ...prefix, [names[0]]: outcome });
  }
}

function evaluateGraph(jobs, leaves) {
  const predicates = Object.fromEntries(Object.entries(aggregateNeeds)
    .map(([id, needs]) => [id, aggregatePredicate(jobs[id], needs)]));
  const results = { ...leaves };
  const active = new Set();
  function visit(id) {
    if (Object.hasOwn(results, id)) return results[id];
    assert(predicates[id], `missing authority: ${id}`);
    assert(!active.has(id), `cyclic aggregate dependency: ${id}`);
    active.add(id);
    const dependencies = Object.fromEntries(jobs[id].needs.map(need => [need, visit(need)]));
    active.delete(id);
    results[id] = predicates[id](dependencies) ? 'success' : 'failure';
    return results[id];
  }
  for (const id of Object.keys(predicates)) visit(id);
  return results;
}

const WINDOWS_RUST_TIMEOUT = "${{ matrix.os == 'windows-latest' && 60 || 40 }}";
const WINDOWS_PROVIDER_TIMEOUT = "${{ matrix.os == 'windows-latest' && 40 || 30 }}";

function withoutWindowsProviderBudget(candidate) {
  const original = structuredClone(candidate);
  const job = original.jobs['provider-conformance-v4'];
  assert.equal(job['timeout-minutes'], WINDOWS_PROVIDER_TIMEOUT);
  assert.deepEqual(job.strategy.matrix, { os: ['ubuntu-latest', 'windows-latest'] });
  job['timeout-minutes'] = 30;
  return original;
}

test('Windows provider budget preserves Linux and all other workflow authority', () => {
  const original = withoutWindowsProviderBudget(workflow);
  assert.equal(original.jobs['provider-conformance-v4']['timeout-minutes'], 30);
  original.jobs['provider-conformance-v4']['timeout-minutes'] = WINDOWS_PROVIDER_TIMEOUT;
  assert.deepEqual(original, workflow);
});

test('provider budget values, operating systems and operators reject before normalization', () => {
  for (const value of [
    undefined, null, true, 0, 30, 40, 60,
    "${{ matrix.os == 'windows-latest' && 39 || 30 }}",
    "${{ matrix.os == 'windows-latest' && 41 || 30 }}",
    "${{ matrix.os == 'windows-latest' && 40 || 29 }}",
    "${{ matrix.os == 'windows-latest' && 40 || 31 }}",
    "${{ matrix.os == 'windows-latest' && 30 || 40 }}",
    "${{ matrix.os == 'ubuntu-latest' && 40 || 30 }}",
    "${{ matrix.arch == 'windows-latest' && 40 || 30 }}",
    "${{ matrix.os != 'windows-latest' && 40 || 30 }}",
    "${{ matrix.os == 'windows-latest' || 40 && 30 }}",
    "${{ matrix.os == 'windows-latest' && 40 && 30 }}",
  ]) {
    const changed = structuredClone(workflow);
    if (value === undefined) delete changed.jobs['provider-conformance-v4']['timeout-minutes'];
    else changed.jobs['provider-conformance-v4']['timeout-minutes'] = value;
    assert.throws(() => withoutWindowsProviderBudget(changed));
  }
});

function withoutWindowsRustBudget(candidate) {
  const original = structuredClone(candidate);
  assert.equal(original.jobs.rust['timeout-minutes'], WINDOWS_RUST_TIMEOUT);
  original.jobs.rust['timeout-minutes'] = 40;
  return original;
}

test('Windows Rust timeout retains finite platform budgets and the unchanged workflow', () => {
  const original = withoutWindowsRustBudget(workflow);
  assert.equal(original.jobs.rust['timeout-minutes'], 40);
  original.jobs.rust['timeout-minutes'] = WINDOWS_RUST_TIMEOUT;
  assert.deepEqual(original, workflow);
});

test('Rust timeout value, operating-system and operator mutations reject before normalization', () => {
  for (const value of [
    undefined, null, true, 0, 40, 60, 75,
    "${{ matrix.os == 'windows-latest' && 59 || 40 }}",
    "${{ matrix.os == 'windows-latest' && 61 || 40 }}",
    "${{ matrix.os == 'windows-latest' && 75 || 40 }}",
    "${{ matrix.os == 'windows-latest' && 60 || 39 }}",
    "${{ matrix.os == 'windows-latest' && 60 || 41 }}",
    "${{ matrix.os == 'windows-latest' && 40 || 60 }}",
    "${{ matrix.os == 'ubuntu-latest' && 60 || 40 }}",
    "${{ matrix.arch == 'windows-latest' && 60 || 40 }}",
    "${{ matrix.os != 'windows-latest' && 60 || 40 }}",
    "${{ matrix.os == 'windows-latest' || 60 && 40 }}",
    "${{ matrix.os == 'windows-latest' && 60 && 40 }}",
  ]) {
    const changed = structuredClone(workflow);
    if (value === undefined) delete changed.jobs.rust['timeout-minutes'];
    else changed.jobs.rust['timeout-minutes'] = value;
    assert.throws(() => withoutWindowsRustBudget(changed));
  }
});

test('CI starts independent authorities together with bounded preflight headroom', () => {
  assert.equal(workflow.jobs['fast-contracts']['timeout-minutes'], 10);
  assert.equal(workflow.jobs.preflight['timeout-minutes'], 40);
  assert.equal(workflow.jobs.rust['timeout-minutes'], WINDOWS_RUST_TIMEOUT);
  assert.equal(workflow.jobs.preflight.if, undefined);
  assert.equal(workflow.jobs.preflight.needs, undefined);
  assert.equal(workflow.jobs['fast-contracts'].needs, undefined);
  assert.equal(workflow.jobs['fast-contracts'].if, undefined);
  assert.equal(workflow.jobs['fast-contracts']['continue-on-error'], undefined);
  for (const id of matrixJobs) {
    const job = workflow.jobs[id];
    assert.equal(job.needs, undefined, `${id}: must start independently`);
    assert.equal(job.if, undefined, `${id}: authority must not be conditionally skipped`);
    assert.equal(job['continue-on-error'], undefined);
    assert.equal(job.strategy['fail-fast'], false);
    assert.deepEqual(job.strategy.matrix, { os: ['ubuntu-latest', 'windows-latest'] });
    assert.equal(job['runs-on'], '${{ matrix.os }}');
  }
});

test('actual aggregate predicates reject every Cartesian non-success result', () => {
  let checked = 0;
  for (const [id, needs] of Object.entries(aggregateNeeds)) {
    const predicate = aggregatePredicate(workflow.jobs[id], needs);
    for (const results of combinations(needs)) {
      assert.equal(predicate(results), needs.every(need => results[need] === 'success'),
        `${id}: ${JSON.stringify(results)}`);
      checked++;
    }
  }
  assert.equal(checked, 823935);
});

test('each OS authority and preflight must succeed through the actual aggregate graph', () => {
  const allSuccess = Object.fromEntries(['fast-contracts', 'preflight', 'route-contracts', 'provider-conformance-v4', ...matrixJobs]
    .map(id => [id, 'success']));
  assert.equal(evaluateGraph(workflow.jobs, allSuccess).m2, 'success');
  for (const preflight of outcomes) {
    const results = evaluateGraph(workflow.jobs, { ...allSuccess, preflight });
    for (const id of Object.keys(aggregateNeeds)) assert.equal(results[id], preflight === 'success' ? 'success' : 'failure');
  }
  for (const id of matrixJobs) {
    const operatingSystems = workflow.jobs[id].strategy.matrix.os;
    for (const legs of combinations(operatingSystems)) {
      // Any non-success matrix leg must be represented by a non-success job result.
      // The aggregate interpreter checks the actual workflow result binding, not a shell stub.
      const failedLeg = operatingSystems.find(os => legs[os] !== 'success');
      const result = failedLeg === undefined ? 'success' : legs[failedLeg];
      const evaluated = evaluateGraph(workflow.jobs, { ...allSuccess, [id]: result });
      assert.equal(evaluated.m2, failedLeg === undefined ? 'success' : 'failure');
      if (id === 'adapter-platform') assert.equal(evaluated.adapter, evaluated.m2);
      if (id !== 'm2-platform') assert.equal(evaluated.m0, evaluated.m2);
    }
  }
});

test('aggregate grammar and dependency mutations fail closed', () => {
  for (const [id, needs] of Object.entries(aggregateNeeds)) {
    for (const mutate of [
      job => { job.if = 'success()'; },
      job => { delete job.if; },
      job => { job['continue-on-error'] = true; },
      job => { job.needs = job.needs.slice(1); },
      job => { job.steps.at(-1).if = 'success()'; },
      job => { job.steps.at(-1)['continue-on-error'] = true; },
      job => { job.steps.at(-1).run += ' || true'; },
      job => { job.steps.at(-1).run += '; exit 0'; },
      job => { job.steps.at(-1).run = job.steps.at(-1).run.replace('= success', '!= failure'); },
      job => { job.steps.at(-1).run = job.steps.at(-1).run.split(' && ').slice(1).join(' && '); },
      job => { job.steps.at(-1).run += ` && ${job.steps.at(-1).run.split(' && ')[0]}`; },
      job => { job.steps.at(-1).run = 'true'; },
      job => { job.steps.at(-1).env[Object.keys(job.steps.at(-1).env)[0]] = 'success'; },
      job => { job.steps.at(-1).env[Object.keys(job.steps.at(-1).env)[0]] = '${{ needs.missing.result }}'; },
      job => { job.steps.at(-1).env[Object.keys(job.steps.at(-1).env)[0]] = Object.values(job.steps.at(-1).env)[1]; },
      job => { job.steps.at(-1).env[Object.keys(job.steps.at(-1).env)[0]] += ' || success'; },
      job => { job.steps.push({ run: 'true' }); },
    ]) {
      const changed = structuredClone(workflow.jobs[id]);
      mutate(changed);
      assert.throws(() => aggregatePredicate(changed, needs), `${id}: mutation must fail`);
    }
  }
});

test('M0 aggregate checks out the pinned verifier before execution', () => {
  for (const mutate of [
    job => { job.steps.shift(); },
    job => { job.steps.reverse(); },
    job => { job.steps[0].uses = 'actions/checkout@main'; },
    job => { job.steps[0].with['fetch-depth'] = 1; },
  ]) {
    const changed = structuredClone(workflow.jobs.m0);
    mutate(changed);
    assert.throws(() => aggregatePredicate(changed, aggregateNeeds.m0));
  }
});

test('routing preserves all other pinned workflow authority', () => {
  bootstrapOrder(workflow, packageDocument);
  const original = withoutWindowsProviderBudget(withoutWindowsRustBudget(workflow));
  const editorIndex = original.jobs.rust.steps.findIndex(step => step.name === "Verify and package editor client");
  assert(editorIndex >= 0);
  assert.deepEqual(original.jobs.rust.steps[editorIndex], {
    name: "Verify and package editor client", run: "pnpm editor:check\npnpm editor:package\n",
  });
  assert.equal(original.jobs.rust.steps[editorIndex + 1].run, "node scripts/run-m0-conformance.mjs");
  original.jobs.rust.steps.splice(editorIndex, 1);
  assert.deepEqual(original.env, {
    ZRYNA_STRUCTURE_BASE: "${{ github.event_name == 'pull_request' && github.event.pull_request.base.sha || inputs.structure_base }}",
  });
  delete original.env;
  for (const job of Object.values(original.jobs)) {
    for (const step of job.steps) {
      if (step.uses?.startsWith('actions/checkout@')) {
        assert.deepEqual(step.with, { 'fetch-depth': 0 });
        delete step.with;
      }
    }
  }
  for (const id of bootstrapJobs) {
    const steps = original.jobs[id].steps;
    const index = steps.findIndex(step => step.uses === pnpmStep.uses);
    if (id === 'owned-data-quick') {
      [steps[index - 1], steps[index]] = [steps[index], steps[index - 1]];
    } else {
      steps.splice(index - 1, 1);
    }
  }
  // M3 additions are independently frozen by m3-conformance.test.mjs.
  delete original.jobs['m3-platform'];
  delete original.jobs.m3;
  // The optional distribution lane is independently frozen by workflow-routing.test.mjs.
  delete original.jobs['distribution-release-contract'];
  // Published clean-host evidence is independently frozen by release-published-workflow.test.mjs.
  delete original.jobs['published-release-clean-host'];
  delete original.jobs['route-contracts'].outputs.distribution_release;
  original.jobs.preflight['timeout-minutes'] = 10;
  for (const id of ['rust', 'adapter-platform', 'm2-platform']) {
    original.jobs[id].needs = 'preflight';
  }
  original.jobs.adapter.needs = 'adapter-platform';
  delete original.jobs.adapter.steps[0].env.PREFLIGHT_RESULT;
  original.jobs.adapter.steps[0].run = 'test "$PLATFORM_RESULT" = success';
  function canonical(value) {
    if (Array.isArray(value)) return value.map(canonical);
    if (value !== null && typeof value === 'object') return Object.fromEntries(
      Object.keys(value).sort().map(key => [key, canonical(value[key])]),
    );
    return value;
  }
  const digest = createHash('sha256').update(JSON.stringify(canonical(original))).digest('hex');
  assert.equal(digest, '038347262f04accafbdfe4db293b232a90cbd7d3ca4044e2114bc68923a76215');
});
