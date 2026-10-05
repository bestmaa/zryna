import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { readFile } from 'node:fs/promises';
import path from 'node:path';
import test from 'node:test';

import { bindGraph, checksum, makeFixture, recordHash, wire } from './package-release-v1/builders.mjs';
import { fixture as recipeFixture } from './native-recipe/fixture.mjs';
import { describeNativeRecipe, preflightNativeRecipe } from '../scripts/native-recipe/preflight.mjs';

import {
  deriveCacheKey,
  deriveTargetCacheKey,
  loadBuildPlan,
  validateBuildPlan,
  validateBuildPlanBytes,
  validateCacheEntry,
  validatePackageAuthority,
  validatePublicationObservation,
  verifySourceMaterials,
  workspaceRoot,
} from '../scripts/build-plan/validate.mjs';

function withCache(document) {
  document.cacheKey = deriveCacheKey(document);
  return document;
}

function independentCacheKey(document) {
  const material = {
    format: document.format,
    sourcePlan: document.sourcePlan,
    status: document.status,
    version: document.version,
  };
  if (document.nativeAppendix) material.nativeAppendix = document.nativeAppendix;
  return createHash('sha256')
    .update(Buffer.from('ZRYNA-RESOLVED-BUILD-PLAN-V0\0plan\0'))
    .update(wire(material))
    .digest('hex');
}

const SOURCE_ONLY_CACHE_KEY = '71e0a59c6c30dbb07e17c3f72ae1aa5a73bf273b27dfd7bc8a962d1887768d62';

function bindPlanToFixture(template, fixture, packageAuthority) {
  const document = structuredClone(template);
  const manifests = new Map(fixture.manifests.map((manifest) =>
    [recordHash('manifest', manifest), manifest]));
  document.sourcePlan.packageLockSha256 = packageAuthority.lockSha256;
  document.sourcePlan.rootPackage = structuredClone(packageAuthority.rootPackage);
  document.sourcePlan.packages = structuredClone(packageAuthority.packages);
  document.sourcePlan.sources = packageAuthority.packages.flatMap((pkg) =>
    manifests.get(pkg.package.id).files.map((file) => ({
      graphRole: 'target/runtime',
      package: structuredClone(pkg.package),
      ...file,
    })));
  return withCache(document);
}

function nativePlan(sourceOnly) {
  const document = structuredClone(sourceOnly);
  document.sourcePlan.targets.splice(1, 0, {
    id: 'native-linux-x86_64',
    triple: 'x86_64-unknown-linux-gnu',
    abi: 'zryna-native-scalar-v1',
    features: [],
    composition: {
      contract: 'zryna.cross-target-profiles.v1',
      row: 'NATIVE-HOST',
      hostPolicySha256: '0'.repeat(64),
      approvedRequestSha256: '1'.repeat(64),
    },
    runtime: { name: 'zryna-native-runtime', version: '1', sha256: '3'.repeat(64) },
  });
  document.sourcePlan.hostTools = [
    { name: 'clang', version: '18.1.8', sha256: '4'.repeat(64), runsOn: 'x86_64-unknown-linux-gnu', targets: ['native-linux-x86_64'] },
    { name: 'lld', version: '18.1.8', sha256: '5'.repeat(64), runsOn: 'x86_64-unknown-linux-gnu', targets: ['native-linux-x86_64'] },
    { ...document.sourcePlan.hostTools[0], targets: ['javascript', 'native-linux-x86_64', 'webassembly'] },
  ];
  document.sourcePlan.outputs.splice(1, 0, { path: 'native/app.elf', target: 'native-linux-x86_64' });
  document.nativeAppendix = {
    status: 'provisional-pending-364',
    target: 'native-linux-x86_64',
    abi: {
      identity: 'zryna-native-c-interop-v0',
      version: '0',
      targetTriple: 'x86_64-unknown-linux-gnu',
      callingConvention: 'sysv-amd64-c-v0',
      carrierModel: 'native-c-interop-v0-carriers',
      ownershipModel: 'native-c-interop-v0-resources',
      runtime: { name: 'zryna-native-runtime', version: '1', sha256: '3'.repeat(64) },
      decisionIssue: 364,
    },
    acquisition: {
      targetLibraries: [
        { id: 'libc', version: '2.39', target: 'native-linux-x86_64', linkage: 'shared', artifact: 'libc-so' },
        { id: 'sample', version: '1.0.0', target: 'native-linux-x86_64', linkage: 'static', artifact: 'libsample-a' },
      ],
      sysroots: [{ id: 'linux-sysroot', target: 'native-linux-x86_64', sha256: '6'.repeat(64) }],
      staticArtifacts: [{ id: 'libsample-a', target: 'native-linux-x86_64', sha256: '7'.repeat(64), size: 128 }],
      sharedArtifacts: [{ id: 'libc-so', target: 'native-linux-x86_64', sha256: '8'.repeat(64), size: 256 }],
      runtimeDependencies: [{ id: 'libc-runtime', target: 'native-linux-x86_64', sharedArtifact: 'libc-so' }],
    },
    compilation: {
      steps: [{
        id: 'compile-sample',
        tool: 'clang',
        target: 'native-linux-x86_64',
        inputs: [{
          kind: 'source',
          graphRole: 'target/runtime',
          package: structuredClone(document.sourcePlan.rootPackage),
          path: 'src/main.zry',
        }],
        invocation: { adapter: 'zryna.native-compile-adapter.v0', mode: 'compile-only' },
        output: 'sample-object',
      }],
    },
    linking: {
      tool: 'lld',
      target: 'native-linux-x86_64',
      inputs: [
        { kind: 'object', id: 'sample-object' },
        { kind: 'static', id: 'libsample-a' },
        { kind: 'shared', id: 'libc-so' },
        { kind: 'sysroot', id: 'linux-sysroot' },
      ],
      invocation: { adapter: 'zryna.native-link-adapter.v0', buildId: 'none' },
      output: 'native/app.elf',
    },
  };
  return withCache(document);
}

