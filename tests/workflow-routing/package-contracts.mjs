import assert from 'node:assert/strict';
import test from 'node:test';
import { classifyWorkflowPaths } from '../../scripts/classify-workflow-paths.mjs';

// Preserve command inventory, pinned actions and unconditional cross-host registration.
export function registerPackageContracts(ci) {
  test('consolidation preserves every prior contract command and pinned action', () => {
    const commands = (id) => ci.jobs[id].steps.flatMap((step) => step.run ? [step.run] : []);
    assert.deepEqual(commands('diagnostics-contract'), [
      'pnpm install --frozen-lockfile',
      'pnpm diagnostics:contract',
      'cargo test --locked -p zryna-diagnostics',
      'cargo clippy --locked -p zryna-diagnostics --all-targets -- -D warnings',
      'cargo fmt --all -- --check',
    ]);
    assert.deepEqual(commands('distribution-release-contract'), [
      'pnpm install --frozen-lockfile',
      'pnpm release:contract',
      'pnpm distribution:check',
    ]);
    assert.deepEqual(commands('package-release-contract'), [
      'pnpm install --frozen-lockfile',
      'pnpm package:contract',
      'pnpm build-plan:contract',
      'cargo test --locked -p zryna-package',
    ]);
    assert.deepEqual(commands('provider-conformance-v4'), [
      'pnpm install --frozen-lockfile',
      'pnpm provider:conformance:v4',
      'node scripts/run-native-lexer-resource-tests.mjs',
    ]);
    assert.deepEqual(commands('wit-capability-contract'), [
      'pnpm install --frozen-lockfile',
      'pnpm wit:contract',
    ]);
    for (const id of [
      'diagnostics-contract',
      'distribution-release-contract',
      'package-release-contract',
      'provider-conformance-v4',
      'wit-capability-contract',
    ]) {
      const uses = ci.jobs[id].steps.flatMap((step) => step.uses ? [step.uses] : []);
      assert(uses.includes('actions/checkout@3d3c42e5aac5ba805825da76410c181273ba90b1'));
      assert(uses.includes('actions/setup-node@820762786026740c76f36085b0efc47a31fe5020'));
      assert(uses.includes('pnpm/action-setup@0977fd99725f1db4007ccb2928dbb4e90d06cc86'));
      if (id === 'package-release-contract') {
        assert(uses.includes('dtolnay/rust-toolchain@4360b52568e2003a75bf9bc1d59f33a8e3fc893c'));
      }
      assert(!uses.some((use) => use.endsWith('@main')));
    }
  });

  function requireBuildPlanContract(candidate) {
    const job = candidate.jobs['package-release-contract'];
    assert.deepEqual(job.strategy.matrix.os, ['ubuntu-latest', 'windows-latest']);
    assert.equal(job['continue-on-error'], undefined);
    assert.equal(job.needs, 'route-contracts');
    assert.equal(job.if, "needs.route-contracts.outputs.package_release == 'true'");
    assert.deepEqual(job.steps.flatMap(step => step.run ? [step.run] : []), [
      'pnpm install --frozen-lockfile', 'pnpm package:contract',
      'pnpm build-plan:contract', 'cargo test --locked -p zryna-package',
    ]);
    assert.deepEqual(job.steps.filter(step => step.run?.includes('build-plan:contract')),
      [{ run: 'pnpm build-plan:contract' }]);
  }

  test('build-plan contracts run on both package hosts and cannot be disabled', () => {
    requireBuildPlanContract(ci);
    for (const mutate of [
      job => { job.strategy.matrix.os.pop(); },
      job => { job['continue-on-error'] = true; },
      job => { job.steps = job.steps.filter(step => step.run !== 'pnpm build-plan:contract'); },
      job => { job.steps.push({ run: 'pnpm build-plan:contract' }); },
      job => { job.steps.push({ run: 'echo unrelated-command' }); },
      job => { job.steps.reverse(); },
      job => { const index = job.steps.findIndex(step => step.run === 'pnpm build-plan:contract'); const [step] = job.steps.splice(index, 1); job.steps.splice(index - 1, 0, step); },
      job => { const index = job.steps.findIndex(step => step.run === 'pnpm build-plan:contract'); const [step] = job.steps.splice(index, 1); job.steps.splice(index + 1, 0, step); },
      job => { job.steps = job.steps.filter(step => step.run !== 'pnpm package:contract'); },
      job => { job.if = 'false'; },
      job => { job.needs = []; },
      job => { job.steps.find(step => step.run === 'pnpm build-plan:contract').if = 'false'; },
      job => { job.steps.find(step => step.run === 'pnpm build-plan:contract')['continue-on-error'] = true; },
      job => { job.steps.find(step => step.run === 'pnpm build-plan:contract').run += ' || true'; },
    ]) {
      const changed = structuredClone(ci);
      mutate(changed.jobs['package-release-contract']);
      assert.throws(() => requireBuildPlanContract(changed));
    }
    for (const path of [
      'schemas/zryna-resolved-build-plan-v0.schema.json', 'scripts/build-plan/validate.mjs',
      'tests/resolved-build-plan-v0.test.mjs', 'tests/resolved-build-plan-v0/source-only.json',
    'scripts/build-plan/plan-identity.mjs',
    'tests/resolved-build-plan-v0/d2-cases.mjs',
    'tests/workflow-routing/package-contracts.mjs',
      'tests/native-recipe/fixture.mjs', 'scripts/native-recipe/preflight.mjs',
      'spec/package/RESOLVED_BUILD_PLAN_V0.md', 'spec/package/SOURCE_TRUST_V0.md',
      'spec/abi/NATIVE_C_INTEROP_V0_REVIEW.md',
    ]) assert.equal(classifyWorkflowPaths([path]).package_release, true, path);
  });
}
