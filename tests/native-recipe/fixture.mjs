import { readFile } from 'node:fs/promises';

import { deriveCacheKey, validatePackageAuthority } from '../../scripts/build-plan/validate.mjs';
import { describeNativeRecipe, sourceKey } from '../../scripts/native-recipe/identity.mjs';
import { bindGraph, hash, makeFixture, wire } from '../package-release-v1/builders.mjs';

export { hash, wire } from '../package-release-v1/builders.mjs';

export function rekey(document) {
  document.cacheKey = deriveCacheKey(document);
  return wire(document);
}

// Synthetic declarative inputs only: these bytes never prove an installed native tool or ABI.
export async function fixture() {
  const envelope = makeFixture(2);
  const targets = ['javascript', 'native-linux-x86_64', 'webassembly'];
  envelope.lock.compatibility.targets = targets;
  for (const manifest of envelope.manifests) manifest.compatibility.targets = targets;
  bindGraph(envelope, 'package-01');
  const packageAuthority = validatePackageAuthority(wire(envelope));
  const document = JSON.parse(await readFile(new URL('../resolved-build-plan-v0/source-only.json', import.meta.url)));
  const plan = document.sourcePlan;
  plan.packageLockSha256 = packageAuthority.lockSha256;
  plan.rootPackage = structuredClone(packageAuthority.rootPackage);
  plan.packages = structuredClone(packageAuthority.packages);
  const sourceBytes = Buffer.from('synthetic source fixture\n');
  plan.sources = plan.packages.map((pkg) => ({
    graphRole: 'target/runtime', package: pkg.package,
    path: 'src/main.zry', size: sourceBytes.length, sha256: hash(sourceBytes),
  }));
  const toolMaterials = new Map(['cc', 'ld', 'zryna'].map((name) =>
    [name, Buffer.from(`synthetic ${name} tool; not executed\n`)]));
  plan.hostTools = [...toolMaterials].map(([name, bytes]) => ({
    name, version: '0.1.0', sha256: hash(bytes), runsOn: plan.host.triple,
    targets: name === 'zryna' ? targets : ['native-linux-x86_64'],
  }));
  plan.compiler.sha256 = hash(toolMaterials.get('zryna'));
  const runtime = { name: 'zryna-native-runtime', version: '1', sha256: '3'.repeat(64) };
  plan.targets.splice(1, 0, {
    id: 'native-linux-x86_64', triple: 'x86_64-unknown-linux-gnu', abi: 'zryna-native-scalar-v1',
    features: [], runtime,
    composition: { contract: 'zryna.cross-target-profiles.v1', row: 'NATIVE-HOST',
      hostPolicySha256: '0'.repeat(64), approvedRequestSha256: '1'.repeat(64) },
  });
  plan.outputs.splice(1, 0, { target: 'native-linux-x86_64', path: 'native/app.elf' });
  const sysroot = Buffer.from('synthetic sysroot inventory\n');
  const archive = Buffer.from('synthetic archive\n');
  document.nativeAppendix = {
    status: 'provisional-pending-364', target: 'native-linux-x86_64',
    abi: { identity: 'zryna-native-c-interop-v0', version: '0',
      targetTriple: 'x86_64-unknown-linux-gnu', callingConvention: 'system-v-amd64-c',
      carrierModel: 'native-c-interop-v0-carriers', ownershipModel: 'native-c-interop-v0-resources',
      runtime, decisionIssue: 364 },
    acquisition: {
      targetLibraries: [{ id: 'sample', version: '1.0.0', target: 'native-linux-x86_64',
        linkage: 'static', artifact: 'sample-a' }],
      sysroots: [{ id: 'linux-sysroot', target: 'native-linux-x86_64', sha256: hash(sysroot) }],
      staticArtifacts: [{ id: 'sample-a', target: 'native-linux-x86_64',
        size: archive.length, sha256: hash(archive) }],
      sharedArtifacts: [], runtimeDependencies: [],
    },
    compilation: { steps: [{ id: 'compile-sample', tool: 'cc', target: 'native-linux-x86_64',
      inputs: [{ kind: 'source', graphRole: 'target/runtime', package: plan.rootPackage, path: 'src/main.zry' },
        { kind: 'static', id: 'sample-a' }, { kind: 'sysroot', id: 'linux-sysroot' }],
      invocation: { adapter: 'zryna.native-compile-adapter.v0', mode: 'compile-only' }, output: 'sample-object' }] },
    linking: { tool: 'ld', target: 'native-linux-x86_64',
      inputs: [{ kind: 'object', id: 'sample-object' }, { kind: 'static', id: 'sample-a' },
        { kind: 'sysroot', id: 'linux-sysroot' }],
      invocation: { adapter: 'zryna.native-link-adapter.v0', buildId: 'none' }, output: 'native/app.elf' },
  };
  const planBytes = rekey(document);
  const review = describeNativeRecipe(planBytes, packageAuthority, 'compile-sample');
  const sourceMaterials = new Map(plan.sources.map((source) => [sourceKey(source), Buffer.from(sourceBytes)]));
  const inputMaterials = new Map([
    [`source:${sourceKey(plan.sources.find((source) => source.package.id === plan.rootPackage.id))}`, Buffer.from(sourceBytes)],
    ['static:sample-a', archive], ['sysroot:linux-sysroot', sysroot],
  ]);
  return { document, review, request: {
    mode: 'trusted-native', planBytes, packageAuthority, stepId: 'compile-sample',
    sourceMaterials, toolMaterials, inputMaterials, expectedRecipeSha256: review.recipeSha256,
  } };
}

export function provenance(review) {
  const bytes = Buffer.from('synthetic output; not an audited ELF\n');
  return {
    candidate: { recipeSha256: review.recipeSha256, planKey: review.bindings.planKey,
      mode: 'trusted-native', outputs: review.bindings.outputAllowlist.map((output) =>
        ({ ...output, size: bytes.length, sha256: hash(bytes) })) },
    outputMaterials: new Map(review.bindings.outputAllowlist.map((output) => [output.path, bytes])),
  };
}
