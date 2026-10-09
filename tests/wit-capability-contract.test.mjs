import assert from 'node:assert/strict';
import { readFile, readdir } from 'node:fs/promises';
import path from 'node:path';
import test from 'node:test';

import {
  loadContract,
  parseWitWorlds,
  validateCapabilityRequest,
  validateRegistryObject,
  workspaceRoot,
} from '../scripts/wit-capabilities/validate.mjs';

const fixtureRoot = path.join(workspaceRoot, 'tests/wit-capability-v1/fixtures');

test('registry seals the exact package, worlds, WASI release and default-deny matrix', async () => {
  const { registry, witSource } = await loadContract();
  assert.equal(registry.wit.package, 'zryna:capability-profiles@0.1.0');
  assert.equal(registry.wasi.version, '0.2.12');
  assert.equal(registry.wasi.commit, '281ba75fafcd50961ef55f9e52747afcc9b71ede');
  assert.equal(Buffer.byteLength(witSource), 1129);
  assert.deepEqual(registry.profiles.map(({ id }) => id), ['browser', 'command', 'server']);
  assert.ok(registry.profiles.every(({ capabilities }) => capabilities.length === 5));
  assert.ok(registry.profiles[0].capabilities.every(({ decision }) => decision === 'denied'));
  assert.deepEqual(
    registry.profiles[2].capabilities.filter(({ decision }) => decision === 'denied').map(({ id }) => id),
    ['environment', 'filesystem'],
  );
});

test('documentation keeps the contract specified-only and preserves current wasm-web behavior', async () => {
  const [contract, roadmap, overview] = await Promise.all([
    readFile(path.join(workspaceRoot, 'spec/wit/CAPABILITY_PROFILES_V1.md'), 'utf8'),
    readFile(path.join(workspaceRoot, 'docs/ROADMAP.md'), 'utf8'),
    readFile(path.join(workspaceRoot, 'spec/language/OVERVIEW.md'), 'utf8'),
  ]);
  for (const phrase of [
    'A profile is an upper bound, not an ambient grant.',
    'Omitting M4 profile selection preserves the\ncurrent import-free core-WebAssembly behavior exactly.',
    'Component emission, host bindings, runtime enforcement, CLI activation',
    'Issue #357 owns cross-target and transitive dependency capability composition.',
    'Issue #359 owns\nJS/WASM value conversion and resource adapters.',
  ]) assert.ok(contract.includes(phrase), phrase);
  assert.match(roadmap, /Its `specified-only` state adds no component emission/);
  assert.match(overview, /It does not activate WASI or Component Model support/);
});

test('granted, denied, malformed and exact-limit fixtures have deterministic outcomes', async () => {
  const { registry, requestSchema } = await loadContract();
  const names = (await readdir(fixtureRoot)).filter((name) => name.endsWith('.json')).sort();
  assert.deepEqual(names, [
    'browser-default-denied.json',
    'browser-filesystem-denied.json',
    'command-environment-over-limit.json',
    'command-granted.json',
    'malformed-unknown-capability.json',
    'server-environment-denied.json',
    'server-granted.json',
    'unrequested-usage.json',
  ]);
  for (const name of names) {
    const fixture = JSON.parse(await readFile(path.join(fixtureRoot, name), 'utf8'));
    const run = () => validateCapabilityRequest(fixture.request, registry, requestSchema);
    if (fixture.expected) {
      assert.deepEqual(run(), fixture.expected, name);
      assert.deepEqual(run(), fixture.expected, `${name} replay`);
    } else {
      for (let replay = 0; replay < 2; replay++) {
        assert.throws(run, (error) => fixture.expectedError
          ? error.message === fixture.expectedError
          : error.message.startsWith(fixture.expectedErrorPrefix), `${name} replay ${replay}`);
      }
    }
  }
});

test('all capability decisions are executable as exhaustive allow-or-deny evidence', async () => {
  const { registry, requestSchema } = await loadContract();
  for (const profile of registry.profiles) {
    for (const capability of profile.capabilities) {
      const request = {
        schema: 'zryna.capability-request.v1',
        profile: profile.id,
        requests: [capability.id],
      };
      if (capability.decision === 'granted') {
        assert.deepEqual(validateCapabilityRequest(request, registry, requestSchema).granted, [capability.id]);
      } else {
        assert.throws(
          () => validateCapabilityRequest(request, registry, requestSchema),
          (error) => error.message === `ZRYNA-C4001: ${profile.id} denies ${capability.id}`,
        );
      }
    }
  }
});

test('registry and WIT validation fail closed on malformed or widened authority', async () => {
  const { registry, witSource, registrySchema } = await loadContract();
  const check = (value, source = witSource) => validateRegistryObject(value, source, registrySchema);
  const mutations = [
    (value) => { value.unknown = true; },
    (value) => { value.profiles[0].world = 'zryna:capability-profiles/command@0.1.0'; },
    (value) => { value.profiles[0].target = 'wasi-command'; },
    (value) => { value.profiles[0].capabilities[0].decision = 'granted'; },
    (value) => { value.profiles[1].capabilities[0].limits.maxTimers = 0; },
    (value) => { value.wit.sha256 = '0'.repeat(64); },
    (value) => { value.profiles.reverse(); },
  ];
  for (const mutate of mutations) {
    const changed = structuredClone(registry);
    mutate(changed);
    assert.throws(() => check(changed), /^Error: ZRYNA-C400[01]:/);
  }
  assert.throws(() => check(registry, witSource.replace('world browser {', 'world browser')), /ZRYNA-C4000/);
  assert.throws(() => check(registry, witSource.replace('world server {', 'world Browser {')), /ZRYNA-C4000/);
  assert.throws(() => check(registry, `${witSource}\ninterface surprise {}`), /ZRYNA-C4000/);
});

test('WIT and request resource bounds accept the exact limit and reject the first extra unit', async () => {
  const { registry, witSource, requestSchema } = await loadContract();
  const paddingBytes = registry.bounds.maxWitBytes - Buffer.byteLength(witSource) - 3;
  const exactWit = `${witSource}\n//${'x'.repeat(paddingBytes)}`;
  assert.equal(Buffer.byteLength(exactWit), registry.bounds.maxWitBytes);
  assert.equal(parseWitWorlds(exactWit, registry.bounds.maxWitBytes).worlds.size, 3);
  assert.throws(() => parseWitWorlds(`${exactWit}x`, registry.bounds.maxWitBytes), /ZRYNA-C4004/);

  const exact = {
    schema: 'zryna.capability-request.v1',
    profile: 'command',
    requests: ['environment'],
    usage: { environment: { maxEntries: 128, maxTotalBytes: 65536 } },
  };
  assert.deepEqual(validateCapabilityRequest(exact, registry, requestSchema).granted, ['environment']);
  const extraBytes = structuredClone(exact);
  extraBytes.usage.environment.maxTotalBytes++;
  assert.throws(() => validateCapabilityRequest(extraBytes, registry, requestSchema), /ZRYNA-C4003/);
  exact.usage.environment.maxEntries++;
  assert.throws(() => validateCapabilityRequest(exact, registry, requestSchema), /ZRYNA-C4003/);
  const reordered = {
    schema: 'zryna.capability-request.v1',
    profile: 'command',
    requests: ['network', 'clock'],
  };
  assert.throws(
    () => validateCapabilityRequest(reordered, registry, requestSchema),
    /ZRYNA-C4000/,
  );
});
