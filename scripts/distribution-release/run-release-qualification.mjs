import { spawnSync } from 'node:child_process';
import { isAbsolute, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { acquireQualificationMaterials } from './acquire-qualification-materials.mjs';
import { canonicalBounded } from './canonical.mjs';
import { compileReleaseQualification } from './compile-release-qualification.mjs';
import { createReleaseQualificationInput } from './create-release-qualification-input.mjs';
import { createQualificationArchitectureReceipt } from './create-source-build-receipt.mjs';
import { inspectReleaseQualification } from './inspect-release-qualification.mjs';
import { observeQualificationTools } from './observe-qualification-tools.mjs';
import {
  auditQualificationCompileSources, provisionQualificationCargoHome,
  seedQualificationCompileCargoHome,
} from './provision-qualification-cargo.mjs';
import { createQualificationArchiveCapability } from './qualification-archive-capability.mjs';
import { assembleQualification, prepareQualification } from './release-qualification-core.mjs';
import { exactReleaseNames, MAX_RELEASE_DOCUMENT, readReleaseFile } from './release-files.mjs';
import { validateReleaseQualificationSourceText } from './validate-release-qualification-source.mjs';
import { writeReleaseQualificationResult } from './write-release-qualification-result.mjs';

const SCRIPT_PATH = fileURLToPath(import.meta.url);
const RECIPE_PATH = 'scripts/distribution/release-recipe-v1.json';
const MAX_GIT_OUTPUT = 16 * 1024 * 1024;
const TARGETS = new Set(['x86_64-unknown-linux-gnu', 'x86_64-pc-windows-msvc']);

function reject(message) {
  throw new Error(`R406-QUALIFICATION-RUNNER: ${message}`);
}

function runGit(spawn, args, cwd) {
  const result = spawn('git', args, { cwd, encoding: null, maxBuffer: MAX_GIT_OUTPUT + 1,
    shell: false, windowsHide: true });
  if (result.error || result.status !== 0 || !Buffer.isBuffer(result.stdout)
    || result.stdout.length > MAX_GIT_OUTPUT) reject(`git ${args[0]} failed`);
  return result.stdout;
}

function gitBlob(spawn, sourceRoot, commit, path) {
  const listing = runGit(spawn, ['ls-tree', '-z', '--full-tree', commit, '--', path], sourceRoot);
  const suffix = Buffer.from(`\t${path}\0`);
  if (!listing.subarray(-suffix.length).equals(suffix) || listing.indexOf(0) !== listing.length - 1) {
    reject('recipe tree entry is missing or ambiguous');
  }
  const match = /^(100644) (blob) ([0-9a-f]{40})$/
    .exec(listing.subarray(0, listing.length - suffix.length).toString('ascii'));
  if (!match) reject('recipe must be one ordinary Git blob');
  return runGit(spawn, ['cat-file', 'blob', match[3]], sourceRoot);
}

export async function runReleaseQualification({
  admissionRoot, outputRoot, sourceRoot, workRoot, target, replica,
  environment = process.env, spawn = spawnSync,
}) {
  const roots = [admissionRoot, outputRoot, sourceRoot, workRoot];
  if (!roots.every((path) => isAbsolute(path ?? '') && resolve(path) === path)
    || new Set(roots).size !== roots.length || !TARGETS.has(target) || ![1, 2].includes(replica)
    || environment.GITHUB_EVENT_NAME !== 'workflow_dispatch'
    || environment.GITHUB_REF !== 'refs/heads/main'
    || environment.GITHUB_SHA !== environment.GITHUB_WORKFLOW_SHA
    || environment.ZRYNA_TARGET !== target || Number(environment.ZRYNA_REPLICA) !== replica) {
    reject('exact qualification workflow identity differs');
  }
  exactReleaseNames(admissionRoot, ['qualification-gates.json', 'qualification-source.json']);
  const sourceBytes = readReleaseFile(admissionRoot, 'qualification-source.json', MAX_RELEASE_DOCUMENT);
  const gateBytes = readReleaseFile(admissionRoot, 'qualification-gates.json', MAX_RELEASE_DOCUMENT);
  const source = validateReleaseQualificationSourceText(sourceBytes.toString('utf8'));
  if (source.source.commit !== environment.GITHUB_SHA) reject('qualification source differs');

  const boundEnvironment = { ...environment,
    SOURCE_DATE_EPOCH: String(source.source.sourceDateEpoch) };
  const bootstrapWorkRoot = `${workRoot}-bootstrap`;
  if (roots.includes(bootstrapWorkRoot)) reject('bootstrap qualification root overlaps an input root');
  const provisioned = provisionQualificationCargoHome({ sourceRoot, workRoot: bootstrapWorkRoot, target,
    environment: boundEnvironment, spawn });
  const receiptEnvironment = { ...boundEnvironment, CARGO_HOME: provisioned.cargoHome };
  const architectureBytes = Buffer.from(`${canonicalBounded(createQualificationArchitectureReceipt({
    environment: receiptEnvironment, cwd: sourceRoot, spawn,
  }))}\n`);
  const tools = observeQualificationTools({ target, architectureBytes, workingRoot: sourceRoot,
    environment: boundEnvironment, spawn });
  const archiveCapability = createQualificationArchiveCapability({ target,
    nativeTools: tools.nativeTools, workingRoot: sourceRoot, spawn });
  const acquired = await acquireQualificationMaterials({ sourceRoot,
    sourceCommit: source.source.commit, sourceRef: source.source.ref, target, archiveCapability, spawn });
  const compileHome = seedQualificationCompileCargoHome({
    bootstrapCargoHome: provisioned.cargoHome, workRoot, rustCaptures: acquired.rustCaptures,
  });
  const recipeBytes = gitBlob(spawn, sourceRoot, source.source.commit, RECIPE_PATH);
  const options = { sourceBytes, architectureBytes, gateBytes, recipeBytes, target,
    capturedMaterials: acquired.capturedMaterials, tools };
  const input = createReleaseQualificationInput(options);
  const binding = Buffer.from(`${canonicalBounded(input)}\n`);
  const prepared = prepareQualification(input, acquired.capturedMaterials,
    { source: sourceBytes, architecture: architectureBytes, gates: gateBytes });
  const compiled = compileReleaseQualification({ binding, sourceRoot, workRoot,
    cargoHome: compileHome.cargoHome, observedTools: tools, spawn });
  auditQualificationCompileSources(compileHome.cargoHome, acquired.rustCaptures);
  const assembled = await assembleQualification(binding,
    { payload: prepared.payload, cli: compiled.cli });
  const inspection = inspectReleaseQualification({ binding, cli: compiled.cli, sourceRoot, workRoot,
    observedTools: { ...tools, executable: compiled.executable }, spawn });
  return writeReleaseQualificationResult({ outputRoot, replica, binding, cli: compiled.cli,
    assembled, inspection });
}

if (process.argv[1] && resolve(process.argv[1]) === SCRIPT_PATH) {
  try {
    if (process.argv.length !== 6 || process.argv[2] !== '--admission'
      || process.argv[4] !== '--output') reject('expected --admission and --output once');
    await runReleaseQualification({ admissionRoot: resolve(process.argv[3]),
      outputRoot: resolve(process.argv[5]), sourceRoot: resolve(process.env.ZRYNA_SOURCE_ROOT ?? ''),
      workRoot: resolve(process.env.ZRYNA_QUALIFICATION_WORK_ROOT ?? ''),
      target: process.env.ZRYNA_TARGET, replica: Number(process.env.ZRYNA_REPLICA) });
  } catch (error) {
    console.error(error instanceof Error ? error.message : String(error));
    process.exitCode = 1;
  }
}
