import { validateProductionRecipeIdentity } from '../../distribution-release/validate-production-recipe.mjs';

const COMMAND = Object.freeze([
  'cargo', 'rustc', '--locked', '--release', '--target', '@target@',
  '-p', 'zryna', '--bin', 'zryna',
]);

function reject(message) {
  throw new Error(`D422-PROVISION: ${message}`);
}

function exactKeys(value, keys, label) {
  if (value === null || typeof value !== 'object' || Array.isArray(value)
    || Object.keys(value).sort().join('\0') !== [...keys].sort().join('\0')) {
    reject(`${label} fields differ`);
  }
}

export function recipeTarget(recipe, target) {
  validateProductionRecipeIdentity(recipe, reject);
  exactKeys(recipe, ['compile', 'format', 'productionAdmission', 'status', 'versionCandidate'],
    'recipe');
  exactKeys(recipe.compile, ['argv', 'targets'], 'recipe compile');
  exactKeys(recipe.compile.targets,
    ['x86_64-pc-windows-msvc', 'x86_64-unknown-linux-gnu'], 'recipe targets');
  if (JSON.stringify(recipe.compile.argv) !== JSON.stringify(COMMAND)) {
    reject('recipe compile command differs');
  }
  const selected = recipe.compile.targets[target];
  exactKeys(selected, ['encodedLinkerFlags', 'encodedRustFlags', 'environment'],
    'recipe target');
  if (![selected.encodedLinkerFlags, selected.encodedRustFlags, selected.environment]
    .every(Array.isArray)) reject('recipe target declarations differ');
  const names = selected.environment.map((entry) => entry?.name);
  if (selected.environment.some((entry) => entry === null || typeof entry !== 'object'
      || Object.keys(entry).sort().join('\0') !== 'name\0value'
      || !/^[A-Za-z][A-Za-z0-9_]{0,63}$/.test(entry.name)
      || typeof entry.value !== 'string' || /[\r\n\0]/.test(entry.value))
    || new Set(names).size !== names.length
    || selected.encodedRustFlags.some((value) => typeof value !== 'string')
    || selected.encodedLinkerFlags.some((value) => typeof value !== 'string')) {
    reject('recipe compile environment or flags differ');
  }
  const distributionMarker = selected.environment.filter(
    ({ name }) => name === 'ZRYNA_DISTRIBUTION_SHA256',
  );
  if (distributionMarker.length !== 1
    || distributionMarker[0].value !== '@qualification-binding-sha256@'
    || selected.encodedLinkerFlags.some(
      (value) => !selected.encodedRustFlags.includes(`-Clink-arg=${value}`),
    )) reject('recipe production identity or linker flags differ');
  return selected;
}

export function replaceTokens(value, replacements) {
  let result = value;
  for (const [token, replacement] of Object.entries(replacements)) {
    result = result.replaceAll(token, replacement);
  }
  if (/@[A-Za-z0-9_-]+@/.test(result)) reject('compile token is unresolved');
  return result;
}

