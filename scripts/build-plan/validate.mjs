import { createHash } from 'node:crypto';
import { readFileSync } from 'node:fs';
import { readFile } from 'node:fs/promises';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

import Ajv2020 from 'ajv/dist/2020.js';

import { digest as packageDigest } from '../package-release/canonical.mjs';
import { validateCollectionBudgets } from './budgets.mjs';
import { buildPlanReceipt, canonicalBytes, deriveCacheKey, deriveTargetCacheKey } from './plan-identity.mjs';

export { canonicalBytes, deriveCacheKey, deriveTargetCacheKey } from './plan-identity.mjs';
import {
  packageKey,
  rolePackageKey,
  validateAuthenticatedPackageGraph,
  validatePackageAuthority,
} from './package-authority.mjs';

export { validatePackageAuthority } from './package-authority.mjs';

export const workspaceRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../..');
export const MAX_PLAN_BYTES = 262144;
const schema = JSON.parse(readFileSync(
  new URL('../../schemas/zryna-resolved-build-plan-v0.schema.json', import.meta.url), 'utf8',
));
const validateSchema = new Ajv2020({ allErrors: true, strict: true }).compile(schema);

function fail(code, detail) {
  throw new Error(`${code}: ${detail}`);
}

function compareAscii(left, right) {
  return Buffer.compare(Buffer.from(left, 'ascii'), Buffer.from(right, 'ascii'));
}

function orderedUnique(items, key, label) {
  for (let index = 1; index < items.length; index++) {
    if (compareAscii(key(items[index - 1]), key(items[index])) >= 0) {
      fail('P361-ORDER', `${label} must be strictly ASCII-sorted and unique`);
    }
  }
}

function materialKey(role, pkg, sourcePath) {
  return `${role}:${pkg.id}:${pkg.sourceSha256}:${sourcePath}`;
}

