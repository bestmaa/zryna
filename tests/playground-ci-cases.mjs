import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { test } from 'node:test';
import { parseDocument } from 'yaml';

const contracts = { name: 'Verify restricted playground and M6 contracts',
  run: 'pnpm playground:contract\npnpm m6:contract\n' };
const helpers = { name: 'Verify Linux playground helper units', if: "runner.os == 'Linux'",
  env: { PYTHONDONTWRITEBYTECODE: '1' }, run: 'python3 tests/playground-host-unit.py\n' +
    'python3 tests/playground-cgroup-unit.py\npython3 tests/playground-join-unit.py\n' +
    'python3 tests/playground-helper-policy-unit.py\n' };

// Remove only the exact reviewed addition before authenticating all prior CI authority.
export function withoutPlaygroundContracts(candidate) {
  const original = structuredClone(candidate), steps = original.jobs.rust.steps;
  const index = steps.findIndex(step => step.name === contracts.name);
  assert(index > 0);
  assert.deepEqual(steps[index], contracts);
  assert.deepEqual(steps[index + 1], helpers);
  assert.equal(steps[index + 2].name, 'Verify documentation bundle contract');
  steps.splice(index, 2);
  assert(!/playground:contract|m6:contract|playground-.*-unit\.py/.test(JSON.stringify(original)));
  return original;
}

const parsed = parseDocument(readFileSync(new URL('../.github/workflows/ci.yml', import.meta.url), 'utf8'));
assert.deepEqual(parsed.errors, []);
const workflow = parsed.toJS();
test('playground CI removal, bypass, duplicate and relocation mutations fail closed', () => {
  for (const mutate of [
    (steps, i) => { steps.splice(i, 1); },
    (steps, i) => { steps[i].if = 'false'; },
    (steps, i) => { steps[i]['continue-on-error'] = true; },
    (steps, i) => { steps[i].run += 'true\n'; },
    (steps, i) => { steps[i + 1].if = 'false'; },
    (steps, i) => { steps[i + 1].run = 'true\n'; },
    (steps, i) => { steps.push(structuredClone(steps[i])); },
    (steps, i) => { [steps[i + 1], steps[i + 2]] = [steps[i + 2], steps[i + 1]]; },
  ]) {
    const changed = structuredClone(workflow), steps = changed.jobs.rust.steps;
    mutate(steps, steps.findIndex(step => step.name === contracts.name));
    assert.throws(() => withoutPlaygroundContracts(changed));
  }
});
