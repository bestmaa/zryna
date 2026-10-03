import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import { test } from 'node:test';
import { parseDocument } from 'yaml';
import './ci-gate-cases.mjs';
import './npm-timing.test.mjs';
import './workflow-routing.test.mjs';
import './stability-gates-test-selection.test.mjs';

import {
  PREFLIGHT_COMMANDS,
  preflightCommandDigest,
  runPreflight,
  selectPreflightCommands,
  validatePreflightCommands,
} from '../scripts/run-preflight.mjs';

function workflowJob(workflow, jobId) {
  const lines = workflow.split(/\r?\n/);
  const start = lines.findIndex((line) => line === `  ${jobId}:`);
  assert.notEqual(start, -1, `missing ${jobId} job`);
  const next = lines.findIndex((line, index) => index > start && /^  [a-zA-Z0-9_-]+:$/.test(line));
  return lines.slice(start, next === -1 ? undefined : next).join('\n');
}

test('preflight has one frozen portable command order', () => {
  assert.deepEqual(
    PREFLIGHT_COMMANDS.map(({ id, executable }) => [id, executable]),
    [
      ['repository-structure', 'node'],
      ['portable-contract-tests', 'node'],
      ['rust-format', 'cargo'],
      ['m2-semantic-driver-tests', 'cargo'],
      ['m3-layout-tests', 'cargo'],
      ['m3-ownership-runtime-abi-tests', 'cargo'],
      ['m3-data-ir-tests', 'cargo'],
      ['m3-data-ir-doc-tests', 'cargo'],
      ['m3-aggregate-semantics-tests', 'cargo'],
      ['m3-aggregate-semantics-doc-tests', 'cargo'],
      ['rust-workspace-check', 'cargo'],
      ['frontend-syntax-tests', 'cargo'],
    ],
  );
  assert.ok(PREFLIGHT_COMMANDS.every(({ args }) => Object.isFrozen(args)));
  assert.ok(Object.isFrozen(PREFLIGHT_COMMANDS));
  assert.equal(preflightCommandDigest(), 'b27402c8d6052eaf665be0d01855ee9febabd2167b459881c320833bce662c3b');
  assert.doesNotThrow(() => validatePreflightCommands());

  for (const mutate of [
    (commands) => commands[0].args.pop(),
    (commands) => { commands[1].args[0] = 'check'; },
    (commands) => commands[2].args.splice(commands[2].args.indexOf('--locked'), 1),
    (commands) => commands[3].args.pop(),
    (commands) => commands[4].args.pop(),
    (commands) => commands[5].args.pop(),
    (commands) => commands[6].args.pop(),
    (commands) => commands[7].args.pop(),
    (commands) => commands[8].args.pop(),
    (commands) => commands[9].args.pop(),
    (commands) => commands[10].args.pop(),
  ]) {
    const changed = structuredClone(PREFLIGHT_COMMANDS);
    mutate(changed);
    assert.throws(() => validatePreflightCommands(changed), /differ from the frozen command set/);
  }
});

test('preflight stops at the first failure', () => {
  const started = [];
  const commands = [
    { id: 'first', executable: 'node', args: ['first'] },
    { id: 'fails', executable: 'node', args: ['fails'] },
    { id: 'must-not-run', executable: 'node', args: ['must-not-run'] },
  ];
  const spawn = (_executable, args, options) => {
    started.push(args[0]);
    assert.equal(options.shell, false);
    return { status: args[0] === 'fails' ? 7 : 0 };
  };

  assert.throws(() => runPreflight(commands, spawn), /fails failed with exit status 7/);
  assert.deepEqual(started, ['first', 'fails']);
});

test('CI preflight partitions retain the exact frozen command set', () => {
  const fast = selectPreflightCommands(['--fast-contracts']);
  const compiler = selectPreflightCommands(['--compiler-contracts']);
  assert.deepEqual(fast.map(({ id }) => id), ['repository-structure', 'portable-contract-tests']);
  assert.deepEqual([...fast, ...compiler], PREFLIGHT_COMMANDS);
  assert.equal(fast.some((command) => command.executable === 'cargo'), false);
  assert.equal(compiler.every((command) => command.executable === 'cargo'), true);
  for (const args of [['--unknown'], ['--fast-contracts', '--compiler-contracts']]) {
    assert.throws(() => selectPreflightCommands(args), /usage/);
  }
});