function validateSourcePlan(plan, packageAuthority) {
  orderedUnique(plan.packages, (item) => rolePackageKey(item.graphRole, item.package), 'packages');
  orderedUnique(plan.sources, (item) => `${rolePackageKey(item.graphRole, item.package)}\0${item.path}`, 'sources');
  orderedUnique(plan.targets, (item) => item.id, 'targets');
  orderedUnique(plan.hostTools, (item) => item.name, 'host tools');
  orderedUnique(plan.outputs, (item) => `${item.target}\0${item.path}`, 'outputs');
  orderedUnique(plan.host.environment, (item) => item.name, 'host environment');

  const packagesByKey = new Map(plan.packages.map((pkg) => [rolePackageKey(pkg.graphRole, pkg.package), pkg]));
  const packageIds = new Set(packagesByKey.keys());
  if (!packageIds.has(rolePackageKey('target/runtime', plan.rootPackage))) fail('P361-IDENTITY', 'root target/runtime package is absent');
  for (const pkg of plan.packages) {
    orderedUnique(pkg.dependencies, (item) => item.alias, `${pkg.package.id} dependencies`);
    for (const dependency of pkg.dependencies) {
      if (dependency.graphRole !== pkg.graphRole || !packageIds.has(rolePackageKey(dependency.graphRole, dependency.package))) {
        fail('P361-IDENTITY', `dependency ${dependency.alias} is absent`);
      }
    }
  }
  validateAuthenticatedPackageGraph(plan, packagesByKey, packageAuthority);
  if (plan.compiler.version !== packageAuthority.compatibility.compiler) {
    fail('P361-TOOLCHAIN', 'compiler version differs from validated #168 compatibility');
  }
  if (plan.profile.id !== packageAuthority.compatibility.profile) {
    fail('P361-TARGET', 'profile differs from validated #168 compatibility');
  }
  const compatibilityTargets = packageAuthority.compatibility.targets;
  if (plan.targets.length !== compatibilityTargets.length ||
      plan.targets.some(({ id }, index) => id !== compatibilityTargets[index])) {
    fail('P361-TARGET', 'target set differs from validated #168 compatibility');
  }
  for (const source of plan.sources) {
    if (!packageIds.has(rolePackageKey(source.graphRole, source.package))) fail('P361-SOURCE', `source package ${source.package.id} is absent`);
  }
  const sourceKeys = new Set(plan.sources.map((source) => materialKey(source.graphRole, source.package, source.path)));
  for (const pkg of plan.packages) {
    const files = plan.sources
      .filter((source) => source.graphRole === pkg.graphRole && packageKey(source.package) === packageKey(pkg.package))
      .map(({ path, sha256, size }) => ({ path, sha256, size }));
    if (files.length === 0) fail('P361-SOURCE', `${pkg.package.id} has no source files`);
    if (files.length > 16) fail('P361-BUDGET', `${pkg.package.id} exceeds #168's file bound`);
    if (pkg.package.sourceSha256 !== packageDigest('source-files', files)) {
      fail('P361-SOURCE', `${pkg.package.id} source digest differs from #168`);
    }
  }

  const targets = new Map(plan.targets.map((target) => [target.id, target]));
  const compositionRows = {
    javascript: new Set(['JS-BROWSER', 'JS-NODE', 'U-JS']),
    'native-linux-x86_64': new Set(['NATIVE-HOST', 'U-NATIVE']),
    webassembly: new Set(['U-WASM', 'WIT-BROWSER', 'WIT-COMMAND', 'WIT-SERVER']),
  };
  for (const target of plan.targets) {
    orderedUnique(target.features, (item) => item, `${target.id} features`);
    if (!compositionRows[target.id].has(target.composition.row)) {
      fail('P361-TARGET', `${target.composition.row} is incompatible with ${target.id}`);
    }
  }
  for (const output of plan.outputs) {
    if (!targets.has(output.target)) fail('P361-TARGET', `output target ${output.target} is absent`);
  }

  const tools = new Map(plan.hostTools.map((tool) => [tool.name, tool]));
  const compiler = tools.get(plan.compiler.tool);
  if (!compiler || compiler.version !== plan.compiler.version || compiler.sha256 !== plan.compiler.sha256) {
    fail('P361-TOOLCHAIN', 'compiler must match one exact declared host tool');
  }
  for (const target of plan.targets) {
    if (!compiler.targets.includes(target.id)) {
      fail('P361-TOOLCHAIN', `compiler does not admit target ${target.id}`);
    }
  }
  for (const tool of plan.hostTools) {
    if (tool.runsOn !== plan.host.triple) fail('P361-TOOLCHAIN', `${tool.name} does not run on the declared host`);
    orderedUnique(tool.targets, (item) => item, `${tool.name} targets`);
    for (const target of tool.targets) {
      if (!targets.has(target)) fail('P361-TARGET', `${tool.name} names undeclared target ${target}`);
    }
  }
  const environment = new Map(plan.host.environment.map((entry) => [entry.name, entry.value]));
  if (environment.get('LC_ALL') !== 'C' || environment.get('TZ') !== 'UTC' ||
      !/^(?:0|[1-9][0-9]{0,9})$/.test(environment.get('SOURCE_DATE_EPOCH') ?? '')) {
    fail('P361-TOOLCHAIN', 'reproduction environment differs from #168');
  }
  const outputKeys = new Set(plan.outputs.map((output) => `${output.target}\0${output.path}`));
  return { packageIds, sourceKeys, targets, tools, outputKeys };
}

function indexById(items, label) {
  orderedUnique(items, (item) => item.id, label);
  return new Map(items.map((item) => [item.id, item]));
}