test('source-only v0 plan validates and canonical replay is stable', async () => {
  const { document, packageAuthority, receipt } = await loadBuildPlan();
  const fixtureBytes = await readFile(path.join(workspaceRoot, 'tests/resolved-build-plan-v0/source-only.json'));
  const packageFixture = JSON.parse(await readFile(path.join(workspaceRoot, 'tests/package-release-v1/valid.json'), 'utf8'));
  assert.equal(document.sourcePlan.packageLockSha256, packageFixture.release.lockSha256);
  assert.deepEqual(document.sourcePlan.packages.map(({ package: identity }) => identity),
    packageFixture.lock.packages.map(({ id, sourceSha256 }) => ({ id, sourceSha256 })));
  assert.deepEqual(document.sourcePlan.packages.flatMap((pkg) => pkg.dependencies.map((dependency) => dependency.alias)),
    packageFixture.lock.packages.flatMap((pkg) => pkg.dependencies.map((dependency) => dependency.alias)));
  assert.deepEqual(document.sourcePlan.targets.map(({ id }) => id), packageFixture.lock.compatibility.targets);
  assert.equal(document.cacheKey, SOURCE_ONLY_CACHE_KEY);
  assert.equal(independentCacheKey(document), SOURCE_ONLY_CACHE_KEY);
  assert.deepEqual(receipt, { cacheKey: document.cacheKey, native: false });
  assert.deepEqual(fixtureBytes, wire(document));
  assert.deepEqual(validateBuildPlan(document, packageAuthority), receipt);
  assert.deepEqual(validateBuildPlanBytes(fixtureBytes, packageAuthority), receipt);
  assert.deepEqual(validateBuildPlanBytes(fixtureBytes, packageAuthority), receipt);
  assert.throws(() => validateBuildPlanBytes(Buffer.from(` ${fixtureBytes}`), packageAuthority), /P361-WIRE/);
  const unknown = structuredClone(document);
  unknown.unbound = 'not-in-cache-material';
  assert.equal(deriveCacheKey(unknown), document.cacheKey);
  assert.throws(() => validateBuildPlanBytes(wire(unknown), packageAuthority), /P361-SCHEMA/);
});

