import assert from 'node:assert/strict';
import {
  cpSync, mkdirSync, readFileSync, rmSync, writeFileSync,
} from 'node:fs';
import { tmpdir } from 'node:os';
import { resolve } from 'node:path';
import test from 'node:test';

import { parseDocument } from 'yaml';

import {
  assertDeterministic,
  digest,
  loadAndValidateProviderConformance,
  runProviderConformance,
  validateProviderConformanceRegistry,
  verifySourceBinding,
  workspaceRoot,
} from '../scripts/check-provider-conformance-v4.mjs';

const registryBytes = readFileSync(resolve(workspaceRoot, 'tests/provider-conformance-v4.json'));

function mutateRegistry(change) {
  const registry = JSON.parse(registryBytes);
  change(registry);
  const bytes = Buffer.from(JSON.stringify(registry));
  return () => validateProviderConformanceRegistry(bytes, workspaceRoot, digest(bytes));
}

function temporaryCorpus() {
  const root = resolve(tmpdir(), `zryna-provider-v4-${process.pid}-${Math.random().toString(16).slice(2)}`);
  const parent = resolve(root, 'tests/provider-conformance-v4');
  mkdirSync(parent, { recursive: true });
  cpSync(
    resolve(workspaceRoot, 'tests/provider-conformance-v4/fixtures'),
    resolve(parent, 'fixtures'),
    { recursive: true },
  );
  return root;
}

test('registry freezes the provider-neutral corpus and TypeScript 6 bootstrap authority', () => {
  const registry = loadAndValidateProviderConformance();
  assert.deepEqual(registry.cases.map((entry) => entry.category), [
    'positive', 'malformed', 'unsupported', 'budget', 'recovery', 'ordering',
  ]);
  assert.equal(registry.artifacts.length, 9);
  assert.deepEqual(registry.bootstrapAuthority, {
    provider: 'typescript-6', providerVersion: '6.0.3', executable: 'node',
    arguments: ['adapters/typescript-6/src/worker-v4.mjs'],
  });
  assert.deepEqual(registry.requiredCapabilities, {
    module_resolution: false,
    semantic_diagnostics: false,
    control_flow_v1: true,
    data_ownership_syntax_v1: true,
  });
});

test('registry rejects missing, extra, and tampered fixture bytes', () => {
  const root = temporaryCorpus();
  try {
    const fixture = (name) => resolve(root, 'tests/provider-conformance-v4/fixtures', name);
    rmSync(fixture('positive.zry'));
    assert.throws(
      () => validateProviderConformanceRegistry(registryBytes, root),
      /positive-source|ENOENT/,
    );
    cpSync(resolve(workspaceRoot, 'tests/provider-conformance-v4/fixtures/positive.zry'), fixture('positive.zry'));
    writeFileSync(fixture('unregistered.zry'), 'export function extra(): i32 { return 0; }\n');
    assert.throws(
      () => validateProviderConformanceRegistry(registryBytes, root),
      /unregistered or missing/,
    );
    rmSync(fixture('unregistered.zry'));
    writeFileSync(fixture('positive.zry'), 'tampered\n');
    assert.throws(
      () => validateProviderConformanceRegistry(registryBytes, root),
      /tampered artifact: positive-source/,
    );
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});

test('registry rejects reordered, case-colliding, and structurally changed inventories', () => {
  assert.throws(mutateRegistry((registry) => registry.artifacts.reverse()), /must remain ordered/);
  assert.throws(mutateRegistry((registry) => registry.cases.reverse()), /case order/);
  assert.throws(mutateRegistry((registry) => {
    registry.artifacts[1].path = registry.artifacts[0].path.replace('budget', 'Budget');
  }), /case-colliding artifact path/);
  assert.throws(mutateRegistry((registry) => registry.artifacts.pop()), /unregistered or missing/);
  assert.throws(mutateRegistry((registry) => { registry.unknown = true; }), /registry fields/);
  assert.throws(() => validateProviderConformanceRegistry(Buffer.alloc(65_537)), /bounded/);
});

test('snapshot comparison remains deterministic and bound to exact source identities', () => {
  const snapshot = JSON.parse(readFileSync(
    resolve(workspaceRoot, 'tests/provider-conformance-v4/fixtures/positive.snapshot.json'),
  ));
  const source = readFileSync(
    resolve(workspaceRoot, 'tests/provider-conformance-v4/fixtures/positive.zry'),
  );
  verifySourceBinding(snapshot, [{ path: 'src/main.zry', bytes: source }]);
  assert.throws(
    () => verifySourceBinding(snapshot, [{ path: 'src/other.zry', bytes: source }]),
    /file identities/,
  );
  const changed = Buffer.from(source.toString('utf8').replace('identity', 'other_id'));
  assert.throws(
    () => verifySourceBinding(snapshot, [{ path: 'src/main.zry', bytes: changed }]),
    /source text binding/,
  );
  assertDeterministic('same\n', 'same\n');
  assert.throws(() => assertDeterministic('first\n', 'second\n'), /nondeterministic/);
});

test('the TypeScript 6 bootstrap provider passes every canonical session twice', () => {
  assert.equal(runProviderConformance(), 6);
});

test('package command and routed Linux/Windows workflow keep conformance fail closed', () => {
  const pkg = JSON.parse(readFileSync(resolve(workspaceRoot, 'package.json')));
  assert.equal(
    pkg.scripts['provider:conformance:v4'],
    'node scripts/check-provider-conformance-v4.mjs && node --test tests/provider-conformance-v4.test.mjs tests/native-lexer-provider-runner.test.mjs tests/native-parser-provider-runner.test.mjs && cargo test --locked -p zryna-frontend --test provider_conformance_v4 && node scripts/run-native-lexer-provider-differential.mjs && node scripts/run-native-parser-provider-differential.mjs',
  );
  const workflow = parseDocument(readFileSync(
    resolve(workspaceRoot, '.github/workflows/ci.yml'),
    'utf8',
  ));
  assert.deepEqual(workflow.errors, []);
  const document = workflow.toJS();
  assert.deepEqual(Object.keys(document.on), ['pull_request', 'workflow_dispatch']);
  assert.deepEqual(document.permissions, { contents: 'read' });
  const job = document.jobs['provider-conformance-v4'];
  assert.equal(job.needs, 'route-contracts');
  assert.equal(job.if, "needs.route-contracts.outputs.provider_v4 == 'true'");
  assert.deepEqual(job.strategy, {
    'fail-fast': false, matrix: { os: ['ubuntu-latest', 'windows-latest'] },
  });
  assert.equal(job['runs-on'], '${{ matrix.os }}');
  assert.equal(job['timeout-minutes'], 30);
  assert.equal(job.steps.at(-2).run, 'pnpm provider:conformance:v4');
  assert.equal(job.steps.at(-1).run, 'node scripts/run-native-lexer-resource-tests.mjs');
  assert(!Object.hasOwn(job, 'continue-on-error'));
});