test('independent platform jobs start alongside preflight and aggregates require every gate', async () => {
  const workflow = await readFile(new URL('../.github/workflows/ci.yml', import.meta.url), 'utf8');
  const document = parseDocument(workflow);
  assert.deepEqual(document.errors, []);
  const parsed = document.toJS();
  const fastContracts = workflowJob(workflow, 'fast-contracts');
  const ownedDataQuick = workflowJob(workflow, 'owned-data-quick');
  const preflight = workflowJob(workflow, 'preflight');
  const rust = workflowJob(workflow, 'rust');
  const adapterPlatform = workflowJob(workflow, 'adapter-platform');
  const aggregate = workflowJob(workflow, 'm0');

  assert.match(fastContracts, /name: fast contract checks/);
  assert.match(fastContracts, /run: node scripts\/run-preflight\.mjs --fast-contracts/);
  assert.doesNotMatch(fastContracts, /needs:/);

  assert.match(ownedDataQuick, /name: owned data quick \(\$\{\{ matrix\.os \}\}\)/);
  assert.match(ownedDataQuick, /name: Verify M3 owned-data semantics\s+run: pnpm m3:owned:quick/);
  assert.doesNotMatch(ownedDataQuick, /needs:/);
  assert.deepEqual(parsed.jobs['owned-data-quick'].strategy.matrix.os, [
    'ubuntu-latest',
    'windows-latest',
  ]);
  assert.match(preflight, /name: preflight/);
  assert.match(preflight, /run: node scripts\/run-preflight\.mjs --compiler-contracts/);
  assert.match(rust, /name: Fetch locked Rust dependencies\s+run: cargo fetch --locked/);
  assert.match(
    rust,
    /name: Verify M2 semantics\s+run: cargo test --locked -p zryna-semantics --lib control_flow_v1/,
  );
  assert.doesNotMatch(rust, /pnpm m3:owned:quick/);
  assert.deepEqual(parsed.jobs.rust.strategy.matrix.os, ['ubuntu-latest', 'windows-latest']);
  assert.match(rust, /run: cargo test --locked -p zryna-driver --lib/);
  assert.doesNotMatch(rust, /module_closure_tests::/);
  assert.doesNotMatch(rust, /needs:/);
  assert.doesNotMatch(adapterPlatform, /needs:/);
  assert.match(aggregate,
    /needs: \[fast-contracts, owned-data-quick, preflight, rust, adapter, route-contracts, provider-conformance-v4\]/);
  assert.match(aggregate, /FAST_CONTRACTS_RESULT: \$\{\{ needs\.fast-contracts\.result \}\}/);
  assert.match(aggregate, /test "\$FAST_CONTRACTS_RESULT" = success/);
  assert.match(
    aggregate,
    /OWNED_DATA_QUICK_RESULT: \$\{\{ needs\.owned-data-quick\.result \}\}/,
  );
  assert.match(aggregate, /test "\$OWNED_DATA_QUICK_RESULT" = success/);
  assert.match(aggregate, /PREFLIGHT_RESULT: \$\{\{ needs\.preflight\.result \}\}/);
  assert.match(aggregate, /test "\$PREFLIGHT_RESULT" = success/);
  assert.match(aggregate, /test "\$ROUTING_RESULT" = success/);
  assert.match(aggregate, /node scripts\/verify-provider-v4-ci-result\.mjs/);

  assert.equal(parsed.jobs.preflight['timeout-minutes'], 40);
  assert.equal(parsed.jobs.preflight.steps.filter(step => step.uses?.startsWith('pnpm/action-setup@')).length, 1);
  assert.equal(parsed.jobs.preflight.steps.find(step => step.uses?.startsWith('pnpm/action-setup@'))['timeout-minutes'], 10);
  assert.deepEqual(parsed.jobs.preflight.steps.filter(step => step.run === 'pnpm preflight'), [
  ]);
  assert.deepEqual(parsed.jobs.preflight.steps.filter(step => step.run === 'node scripts/run-preflight.mjs --compiler-contracts'), [
    { run: 'node scripts/run-preflight.mjs --compiler-contracts', 'timeout-minutes': 25 },
  ]);
  assert.equal(parsed.jobs.rust.needs, undefined);
  assert.equal(parsed.jobs['adapter-platform'].needs, undefined);
  assert.deepEqual(parsed.jobs.adapter.needs, ['fast-contracts', 'preflight', 'adapter-platform']);
  assert.deepEqual(parsed.jobs.m0.needs,
    ['fast-contracts', 'owned-data-quick', 'preflight', 'rust', 'adapter', 'route-contracts', 'provider-conformance-v4']);
});

test('package exposes the exact documented preflight entrypoint', async () => {
  const packageDocument = JSON.parse(await readFile(new URL('../package.json', import.meta.url), 'utf8'));
  assert.equal(packageDocument.scripts.preflight, 'node scripts/run-preflight.mjs');
  assert.equal(
    packageDocument.scripts['m2:quick'],
    'node scripts/run-m2-quick.mjs',
  );
  assert.equal(
    packageDocument.scripts['m3:data:quick'],
    'cargo test --locked -p zryna-semantics data_ownership_v1 -- --skip authenticated_v4_derived_value_budget_is_exact_and_plus_one_fails_m3201',
  );
  assert.equal(
    packageDocument.scripts['m3:owned:quick'],
    'cargo test --locked -p zryna-ir data_ownership_v1 && cargo test --locked -p zryna-ir --doc data_ownership_v1 && cargo test --locked -p zryna-semantics data_ownership_v1 -- --skip authenticated_v4_derived_value_budget_is_exact_and_plus_one_fails_m3201 && cargo test --locked -p zryna-semantics --doc data_ownership_v1',
  );
  assert.equal(
    packageDocument.scripts['m3:runtime-abi:quick'],
    'cargo test --locked -p zryna-ownership-runtime-abi',
  );
});
