import assert from 'node:assert/strict';
import test from 'node:test';

export function registerInstalledCommandContract(workflow) {
  test('installed H1 qualification uses the exact candidate, pinned tools and read-only authority', () => {
    const candidate = workflow('installed-command-h1.yml');
    assert.deepEqual(candidate.permissions, { contents: 'read' });
    const job = candidate.jobs.qualify;
    assert.deepEqual(job.strategy.matrix.os, ['ubuntu-24.04', 'windows-2022']);
    assert.equal(job.env.CARGO_BUILD_JOBS, 2);
    const checkout = job.steps.find(step => step.name === 'Checkout exact candidate');
    assert.equal(checkout.with.ref, '${{ github.event.pull_request.head.sha || github.sha }}');
    assert.equal(checkout.with['persist-credentials'], false);
    const qualification = job.steps.find(step => step.name === 'Build, verify, relocate and invoke the installed candidate');
    assert.equal(qualification.env.CARGO_NET_OFFLINE, true);
    assert.match(qualification.run, /tests\/installed-command-qualification\/run\.mjs/);
    assert(!job.steps.some(step => /publish|release create|sudo|runas/i.test(step.run ?? '')));
  });
}