test('validated #168 projection authenticates lock, root, exact pairs, aliases and edges', async () => {
  const { document, packageAuthority } = await loadBuildPlan();
  assert.throws(() => validateBuildPlan(document, structuredClone(packageAuthority)),
    /P361-IDENTITY: a validated #168 package authority is required/);
  const forgedLock = structuredClone(document);
  forgedLock.sourcePlan.packageLockSha256 = '0'.repeat(64);
  assert.throws(() => validateBuildPlan(withCache(forgedLock), packageAuthority),
    /P361-IDENTITY: package lock digest differs/);
  const forgedRoot = structuredClone(document);
  forgedRoot.sourcePlan.rootPackage = structuredClone(forgedRoot.sourcePlan.packages[1].package);
  assert.throws(() => validateBuildPlan(withCache(forgedRoot), packageAuthority), /P361-IDENTITY/);
  const forgedAlias = structuredClone(document);
  forgedAlias.sourcePlan.packages[0].dependencies[0].alias = 'other';
  assert.throws(() => validateBuildPlan(withCache(forgedAlias), packageAuthority),
    /P361-IDENTITY: package pairs or alias edges differ/);
  const forgedEdge = structuredClone(document);
  forgedEdge.sourcePlan.packages[0].dependencies[0].package.id = '0'.repeat(64);
  assert.throws(() => validateBuildPlan(withCache(forgedEdge), packageAuthority),
    /P361-IDENTITY: dependency .* is absent/);
});

test('validated #168 compatibility binds compiler, profile and requested target coverage', async () => {
  const { document, packageAuthority } = await loadBuildPlan();
  const compiler = structuredClone(document);
  compiler.sourcePlan.compiler.version = '0.1.1';
  compiler.sourcePlan.hostTools[0].version = '0.1.1';
  assert.throws(() => validateBuildPlan(withCache(compiler), packageAuthority),
    /P361-TOOLCHAIN: compiler version differs from validated #168 compatibility/);
  const profile = structuredClone(document);
  profile.sourcePlan.profile.id = 'control-flow-v1';
  assert.throws(() => validateBuildPlan(withCache(profile), packageAuthority),
    /P361-TARGET: profile differs from validated #168 compatibility/);
  const target = nativePlan(document);
  assert.throws(() => validateBuildPlan(target, packageAuthority),
    /P361-TARGET: target set differs from validated #168 compatibility/);

  const removedTarget = structuredClone(document);
  removedTarget.sourcePlan.targets.pop();
  removedTarget.sourcePlan.hostTools[0].targets.pop();
  removedTarget.sourcePlan.outputs.pop();
  assert.throws(() => validateBuildPlan(withCache(removedTarget), packageAuthority),
    /P361-TARGET: target set differs from validated #168 compatibility/);
});

test('every selected target is admitted by the exact compiler tool', async () => {
  const { document, packageAuthority } = await loadBuildPlan();
  const uncovered = structuredClone(document);
  uncovered.sourcePlan.hostTools[0].targets.pop();
  assert.throws(() => validateBuildPlan(withCache(uncovered), packageAuthority),
    /P361-TOOLCHAIN: compiler does not admit target webassembly/);
});

test('cache identity binds source, dependency, compiler, runtime, profile, target and host-tool inputs', async () => {
  const { document } = await loadBuildPlan();
  const changes = [
    (plan) => { plan.sourcePlan.sources[0].sha256 = '0'.repeat(64); },
    (plan) => { plan.sourcePlan.packageLockSha256 = '0'.repeat(64); },
    (plan) => { plan.sourcePlan.packages[0].package.id = '0'.repeat(64); },
    (plan) => { plan.sourcePlan.packages[0].package.sourceSha256 = '0'.repeat(64); },
    (plan) => { plan.sourcePlan.packages[0].dependencies[0].package.id = '3'.repeat(64); },
    (plan) => { plan.sourcePlan.packages[0].graphRole = 'host/build'; },
    (plan) => { plan.sourcePlan.compiler.version = '0.1.1'; },
    (plan) => { plan.sourcePlan.profile.configurationSha256 = '0'.repeat(64); },
    (plan) => { plan.sourcePlan.executionPolicy.configurationSha256 = '0'.repeat(64); },
    (plan) => { plan.sourcePlan.host.environment[0].value = 'en_US.UTF-8'; },
    (plan) => { plan.sourcePlan.targets[0].runtime.version = '2'; },
    (plan) => { plan.sourcePlan.targets[0].composition.hostPolicySha256 = '2'.repeat(64); },
    (plan) => { plan.sourcePlan.targets[0].triple = 'ecmascript2024-unknown-unknown'; },
    (plan) => { plan.sourcePlan.hostTools[0].sha256 = '0'.repeat(64); },
  ];
  for (const change of changes) {
    const changed = structuredClone(document);
    change(changed);
    assert.notEqual(deriveCacheKey(changed), document.cacheKey);
  }
});

test('source material repeat succeeds while missing, extra and stale bytes fail closed', async () => {
  const { document, packageAuthority } = await loadBuildPlan();
  const materialKey = (source) =>
    `${source.graphRole}:${source.package.id}:${source.package.sourceSha256}:${source.path}`;
  const materials = new Map(document.sourcePlan.sources.map((source) =>
    [materialKey(source), Buffer.from('synthetic source fixture\n')]));
  assert.equal(verifySourceMaterials(document, materials), true);
  assert.equal(verifySourceMaterials(document, materials), true);
  const missing = new Map(materials);
  missing.delete(materialKey(document.sourcePlan.sources[1]));
  assert.throws(() => verifySourceMaterials(document, missing), /P361-SOURCE: source material inventory has missing or extra entries/);
  const extra = new Map(materials);
  extra.set('target/runtime:extra:extra:src/extra.zry', Buffer.alloc(0));
  assert.throws(() => verifySourceMaterials(document, extra), /P361-SOURCE: source material inventory has missing or extra entries/);
  const stale = new Map(materials);
  stale.set(materialKey(document.sourcePlan.sources[0]), Buffer.from('changed'));
  assert.throws(() => verifySourceMaterials(document, stale), /P361-SOURCE: stale source material/);
  const mismatchedDigest = structuredClone(document);
  mismatchedDigest.sourcePlan.sources[0].sha256 = '0'.repeat(64);
  assert.throws(() => validateBuildPlan(withCache(mismatchedDigest), packageAuthority), /P361-SOURCE: .* source digest differs from #168/);
});

test('source collection budget rejects the first extra before schema validation', async () => {
  const { document, packageAuthority } = await loadBuildPlan();
  const extra = structuredClone(document);
  while (extra.sourcePlan.sources.length < 257) {
    extra.sourcePlan.sources.push(structuredClone(extra.sourcePlan.sources.at(-1)));
  }
  assert.throws(() => validateBuildPlan(withCache(extra), packageAuthority),
    /P361-BUDGET: sources exceeds 256/);
});

test('source path accepts 96 ASCII bytes and rejects byte 97', async () => {
  const { document } = await loadBuildPlan();
  const fixture = makeFixture();
  const path96 = `src/${'a'.repeat(88)}.zry`;
  assert.equal(Buffer.byteLength(path96), 96);
  for (const manifest of fixture.manifests) {
    manifest.files = [checksum(path96, 'synthetic source fixture\n')];
  }
  bindGraph(fixture, 'package-01');
  const authority = validatePackageAuthority(wire(fixture));
  const exact = bindPlanToFixture(document, fixture, authority);
  assert.doesNotThrow(() => validateBuildPlan(exact, authority));
  const extra = structuredClone(exact);
  extra.sourcePlan.sources[0].path = `src/${'a'.repeat(89)}.zry`;
  assert.equal(Buffer.byteLength(extra.sourcePlan.sources[0].path), 97);
  assert.throws(() => validateBuildPlan(withCache(extra), authority), /P361-SCHEMA/);
});

test('target graph rejects deterministic cycles, orphans and host/build occurrences', async () => {
  const { document, packageAuthority } = await loadBuildPlan();
  const cyclic = structuredClone(document);
  cyclic.sourcePlan.packages[1].dependencies = [{
    alias: 'root',
    graphRole: 'target/runtime',
    package: structuredClone(cyclic.sourcePlan.packages[0].package),
  }];
  assert.throws(() => validateBuildPlan(withCache(cyclic), packageAuthority),
    /P361-IDENTITY: dependency cycle/);
  const orphaned = structuredClone(document);
  orphaned.sourcePlan.packages[0].dependencies = [];
  assert.throws(() => validateBuildPlan(withCache(orphaned), packageAuthority),
    /P361-IDENTITY: unreachable package/);
  const hostBuild = structuredClone(document);
  hostBuild.sourcePlan.packages[0].graphRole = 'host/build';
  assert.throws(() => validateBuildPlan(withCache(hostBuild), packageAuthority), /P361-SCHEMA/);
});

test('native draft separates acquisition, compilation and linking and rejects boundary confusion', async () => {
  const { document } = await loadBuildPlan();
  const fixture = makeFixture();
  const targets = ['javascript', 'native-linux-x86_64', 'webassembly'];
  fixture.lock.compatibility.targets = targets;
  for (const manifest of fixture.manifests) manifest.compatibility.targets = targets;
  bindGraph(fixture, 'package-01');
  const packageAuthority = validatePackageAuthority(wire(fixture));
  const valid = nativePlan(bindPlanToFixture(document, fixture, packageAuthority));
  assert.deepEqual(validateBuildPlan(valid, packageAuthority), { cacheKey: valid.cacheKey, native: true });
  const changedArtifact = structuredClone(valid);
  changedArtifact.nativeAppendix.acquisition.staticArtifacts[0].sha256 = '9'.repeat(64);
  assert.notEqual(deriveCacheKey(changedArtifact), valid.cacheKey);

  const wrongTarget = structuredClone(valid);
  wrongTarget.nativeAppendix.acquisition.staticArtifacts[0].target = 'javascript';
  assert.throws(() => validateBuildPlan(withCache(wrongTarget), packageAuthority), /P361-TARGET: libsample-a has the wrong target/);

  const wrongRuntime = structuredClone(valid);
  wrongRuntime.nativeAppendix.abi.runtime.sha256 = '0'.repeat(64);
  assert.throws(() => validateBuildPlan(withCache(wrongRuntime), packageAuthority), /P361-NATIVE: native ABI runtime identity differs/);

  const missingLibrary = structuredClone(valid);
  missingLibrary.nativeAppendix.acquisition.targetLibraries[1].artifact = 'missing-a';
  assert.throws(() => validateBuildPlan(withCache(missingLibrary), packageAuthority), /P361-NATIVE: sample is missing its static artifact/);

  const undeclaredTool = structuredClone(valid);
  undeclaredTool.nativeAppendix.compilation.steps[0].tool = 'ambient-cc';
  assert.throws(() => validateBuildPlan(withCache(undeclaredTool), packageAuthority), /P361-TOOLCHAIN: undeclared compilation tool/);

  for (const argument of ['sample.c', '@args.rsp', '-L/tmp/lib', '-lsample']) {
    const ambientArgument = structuredClone(valid);
    ambientArgument.nativeAppendix.compilation.steps[0].arguments = [argument];
    assert.throws(() => validateBuildPlan(withCache(ambientArgument), packageAuthority), /P361-SCHEMA/);
  }
  const wrongAdapter = structuredClone(valid);
  wrongAdapter.nativeAppendix.linking.invocation.adapter = 'ambient-linker';
  assert.throws(() => validateBuildPlan(withCache(wrongAdapter), packageAuthority), /P361-SCHEMA/);
  const mismatchedOutput = structuredClone(valid);
  mismatchedOutput.nativeAppendix.linking.output = 'native/other.elf';
  assert.throws(() => validateBuildPlan(withCache(mismatchedOutput), packageAuthority),
    /P361-NATIVE: linker output is not an exact declared target output/);
});

test('accepted #357 profile and target rows do not widen or cross target axes', async () => {
  const { document, packageAuthority } = await loadBuildPlan();
  const wrongRow = structuredClone(document);
  wrongRow.sourcePlan.targets[0].composition.row = 'U-WASM';
  assert.throws(() => validateBuildPlan(withCache(wrongRow), packageAuthority), /P361-TARGET: U-WASM is incompatible with javascript/);
  const unresolvedAll = structuredClone(document);
  unresolvedAll.sourcePlan.targets[0].id = 'all';
  assert.throws(() => validateBuildPlan(withCache(unresolvedAll), packageAuthority), /P361-SCHEMA/);
});

test('proposed D2 pins the exact #364 tuple without accepting legacy aliases or status claims', async () => {
  const { document, request } = await recipeFixture();
  document.nativeAppendix.status = 'proposed-specified-native-c-interop-v0';
  document.nativeAppendix.abi.callingConvention = 'sysv-amd64-c-v0';
  const valid = withCache(document);
  assert.deepEqual(validateBuildPlanBytes(wire(valid), request.packageAuthority), {
    cacheKey: valid.cacheKey, native: true, nativeAdmission: 'denied-proposed',
  });
  for (const [field, value] of [
    ['identity', 'another-abi'], ['version', '1'], ['version', 0],
    ['targetTriple', 'x86_64-pc-windows-msvc'], ['callingConvention', 'system-v-amd64-c'],
    ['carrierModel', 'scalar-v1'], ['ownershipModel', 'ownership-runtime-v1'],
    ['decisionIssue', 361], ['artifactApproved', true],
  ]) {
    const malformed = structuredClone(valid);
    malformed.nativeAppendix.abi[field] = value;
    assert.throws(() => validateBuildPlanBytes(wire(withCache(malformed)), request.packageAuthority), /P361-SCHEMA/);
  }
  for (const status of ['specified-native-c-interop-v0', 'accepted', 'implemented', 'conformance-passed', 'publicly-supported']) {
    const claimed = structuredClone(valid);
    claimed.nativeAppendix.status = status;
    assert.throws(() => validateBuildPlan(withCache(claimed), request.packageAuthority), /P361-SCHEMA/);
  }
  const crossTarget = structuredClone(valid);
  crossTarget.nativeAppendix.target = 'javascript';
  assert.throws(() => validateBuildPlan(withCache(crossTarget), request.packageAuthority), /P361-SCHEMA/);
  const wrongTriple = structuredClone(valid);
  wrongTriple.nativeAppendix.abi.targetTriple = wrongTriple.sourcePlan.targets[1].triple = 'aarch64-unknown-linux-gnu';
  assert.throws(() => validateBuildPlan(withCache(wrongTriple), request.packageAuthority), /P361-SCHEMA/);
});

test('proposed runtime hashes bind identity but never authenticate bytes or admit recipe execution', async () => {
  const { document, request } = await recipeFixture();
  document.nativeAppendix.status = 'proposed-specified-native-c-interop-v0';
  document.nativeAppendix.abi.callingConvention = 'sysv-amd64-c-v0';
  const initialKey = deriveCacheKey(document);
  const mismatched = structuredClone(document);
  mismatched.nativeAppendix.abi.runtime = { ...mismatched.nativeAppendix.abi.runtime, sha256: '0'.repeat(64) };
  assert.throws(() => validateBuildPlan(withCache(mismatched), request.packageAuthority), /P361-NATIVE: native ABI runtime identity differs/);
  document.nativeAppendix.abi.runtime.sha256 = document.sourcePlan.targets[1].runtime.sha256 = '9'.repeat(64);
  const planBytes = wire(withCache(document));
  const receipt = validateBuildPlanBytes(planBytes, request.packageAuthority);
  assert.equal(receipt.nativeAdmission, 'denied-proposed');
  assert.notEqual(receipt.cacheKey, initialKey);
  assert.equal(receipt.cacheKey, independentCacheKey(document));
  const review = describeNativeRecipe(planBytes, request.packageAuthority, request.stepId);
  assert.throws(() => preflightNativeRecipe({ ...request, planBytes, expectedRecipeSha256: review.recipeSha256,
    nativeAppendixAccepted: true, runtimeArtifactAuthenticated: true, ffiConformance: true }),
  /TRUST-NATIVE: native appendix acceptance and authenticated ABI\/host enforcement are unavailable/);
});

test('cache miss, hit, wrong target and stale output have distinct deterministic outcomes', async () => {
  const { document } = await loadBuildPlan();
  assert.deepEqual(validateCacheEntry(document, 'javascript', undefined, new Map()), { outcome: 'miss' });
  const bytes = Buffer.from('export function main(){return 1}\n');
  const entry = {
    key: deriveTargetCacheKey(document, 'javascript'),
    target: 'javascript',
    outputs: [{ path: 'javascript/app.mjs', size: bytes.length, sha256: createHash('sha256').update(bytes).digest('hex') }],
  };
  for (const malformed of [null, {}, { key: entry.key, target: entry.target },
    { ...entry, key: 'too-short' }, { ...entry, target: 'Invalid Target' },
    { ...entry, outputs: 'not-an-array' }, { ...entry, extra: true }]) {
    assert.throws(() => validateCacheEntry(document, 'javascript', malformed, new Map()),
      /P361-CACHE: cache entry metadata is malformed or incomplete/);
  }
  const oversized = { ...entry, outputs: Array.from({ length: 17 }, () => structuredClone(entry.outputs[0])) };
  assert.throws(() => validateCacheEntry(document, 'javascript', oversized, new Map()), /P361-CACHE/);
  const duplicate = { ...entry, outputs: [structuredClone(entry.outputs[0]), structuredClone(entry.outputs[0])] };
  assert.throws(() => validateCacheEntry(document, 'javascript', duplicate, new Map()), /P361-CACHE/);
  for (const output of [null, {}, { ...entry.outputs[0], extra: true },
    { ...entry.outputs[0], sha256: 'too-short' }, { ...entry.outputs[0], size: -1 }]) {
    assert.throws(() => validateCacheEntry(document, 'javascript', { ...entry, outputs: [output] },
      new Map([['javascript/app.mjs', bytes]])), /P361-CACHE: cache output metadata is malformed/);
  }
  assert.throws(() => validateCacheEntry(document, 'javascript', entry, {}), /P361-CACHE/);
  assert.throws(() => validateCacheEntry(document, 'javascript', entry,
    new Map([['javascript/extra.mjs', bytes]])), /P361-CACHE: cache output material inventory has extra entries/);
  assert.deepEqual(validateCacheEntry(document, 'javascript', entry, new Map([['javascript/app.mjs', bytes]])), {
    outcome: 'hit',
    key: entry.key,
  });
  assert.throws(
    () => validateCacheEntry(document, 'javascript', { ...entry, target: 'native-linux-x86_64' },
      new Map([['javascript/app.mjs', bytes]])),
    /P361-CACHE: cache entry identity is incompatible/,
  );
  assert.throws(
    () => validateCacheEntry(document, 'javascript', entry, new Map([['javascript/app.mjs', Buffer.from('stale')]])),
    /P361-CACHE: stale cached output/,
  );
});

test('interrupted and create-only publication preserve the destination boundary', () => {
  assert.deepEqual(validatePublicationObservation({
    destinationExistedBefore: false,
    commitAttempted: false,
    commitCompleted: false,
    destinationVisible: false,
    completeManifest: false,
    priorDestinationPreserved: false,
  }), { outcome: 'interrupted-hidden' });
  assert.deepEqual(validatePublicationObservation({
    destinationExistedBefore: true,
    commitAttempted: false,
    commitCompleted: false,
    destinationVisible: true,
    completeManifest: true,
    priorDestinationPreserved: true,
  }), { outcome: 'collision-preserved' });
  assert.throws(() => validatePublicationObservation({
    destinationExistedBefore: false,
    commitAttempted: true,
    commitCompleted: false,
    destinationVisible: true,
    completeManifest: false,
    priorDestinationPreserved: false,
  }), /P361-PUBLICATION: interrupted staging became visible/);
});

test('documentation keeps authority, exact identities and implementation stages explicit', async () => {
  const contract = await readFile(path.join(workspaceRoot, 'spec/package/RESOLVED_BUILD_PLAN_V0.md'), 'utf8');
  const normalized = contract.replace(/\s+/g, ' ');
  for (const phrase of [
    'Package and provenance authority remains with #168.',
    'The package identity is the exact ordered pair `(id, sourceSha256)`',
    'Source-only v0 rejects every `host/build` occurrence rather than inventing an unauthenticated root.',
    '`zryna.cross-target-profiles.v1`',
    'The driver alone owns compilation orchestration, tool validation, linking, cache materialization, and output publication.',
    'The legacy native appendix stays `provisional-pending-364`.',
    'specified → implemented → conformance-passed → publicly supported',
  ]) assert.ok(normalized.includes(phrase), phrase);
});
