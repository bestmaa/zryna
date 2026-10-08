import { resolve, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';
import { INITIAL_COMMIT, MODULE_COMMIT, MODULE_LIMIT, POLICY_PATH, fail, physicalLines, reviewChanges, validatePolicy } from './structure/policy.mjs';
import { blobs, commit, git, legacySource, navigation, readSafe, renames, source, tree, workingPaths } from './structure/repository.mjs';
import { history } from './structure/history.mjs';
import { validateUnsafeRustWorkspace } from './structure/unsafe-rust.mjs';
import { moduleLines } from './structure/module-size.mjs';
import { moduleHistory } from './structure/module-history.mjs';

export function checkRepository({ root, base = process.env.ZRYNA_STRUCTURE_BASE,
  today = new Date().toISOString().slice(0, 10), bootstrap = INITIAL_COMMIT,
  moduleBootstrap = MODULE_COMMIT } = {}) {
  if (!root) fail('repository root is required');
  if (!base && process.env.CI) fail('CI must supply ZRYNA_STRUCTURE_BASE as a full trusted commit SHA');
  const comparison = commit(root, base ?? 'HEAD');
  const head = commit(root, 'HEAD');
  git(root, ['merge-base', '--is-ancestor', comparison, head]);
  const policyText = readSafe(root, POLICY_PATH);
  const policy = validatePolicy(JSON.parse(policyText), today);
  const beforePolicy = tree(root, comparison);
  const trustedPolicyEntry = beforePolicy.get(POLICY_PATH);
  let trustedPolicy;
  if (trustedPolicyEntry) {
    if (trustedPolicyEntry.mode !== '100644') fail('trusted policy is not a regular file');
    trustedPolicy = validatePolicy(JSON.parse(blobs(root, [trustedPolicyEntry]).get(trustedPolicyEntry.hash)), '0000-00-00');
  }
  if (policy.anchor !== (trustedPolicy?.anchor ?? bootstrap)) fail('anchor differs from trusted policy/bootstrap commit');
  const { origin, ratchetBase, before, historical, ceilings: baseline } =
    history(root, comparison, head, policy, trustedPolicy);
  const modules = moduleHistory(root, ratchetBase, head, policy, trustedPolicy, moduleBootstrap);
  const messages = [];
  if (!trustedPolicy || policyText !== blobs(root, [trustedPolicyEntry]).get(trustedPolicyEntry.hash)) {
    messages.push('REVIEW policy changed: classifications, baseline and exceptions require explicit maintainer review; this check does not prove approval');
    messages.push(...reviewChanges(trustedPolicy, policy));
  }
  const paths = workingPaths(root);
  validateUnsafeRustWorkspace(root, paths);
  const classifications = new Map(policy.classifications.map(entry => [entry.path, entry]));
  const current = new Map();
  const normalized = new Map();
  for (const path of paths.filter(source)) {
    const text = readSafe(root, path, true);
    if (text !== undefined) {
      current.set(path, physicalLines(text));
      if (!classifications.has(path)) normalized.set(path, moduleLines(path, text));
    }
  }
  const oldClasses = new Map((trustedPolicy?.classifications ?? policy.classifications).map(entry => [entry.path, entry]));
  const exceptions = new Map(policy.exceptions.map(entry => [entry.path, entry]));
  for (const entry of [...classifications.values(), ...exceptions.values()]) {
    if (!current.has(entry.path)) fail(`stale policy path: ${entry.path}; remove deleted or renamed records explicitly`);
  }
  for (const path of exceptions.keys()) {
    if (classifications.has(path) || current.get(path) <= 500) fail(`stale exception for non-production or ordinary-sized file: ${path}`);
  }
  const historicalRenames = renames(root, origin, ratchetBase);
  const currentRenames = renames(root, ratchetBase);
  const errors = [];
  for (const [path, lines] of current) {
    if (classifications.has(path)) continue;
    const previousPath = currentRenames.get(path) ?? path;
    const originalPath = historicalRenames.get(previousPath) ?? previousPath;
    const previous = before.get(previousPath);
    const previousLines = previous && historical.has(previous.hash)
      ? physicalLines(historical.get(previous.hash)) : undefined;
    const ordinaryCeiling = (
      baseline.has(originalPath) && previousLines > 500 && !oldClasses.has(previousPath)
        ? Math.min(baseline.get(originalPath), previousLines) : 500);
    if (exceptions.has(path) && lines <= ordinaryCeiling) fail(`stale unnecessary exception: ${path}`);
    const ceiling = exceptions.get(path)?.ceiling ?? ordinaryCeiling;
    if (legacySource(path) && lines > ceiling) errors.push(`ERROR ${path}: ${lines} lines exceeds ${ceiling}; split by cohesive responsibility into modules of at most ${MODULE_LIMIT} lines`);
    const modulePath = modules.renames.get(previousPath) ?? previousPath;
    const previousSize = previous && historical.has(previous.hash)
      ? moduleLines(previousPath, historical.get(previous.hash)) : undefined;
    const moduleCeiling = modules.ceilings.has(modulePath) && previousSize > MODULE_LIMIT && !oldClasses.has(previousPath)
      ? Math.min(modules.ceilings.get(modulePath), previousSize) : MODULE_LIMIT;
    const actual = normalized.get(path);
    if (actual > moduleCeiling) errors.push(`ERROR ${path}: ${actual} module lines (${lines} physical) exceeds ${moduleCeiling}; split by cohesive responsibility into private modules of at most ${MODULE_LIMIT} lines`);
  }
  navigation(root);
  messages.push(...errors);
  return { ok: errors.length === 0, messages, comparison, files: current.size };
}

const script = fileURLToPath(import.meta.url);
if (process.argv[1] && resolve(process.argv[1]) === script) {
  try {
    if (process.argv.length !== 2) fail('no positional options; supply trusted base using ZRYNA_STRUCTURE_BASE');
    const result = checkRepository({ root: resolve(dirname(script), '..') });
    for (const message of result.messages) console.log(message);
    console.log(`Structure checked ${result.files} source files against ${result.comparison}; architecture and compiler gates remain required.`);
    if (!result.ok) process.exitCode = 1;
  } catch (error) {
    console.error(error.message);
    process.exitCode = 1;
  }
}
