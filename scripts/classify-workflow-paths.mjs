import { spawnSync } from 'node:child_process';
import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const SCRIPT_PATH = fileURLToPath(import.meta.url);

export const CONTRACT_LANES = Object.freeze([
  'diagnostics',
  'distribution_release',
  'package_release',
  'provider_v4',
  'wit',
]);

const ALL_LANES_PATHS = Object.freeze([
  /^crates\/zryna-process\//,
  /^(?:Cargo\.lock|Cargo\.toml|package\.json|pnpm-lock\.yaml|pnpm-workspace\.yaml|rust-toolchain\.toml|zryna\.workspace\.json)$/,
  /^\.github\/(?:actions|workflows)\//,
  /^scripts\/classify-workflow-paths\.mjs$/,
  /^scripts\/repository-structure-policy\.json$/,
  /^tests\/workflow-routing\.test\.mjs$/,
  /^toolchains\//,
]);

const KNOWN_UNRELATED_PATHS = Object.freeze([
  /^(?:apps|crates|docs|editors|examples|runtime)\//,
  /^(?:\.gitattributes|\.gitignore|CODE_OF_CONDUCT\.md|CONTRIBUTING\.md|LICENSE|NOTICE|README\.md|SECURITY\.md)$/,
]);

const LANE_PATHS = Object.freeze({
  diagnostics: Object.freeze([
    /^crates\/zryna-(?:diagnostics|source)\//,
    /^schemas\/zryna-diagnostics-v2\.schema\.json$/,
    /^spec\/diagnostics\//,
    /^tests\/diagnostics-protocol-v2\.test\.mjs$/,
  ]),
  distribution_release: Object.freeze([
    /^scripts\/portable-setup\//,
    /^scripts\/package-editor\.mjs$/,
    /^editors\/vscode-zryna\//,
    /^apps\/zryna-language-server\//,
    /^crates\/zryna-driver\/src\/diagnostic_sessions(?:\.rs|\/)/,
    /^docs\/PORTABLE_SETUP\.md$/,
    /^tests\/portable-vsix\.test\.mjs$/,
    /^schemas\/zryna-distribution(?:-(?:files|materials|inventory))?-v1\.schema\.json$/,
    /^scripts\/distribution\//,
    /^crates\/zryna-driver\/src\/distribution\//,
    /^apps\/zryna\/src\/installed\.rs$/,
    /^tests\/distribution-(?:archive|inventory|source-receipt|receipt-compatibility|schema|payload|npm-materials)\.test\.mjs$/,
    /^schemas\/zryna-(?:distribution-(?:release|build-input)|release-(?:build-result|reproduction|tag-receipt|qualification-(?:source|architecture|gates|input|inspection|result))|source-build-receipt|preassembly-gates)-v1\.schema\.json$/,
    /^scripts\/distribution-release\//,
    /^spec\/release\//,
    /^tests\/(?:(?:distribution-(?:release|build-input)|source-build-receipt|preassembly-gates|release-tag-receipt)-v1|distribution-release-producers|protected-release-build|release-(?:evidence|reproduction|workflow|qualification-(?:source|workflow|acquisition|archive-capability|cargo|input-producer|compile|inspection|core|comparison|result)))\.test\.mjs$/,
  ]),
  package_release: Object.freeze([
    /^crates\/zryna-package\//,
    /^crates\/zryna-driver\/src\/package_resolution(?:\.rs|_tests?\.rs|\/)/,
    /^apps\/zryna\/src\/package\.rs$/,
    /^docs\/PACKAGE_RESOLUTION\.md$/,
    /^schemas\/zryna-package-release-v1\.schema\.json$/,
    /^scripts\/package-release\//,
    /^spec\/package\//,
    /^tests\/package-release-v1\//,
    /^tests\/package-source-trust\.test\.mjs$/,
  ]),
  provider_v4: Object.freeze([
    /^adapters\/typescript-6\//,
    /^crates\/zryna-(?:diagnostics|frontend|source|syntax)\//,
    /^schemas\/zryna-syntax-v4\.schema\.json$/,
    /^scripts\/check-provider-conformance-v4\.mjs$/,
    /^scripts\/native-lexer-provider-witness\.mjs$/,
    /^scripts\/run-native-lexer-provider-differential\.mjs$/,
    /^scripts\/run-native-parser-provider-differential\.mjs$/,
    /^scripts\/run-native-lexer-resource-tests\.mjs$/,
    /^scripts\/verify-provider-v4-ci-result\.mjs$/,
    /^tests\/provider-conformance-v4(?:\.json|\.test\.mjs|\/.*)$/,
    /^tests\/native-lexer-resource-runner\.test\.mjs$/,
    /^tests\/native-lexer-provider-runner\.test\.mjs$/,
    /^tests\/native-parser-provider-runner\.test\.mjs$/,
    /^tests\/provider-v4-ci-result\.test\.mjs$/,
    /^tests\/syntax-protocol-v4\.test\.mjs$/,
  ]),
  wit: Object.freeze([
    /^spec\/abi\/scalar-v1-fixtures\.json$/,
    /^spec\/language\/CROSS_TARGET_PROFILES_V1\.md$/,
    /^spec\/libraries\/MINIMAL_CORE_HOST_V0\.md$/,
    /^schemas\/zryna-(?:capability-request|wit-capability-profiles)-v1\.schema\.json$/,
    /^scripts\/wit-capabilities\//,
    /^spec\/interop\/JS_WASM_ADAPTERS_V1\.md$/,
    /^spec\/interop\/JS_WASM_ADAPTER_CONFORMANCE_V1\.md$/,
    /^spec\/wit\//,
    /^tests\/js-wasm-adapter-contract\.test\.mjs$/,
    /^tests\/js-wasm-adapter-v1-vectors\.json$/,
    /^tests\/wit-capability-contract\.test\.mjs$/,
    /^tests\/wit-capability-profiles-v1\.json$/,
    /^tests\/wit-capability-v1\//,
  ]),
});

