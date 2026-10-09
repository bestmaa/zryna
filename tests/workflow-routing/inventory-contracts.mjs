import assert from 'node:assert/strict';
import test from 'node:test';
import { readdirSync } from 'node:fs';
import { resolve } from 'node:path';

export function registerWorkflowInventoryContracts({ workflow, root, ci, documentation }) {
  test('pull-request workflows stay inventoried and every superseded run cancels', () => {
    const names = readdirSync(resolve(root, '.github/workflows')).sort();
    assert.deepEqual(names, [
      'ci.yml', 'documentation.yml', 'installed-command-h1.yml', 'native-provider-activation.yml', 'portable-setup.yml',
      'release-production-candidate.yml',
      'release-qualification.yml', 'release.yml',
    ]);
    for (const name of names) {
      const candidate = workflow(name);
      if (!Object.hasOwn(candidate.on, 'pull_request')) continue;
      assert.equal(candidate.concurrency['cancel-in-progress'], true, name);
      assert.match(candidate.concurrency.group, /pull_request\.number/, name);
    }
    assert.equal(Object.hasOwn(workflow('portable-setup.yml').on, 'pull_request'), false);
    assert.equal(ci.jobs['portable-setup'].uses, './.github/workflows/portable-setup.yml');
    assert.deepEqual(ci.jobs['portable-setup'].needs, ['route-contracts', 'm0']);
  });

  test('main runs only documentation validation and publication with short retention', () => {
    assert.deepEqual(documentation.on, { push: { branches: ['main'] } });
    assert.deepEqual(Object.keys(documentation.jobs), ['docs-publish']);
    assert.equal(documentation.concurrency['cancel-in-progress'], true);
    const publisher = documentation.jobs['docs-publish'];
    assert.equal(publisher.needs, undefined);
    assert(publisher.steps.some((step) => step.run === 'pnpm docs:check'));
    const upload = publisher.steps.at(-1);
    assert.equal(upload.uses,
      'actions/upload-artifact@ea165f8d65b6e75b540449e92b4886f43607fa02');
    assert.equal(upload.with['retention-days'], 7);
  });
}
