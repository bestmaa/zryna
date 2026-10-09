import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { lstatSync, readFileSync, readdirSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const scriptPath = fileURLToPath(import.meta.url);
export const workspaceRoot = resolve(dirname(scriptPath), '..');
const expectedDigest = '1d29f8d5c24cf1215d64f340576934f674f5cfb89a51d4f82a3882fe75cb36b9';
export function digest(value) {
  return createHash('sha256').update(value).digest('hex');
}
export function validateM3Registry(bytes, root = workspaceRoot) {
  assert(Buffer.byteLength(bytes) <= 64 * 1024, 'bounded registry');
  assert.equal(digest(bytes), expectedDigest, 'M3 registry differs from its frozen oracle');
  const registry = JSON.parse(bytes);
  const ids = new Set();
  const paths = new Set();
  for (const fixture of registry.fixtures) {
    assert.match(fixture.path, /^tests\/m3-fixtures\/conformance\/[a-z0-9-]+\.zry$/);
    assert(!ids.has(fixture.id), 'duplicate fixture id');
    ids.add(fixture.id);
    paths.add(fixture.path.split('/').at(-1));
    let current = root;
    for (const segment of fixture.path.split('/')) {
      current = resolve(current, segment);
      assert(!lstatSync(current).isSymbolicLink(), 'fixture link forbidden');
    }
    assert(lstatSync(current).isFile());
    assert.equal(digest(readFileSync(current)), fixture.sha256, fixture.id);
  }
  assert.deepEqual(readdirSync(resolve(root, 'tests/m3-fixtures/conformance')).sort(), [...paths].sort(),
    'unregistered or missing corpus fixture');
  const cases = new Set();
  for (const entry of [...registry.valid, ...registry.invalid, ...registry.runtimeInvalid, ...registry.faults]) {
    assert(!cases.has(entry.id), 'duplicate case id');
    cases.add(entry.id);
    assert(ids.has(entry.fixture), 'unregistered fixture');
  }
  for (const fault of registry.faults) {
    assert([2, 3, 4, 5].includes(fault.fault.code));
    assert(Number.isInteger(fault.fault.ordinal) && fault.fault.ordinal > 0 && fault.fault.ordinal <= 1048576);
    assert.equal(fault.expected.kind, 'trapped');
    assert.equal(fault.expected.code, `zryna.trap.${['', '', 'allocation', 'capacity', 'refcount', 'utf8'][fault.fault.code]}-v1`);
    assert(fault.trace.length <= 4096);
    for (const event of fault.trace) {
      if (event.kind === 'cleanup') {
        assert.deepEqual(Object.keys(event).sort(), ['function', 'kind', 'module', 'place']);
        for (const key of ['module', 'function', 'place']) assert(Number.isInteger(event[key]) && event[key] >= 0 && event[key] <= (key === 'place' ? 1048576 : 65535));
      } else if (event.kind === 'drop') {
        assert.deepEqual(Object.keys(event).sort(), ['kind', 'value']);
        assert(['string', 'sequence', 'enum', 'shared', 'weak'].includes(event.value));
      } else {
        assert.deepEqual(Object.keys(event), ['kind']);
        assert(['release-implicit-weak', 'release-control'].includes(event.kind));
      }
    }
  }
  for (const evidence of registry.evidence) {
    const source = readFileSync(resolve(root, evidence.path), 'utf8');
    assert(source.includes(`fn ${evidence.test.split('::').at(-1)}(`), evidence.id);
  }
  return registry;
}
export function loadAndValidateM3Conformance() {
  return validateM3Registry(readFileSync(resolve(workspaceRoot, 'tests/m3-conformance-v1.json')));
}
if (process.argv[1] && resolve(process.argv[1]) === scriptPath) {
  const registry = loadAndValidateM3Conformance();
  console.log(`M3 registry: ${registry.valid.length} valid, ${registry.invalid.length} invalid, ${registry.evidence.length} boundary oracles.`);
}