function validateNativeAppendix(appendix, sourceAuthority) {
  const target = sourceAuthority.targets.get(appendix.target);
  if (!target || !appendix.target.startsWith('native-')) {
    fail('P361-TARGET', 'native appendix target is absent or non-native');
  }
  if (appendix.abi.targetTriple !== target.triple) fail('P361-TARGET', 'native ABI target triple differs from the source plan');
  if (canonicalBytes(appendix.abi.runtime).compare(canonicalBytes(target.runtime)) !== 0) {
    fail('P361-NATIVE', 'native ABI runtime identity differs from the source plan');
  }
  if (appendix.linking.target !== appendix.target) fail('P361-TARGET', 'link target differs from native target');

  const acquired = appendix.acquisition;
  const libraries = indexById(acquired.targetLibraries, 'target libraries');
  const sysroots = indexById(acquired.sysroots, 'sysroots');
  const staticArtifacts = indexById(acquired.staticArtifacts, 'static artifacts');
  const sharedArtifacts = indexById(acquired.sharedArtifacts, 'shared artifacts');
  const runtimeDependencies = indexById(acquired.runtimeDependencies, 'runtime dependencies');
  const targetBound = [
    ...libraries.values(),
    ...sysroots.values(),
    ...staticArtifacts.values(),
    ...sharedArtifacts.values(),
    ...runtimeDependencies.values(),
  ];
  for (const item of targetBound) {
    if (item.target !== appendix.target) fail('P361-TARGET', `${item.id} has the wrong target`);
  }

  for (const library of libraries.values()) {
    const artifacts = library.linkage === 'static' ? staticArtifacts : sharedArtifacts;
    if (!artifacts.has(library.artifact)) fail('P361-NATIVE', `${library.id} is missing its ${library.linkage} artifact`);
  }
  for (const dependency of runtimeDependencies.values()) {
    if (!sharedArtifacts.has(dependency.sharedArtifact)) {
      fail('P361-NATIVE', `${dependency.id} is missing its shared runtime artifact`);
    }
  }

  const objectIds = new Set();
  orderedUnique(appendix.compilation.steps, (item) => item.id, 'compilation steps');
  for (const step of appendix.compilation.steps) {
    const tool = sourceAuthority.tools.get(step.tool);
    if (!tool || !tool.targets.includes(appendix.target)) fail('P361-TOOLCHAIN', `undeclared compilation tool ${step.tool}`);
    if (step.target !== appendix.target) fail('P361-TARGET', `${step.id} compiles for the wrong target`);
    for (const input of step.inputs) {
      if (input.kind === 'source') {
        if (!sourceAuthority.sourceKeys.has(materialKey(input.graphRole, input.package, input.path))) {
          fail('P361-NATIVE', `missing source compilation input ${input.path}`);
        }
      } else {
        const index = input.kind === 'static' ? staticArtifacts
          : input.kind === 'shared' ? sharedArtifacts : sysroots;
        if (!index.has(input.id)) fail('P361-NATIVE', `missing ${input.kind} compilation input ${input.id}`);
      }
    }
    if (objectIds.has(step.output)) fail('P361-NATIVE', `duplicate object output ${step.output}`);
    objectIds.add(step.output);
  }
  const linker = sourceAuthority.tools.get(appendix.linking.tool);
  if (!linker || !linker.targets.includes(appendix.target)) {
    fail('P361-TOOLCHAIN', `undeclared linker ${appendix.linking.tool}`);
  }

  for (const input of appendix.linking.inputs) {
    const index = input.kind === 'object' ? objectIds
      : input.kind === 'static' ? staticArtifacts
        : input.kind === 'shared' ? sharedArtifacts : sysroots;
    if (!index.has(input.id)) fail('P361-NATIVE', `missing ${input.kind} linker input ${input.id}`);
  }
  if (!sourceAuthority.outputKeys.has(`${appendix.target}\0${appendix.linking.output}`)) {
    fail('P361-NATIVE', 'linker output is not an exact declared target output');
  }
}

export function validateBuildPlan(document, packageAuthority) {
  validateCollectionBudgets(document);
  let planBytes;
  try {
    planBytes = canonicalBytes(document);
  } catch {
    planBytes = Buffer.alloc(0);
  }
  if (planBytes.length > MAX_PLAN_BYTES) fail('P361-BUDGET', 'plan exceeds 262144 bytes');
  if (!validateSchema(document)) fail('P361-SCHEMA', `schema failed at ${validateSchema.errors[0].instancePath || '/'}`);
  const sourceAuthority = validateSourcePlan(document.sourcePlan, packageAuthority);
  if (document.nativeAppendix) validateNativeAppendix(document.nativeAppendix, sourceAuthority);
  if (document.cacheKey !== deriveCacheKey(document)) fail('P361-CACHE', 'cache key does not bind the canonical plan');
  return buildPlanReceipt(document);
}

export function validateBuildPlanBytes(input, packageAuthority) {
  if (!Buffer.isBuffer(input) || input.length > MAX_PLAN_BYTES) fail('P361-BUDGET', 'wire input exceeds its bound');
  let document;
  try {
    document = JSON.parse(new TextDecoder('utf-8', { fatal: true }).decode(input));
  } catch {
    fail('P361-WIRE', 'input is not strict UTF-8 JSON');
  }
  if (!canonicalBytes(document).equals(input)) fail('P361-WIRE', 'input is not canonical JSON');
  return validateBuildPlan(document, packageAuthority);
}

export function verifySourceMaterials(document, materials) {
  if (!(materials instanceof Map) || materials.size !== document.sourcePlan.sources.length) {
    fail('P361-SOURCE', 'source material inventory has missing or extra entries');
  }
  for (const source of document.sourcePlan.sources) {
    const key = materialKey(source.graphRole, source.package, source.path);
    const bytes = materials.get(key);
    if (!Buffer.isBuffer(bytes)) fail('P361-SOURCE', `missing source material ${key}`);
    const actual = createHash('sha256').update(bytes).digest('hex');
    if (bytes.length !== source.size || actual !== source.sha256) fail('P361-SOURCE', `stale source material ${key}`);
  }
  return true;
}