function everyLane(value) {
  return Object.fromEntries(CONTRACT_LANES.map((lane) => [lane, value]));
}

function isPortablePath(candidate) {
  if (typeof candidate !== 'string' || candidate.length === 0 || candidate.includes('\\')) {
    return false;
  }
  if (candidate.startsWith('/') || candidate.includes('\0') || /[\r\n]/.test(candidate)) {
    return false;
  }
  const segments = candidate.split('/');
  return segments.every((segment) => segment.length > 0 && segment !== '.' && segment !== '..');
}

export function classifyWorkflowPaths(paths, { full = false } = {}) {
  if (full) return everyLane(true);
  const result = everyLane(false);
  for (const changedPath of paths) {
    // Unexpected Git path forms run every optional lane instead of risking a false negative.
    if (!isPortablePath(changedPath) || ALL_LANES_PATHS.some((pattern) => pattern.test(changedPath))) {
      return everyLane(true);
    }
    let matchedLane = false;
    for (const lane of CONTRACT_LANES) {
      if (LANE_PATHS[lane].some((pattern) => pattern.test(changedPath))) {
        result[lane] = true;
        matchedLane = true;
      }
    }
    // Other crates cannot enter these contract graphs without a captured member or root
    // manifest change. New scripts, tests, schemas, and normative specs are intentionally
    // absent from this allowlist, so an unclassified helper defaults to every lane.
    if (!matchedLane && !KNOWN_UNRELATED_PATHS.some((pattern) => pattern.test(changedPath))) {
      return everyLane(true);
    }
  }
  return result;
}

export function formatWorkflowOutputs(classification) {
  return `${CONTRACT_LANES.map((lane) => `${lane}=${classification[lane] ? 'true' : 'false'}`).join('\n')}\n`;
}

function changedPathsFromBuffer(input) {
  if (input.length === 0) return [];
  if (input.at(-1) !== 0) throw new Error('changed-path input must be NUL terminated');
  const text = new TextDecoder('utf-8', { fatal: true }).decode(input.subarray(0, -1));
  return text.length === 0 ? [] : text.split('\0');
}

export function classifyGitDiff(base, head, spawn = spawnSync) {
  if (![base, head].every((revision) => /^[0-9a-f]{40}$/.test(revision))) return everyLane(true);
  const result = spawn('git', [
    'diff', '--name-only', '--no-renames', '-z', base, head,
  ], {
    encoding: null,
    maxBuffer: 2 * 1024 * 1024,
    shell: false,
    windowsHide: true,
  });
  if (result.error || result.status !== 0 || !Buffer.isBuffer(result.stdout)) return everyLane(true);
  try {
    return classifyWorkflowPaths(changedPathsFromBuffer(result.stdout));
  } catch {
    return everyLane(true);
  }
}

if (process.argv[1] && resolve(process.argv[1]) === SCRIPT_PATH) {
  try {
    const args = process.argv.slice(2);
    const full = args.length === 1 && args[0] === '--all';
    const gitDiff = args.length === 3 && args[0] === '--git-diff';
    if (!full && !gitDiff && args.length !== 0) {
      throw new Error('usage: node scripts/classify-workflow-paths.mjs [--all | --git-diff <base> <head>]');
    }
    const classification = full
      ? classifyWorkflowPaths([], { full: true })
      : gitDiff
        ? classifyGitDiff(args[1], args[2])
        : classifyWorkflowPaths(changedPathsFromBuffer(readFileSync(0)));
    process.stdout.write(formatWorkflowOutputs(classification));
  } catch (error) {
    console.error(error instanceof Error ? error.message : String(error));
    process.exitCode = 1;
  }
}