function exactObjectKeys(value, expected) {
  return value !== null && typeof value === 'object' && !Array.isArray(value) &&
    Object.keys(value).sort().join('\0') === [...expected].sort().join('\0');
}

function validateCacheMetadata(entry, outputMaterials) {
  if (!exactObjectKeys(entry, ['key', 'outputs', 'target']) ||
      typeof entry.key !== 'string' || !/^[0-9a-f]{64}$/.test(entry.key) ||
      !['javascript', 'native-linux-x86_64', 'webassembly'].includes(entry.target) ||
      !Array.isArray(entry.outputs) || entry.outputs.length > 16 ||
      !(outputMaterials instanceof Map) || outputMaterials.size !== entry.outputs.length) {
    fail('P361-CACHE', 'cache entry metadata is malformed or incomplete');
  }
  const paths = new Set();
  for (const output of entry.outputs) {
    if (!exactObjectKeys(output, ['path', 'sha256', 'size']) ||
        typeof output.path !== 'string' || output.path.length === 0 || output.path.length > 160 ||
        !/^[a-z0-9][a-z0-9._/-]*$/.test(output.path) || paths.has(output.path) ||
        !Number.isSafeInteger(output.size) || output.size < 0 || output.size > 1073741824 ||
        typeof output.sha256 !== 'string' || !/^[0-9a-f]{64}$/.test(output.sha256)) {
      fail('P361-CACHE', 'cache output metadata is malformed, duplicate, or oversized');
    }
    paths.add(output.path);
  }
  for (const key of outputMaterials.keys()) {
    if (!paths.has(key)) fail('P361-CACHE', 'cache output material inventory has extra entries');
  }
}

export function validateCacheEntry(document, target, entry, outputMaterials) {
  if (!document.sourcePlan.targets.some((candidate) => candidate.id === target)) {
    fail('P361-TARGET', `cache lookup target ${target} is absent`);
  }
  if (entry === undefined) return Object.freeze({ outcome: 'miss' });
  validateCacheMetadata(entry, outputMaterials);
  const expectedKey = deriveTargetCacheKey(document, target);
  if (entry.key !== expectedKey || entry.target !== target) fail('P361-CACHE', 'cache entry identity is incompatible');
  const expectedPaths = document.sourcePlan.outputs.filter((output) => output.target === target).map((output) => output.path);
  const actualPaths = entry.outputs.map((output) => output.path);
  if (expectedPaths.length !== actualPaths.length || expectedPaths.some((value, index) => value !== actualPaths[index])) {
    fail('P361-CACHE', 'cache output inventory is incompatible');
  }
  for (const output of entry.outputs) {
    const bytes = outputMaterials.get(output.path);
    const actual = Buffer.isBuffer(bytes) ? createHash('sha256').update(bytes).digest('hex') : '';
    if (!bytes || bytes.length !== output.size || actual !== output.sha256) fail('P361-CACHE', `stale cached output ${output.path}`);
  }
  return Object.freeze({ outcome: 'hit', key: expectedKey });
}

export function validatePublicationObservation(observation) {
  if (observation.destinationExistedBefore) {
    if (observation.commitAttempted || !observation.destinationVisible || !observation.priorDestinationPreserved) {
      fail('P361-PUBLICATION', 'create-only collision changed or replaced the destination');
    }
    return Object.freeze({ outcome: 'collision-preserved' });
  }
  if (!observation.commitCompleted) {
    if (observation.destinationVisible) fail('P361-PUBLICATION', 'interrupted staging became visible');
    return Object.freeze({ outcome: 'interrupted-hidden' });
  }
  if (!observation.commitAttempted || !observation.destinationVisible || !observation.completeManifest) {
    fail('P361-PUBLICATION', 'published bundle is incomplete');
  }
  return Object.freeze({ outcome: 'published' });
}

export async function loadBuildPlan(root = workspaceRoot) {
  const [documentBytes, packageFixtureBytes] = await Promise.all([
    readFile(path.join(root, 'tests/resolved-build-plan-v0/source-only.json')),
    readFile(path.join(root, 'tests/package-release-v1/valid.json')),
  ]);
  const packageAuthority = validatePackageAuthority(packageFixtureBytes);
  const receipt = validateBuildPlanBytes(documentBytes, packageAuthority);
  return { document: JSON.parse(documentBytes), packageAuthority, receipt };
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  const { document } = await loadBuildPlan();
  process.stdout.write(`${document.format}: ${document.cacheKey}\n`);
}
