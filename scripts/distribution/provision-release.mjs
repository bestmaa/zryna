import { spawnSync } from 'node:child_process';
import {
  lstatSync, mkdirSync, readFileSync, realpathSync, readdirSync,
} from 'node:fs';
import { isAbsolute, join, resolve } from 'node:path';
import { canonicalBounded, sha256 } from '../distribution-release/canonical.mjs';
import { recipeTarget, replaceTokens } from './provision/recipe.mjs';
import { validateSourceBuildReceiptText } from '../distribution-release/validate-source-build-receipt.mjs';

const MAX_BINARY = 256 * 1024 * 1024;
const MAX_OUTPUT = 16 * 1024 * 1024;
const TARGETS = new Set(['x86_64-pc-windows-msvc', 'x86_64-unknown-linux-gnu']);

function reject(message) {
  throw new Error(`D422-PROVISION: ${message}`);
}

function samePath(left, right, platform) {
  const normalize = (value) => platform === 'win32'
    ? resolve(value).toLowerCase() : resolve(value);
  return normalize(left) === normalize(right);
}

function cloneFiles(files) {
  return files.map(({ path, mode, data }) => ({ path, mode, data: Buffer.from(data) }));
}

function productionArchitecture(bytes, source) {
  if (!Buffer.isBuffer(bytes)) reject('source architecture receipt bytes are missing');
  const receipt = validateSourceBuildReceiptText(bytes.toString('utf8'));
  if (receipt.source.repository !== source.repository || receipt.source.commit !== source.commit
    || receipt.source.tree !== source.tree) reject('source architecture identity differs');
  const observation = {
    command: receipt.command,
    inputs: receipt.inputs,
    report: receipt.report,
    toolchain: receipt.toolchain,
  };
  return {
    receipt,
    qualificationBytes: Buffer.from(`${canonicalBounded({
      format: 'zryna.release-qualification-architecture.v1',
      status: 'provisional-candidate',
      productionAdmission: 'forbidden',
      source: { ...receipt.source, ref: 'refs/heads/main' },
      ...observation,
    })}\n`),
  };
}

function toolchains(observed, architectureBytes) {
  const evidence = sha256(architectureBytes);
  return observed.toolchains.map((record) => ({
    name: record.name,
    version: record.version,
    origin: record.origin,
    sha256: record.sha256,
    signatureEvidenceSha256: record.name === 'node'
      ? record.observationEvidenceSha256 : evidence,
  }));
}

function directTool(path, record, system) {
  if (!isAbsolute(path ?? '') || !record || !/^[0-9a-f]{64}$/.test(record.sha256 ?? '')) {
    reject('observed tool identity is missing');
  }
  const metadata = system.inspect(path);
  if (!metadata.isFile() || metadata.isSymbolicLink() || metadata.size !== record.size
    || metadata.size < 1 || metadata.size > 512 * 1024 * 1024
    || !samePath(system.realPath(path), path, system.platform)) {
    reject(`${record.name} executable identity differs`);
  }
  const bytes = system.read(path);
  if (bytes.length !== record.size || sha256(bytes) !== record.sha256) {
    reject(`${record.name} executable bytes differ`);
  }
}

async function defaultDependencies() {
  const [{ acquireQualificationMaterials }, { createQualificationArchiveCapability },
    { observeQualificationTools }, { auditQualificationCompileSources,
      seedQualificationCompileCargoHome },
    { preparePayload }] = await Promise.all([
    import('../distribution-release/acquire-qualification-materials.mjs'),
    import('../distribution-release/qualification-archive-capability.mjs'),
    import('../distribution-release/observe-qualification-tools.mjs'),
    import('../distribution-release/provision-qualification-cargo.mjs'),
    import('./payload.mjs'),
  ]);
  return {
    acquireMaterials: acquireQualificationMaterials,
    auditCompileSources: auditQualificationCompileSources,
    createArchiveCapability: createQualificationArchiveCapability,
    observeTools: observeQualificationTools,
    preparePayload,
    seedCargoHome: seedQualificationCompileCargoHome,
  };
}

export function createProductionProvisioner(injected = {}) {
  const pending = new Map();
  const system = injected.system ?? {
    platform: process.platform,
    inspect: lstatSync,
    realPath: realpathSync.native,
    read: readFileSync,
    list: readdirSync,
    make: mkdirSync,
  };
  const environment = injected.environment ?? process.env;
  const spawn = injected.spawn ?? spawnSync;
  let dependencies;
  const load = async () => {
    dependencies ??= Object.keys(injected).some((name) => [
      'acquireMaterials', 'auditCompileSources', 'createArchiveCapability', 'observeTools',
      'preparePayload', 'seedCargoHome',
    ].includes(name)) ? injected : await defaultDependencies();
    return dependencies;
  };

  async function captureReleaseMaterials({
    recipe, recipeBytes, sourceRoot, source, target, architectureReceipt,
    productionCandidate = false, localReview = false,
  }) {
    const sourceCommit = localReview ? environment.ZRYNA_SOURCE_COMMIT : environment.GITHUB_SHA;
    if (!isAbsolute(sourceRoot ?? '') || !TARGETS.has(target?.triple)
      || target.triple !== environment.ZRYNA_TARGET
      || source?.commit !== sourceCommit
      || source?.ref !== (productionCandidate ? 'refs/heads/main' : 'refs/tags/v0.2.3')
      || !Buffer.isBuffer(recipeBytes)
      || !recipeBytes.equals(Buffer.from(`${canonicalBounded(recipe)}\n`))) {
      reject('exact production capture identity differs');
    }
    recipeTarget(recipe, target.triple);
    const key = `${source.commit}:${target.triple}:${sha256(recipeBytes)}`;
    if (pending.has(key)) reject('production material capture is already pending');
    const architecture = productionArchitecture(architectureReceipt, source);
    const implementation = await load();
    const observed = implementation.observeTools({
      target: target.triple,
      architectureBytes: architecture.qualificationBytes,
      workingRoot: sourceRoot,
      environment,
      spawn,
      nodePath: injected.nodePath,
      system: injected.toolSystem,
    });
    const expectedNative = target.triple === 'x86_64-unknown-linux-gnu'
      ? ['inspector', 'linker', 'xz'] : ['inspector', 'linker'];
    if (observed?.toolchains?.map(({ name }) => name).join('\0') !== 'cargo\0node\0rustc'
      || observed?.nativeTools?.map(({ name }) => name).join('\0') !== expectedNative.join('\0')
      || Object.keys(observed.paths ?? {}).sort().join('\0')
        !== [...['cargo', 'node', 'rustc'], ...expectedNative].sort().join('\0')) {
      reject('exact production tool observations differ');
    }
    const archiveCapability = implementation.createArchiveCapability({
      target: target.triple,
      nativeTools: observed.nativeTools,
      workingRoot: sourceRoot,
      spawn,
      system: injected.archiveSystem,
    });
    const acquired = await implementation.acquireMaterials({
      sourceRoot,
      sourceCommit: source.commit,
      sourceRef: source.ref,
      target: target.triple,
      archiveCapability,
      spawn,
      fetchImpl: injected.fetchImpl,
      adapters: injected.materialAdapters,
    });
    if (!Array.isArray(acquired?.capturedMaterials) || !Array.isArray(acquired?.rustCaptures)
      || acquired.capturedMaterials.some(({ path, mode, data } = {}) => typeof path !== 'string'
        || ![0o644, 0o755].includes(mode) || !Buffer.isBuffer(data) || data.length < 1)
      || acquired.rustCaptures.some(({ identity, archive } = {}) => typeof identity !== 'string'
        || !Buffer.isBuffer(archive) || archive.length < 1)) {
      reject('authenticated material capture differs');
    }
    const captured = cloneFiles(acquired.capturedMaterials);
    pending.set(key, {
      architectureReceipt: Buffer.from(architectureReceipt),
      bootstrapCargoHome: environment.CARGO_HOME,
      captured,
      observed,
      recipe: structuredClone(recipe),
      recipeBytes: Buffer.from(recipeBytes),
      rustCaptures: acquired.rustCaptures.map(({ identity, archive }) => ({
        identity, archive: Buffer.from(archive),
      })),
      source: structuredClone(source),
      sourceRoot,
      target: structuredClone(target),
      toolchains: toolchains(observed, architectureReceipt),
      productionCandidate,
      localReview,
    });
    return cloneFiles(captured);
  }

  async function compileReleaseCli({
    recipe, recipeBytes, sourceRoot, source, target, replica, preparedDistribution,
    productionCandidate = false, localReview = false,
  }) {
    const key = `${source?.commit}:${target?.triple}:${sha256(recipeBytes ?? Buffer.alloc(0))}`;
    const state = pending.get(key);
    if (!state || ![1, 2].includes(replica)
      || environment.ZRYNA_REPLICA !== String(replica)
      || state.productionCandidate !== productionCandidate
      || state.localReview !== localReview
      || canonicalBounded({ recipe, source, target })
        !== canonicalBounded({ recipe: state.recipe, source: state.source, target: state.target })
      || sourceRoot !== state.sourceRoot || !recipeBytes.equals(state.recipeBytes)) {
      reject('compile does not match one authenticated material capture');
    }
    const implementation = await load();
    const expected = implementation.preparePayload({
      version: '0.2.3', source, target,
      recipe: { format: recipe.format, sha256: sha256(recipeBytes) },
    }, cloneFiles(state.captured), state.architectureReceipt,
    { productionCandidate }).preparedDistribution;
    if (canonicalBounded(preparedDistribution) !== canonicalBounded(expected)
      || !/^[0-9a-f]{64}$/.test(preparedDistribution.sha256 ?? '')) {
      reject('prepared distribution differs from authenticated materials');
    }
    pending.delete(key);
    const runnerTemp = environment.RUNNER_TEMP;
    const runId = localReview ? environment.ZRYNA_LOCAL_REPLAY_ID : environment.GITHUB_RUN_ID;
    if (!isAbsolute(runnerTemp ?? '') || !isAbsolute(state.bootstrapCargoHome ?? '')
      || samePath(runnerTemp, sourceRoot, system.platform)
      || samePath(state.bootstrapCargoHome, sourceRoot, system.platform)
      || !/^[1-9][0-9]{0,19}$/.test(runId ?? '')) {
      reject('production compile roots differ');
    }
    const workRoot = join(runnerTemp,
      `zryna-production-${target.triple}-${runId}-${replica}`);
    const seeded = implementation.seedCargoHome({
      bootstrapCargoHome: state.bootstrapCargoHome,
      workRoot,
      rustCaptures: state.rustCaptures,
    });
    if (seeded.workRoot !== workRoot || !isAbsolute(seeded.cargoHome ?? '')) {
      reject('production Cargo seed identity differs');
    }
    const selected = recipeTarget(recipe, target.triple);
    const records = new Map([...state.observed.toolchains, ...state.observed.nativeTools]
      .map((record) => [record.name, record]));
    for (const [name, path] of Object.entries(state.observed.paths)) {
      directTool(path, records.get(name), system);
    }
    const targetRoot = join(workRoot, 'target');
    const tempRoot = join(workRoot, 'temp');
    if (system.list(workRoot).sort().join('\0') !== 'cargo-home') {
      reject('production work root contains unexpected entries');
    }
    system.make(targetRoot, { recursive: false, mode: 0o700 });
    system.make(tempRoot, { recursive: false, mode: 0o700 });
    const replacements = {
      '@cargo-home@': seeded.cargoHome,
      '@linker@': state.observed.paths.linker,
      '@qualification-binding-sha256@': preparedDistribution.sha256,
      '@rustc@': state.observed.paths.rustc,
      '@source-date-epoch@': String(source.sourceDateEpoch),
      '@source-root@': sourceRoot,
      '@target@': target.triple,
      '@target-root@': targetRoot,
      '@temp-root@': tempRoot,
      '@work-root@': workRoot,
      ...Object.fromEntries(Object.entries(state.observed.hostEnvironment ?? {})
        .map(([name, value]) => [`@host-${name}@`, value])),
    };
    const compileEnvironment = Object.fromEntries(selected.environment
      .map(({ name, value }) => [name, replaceTokens(value, replacements)]));
    compileEnvironment.CARGO_ENCODED_RUSTFLAGS = selected.encodedRustFlags
      .map((value) => replaceTokens(value, replacements)).join('\x1f');
    if (compileEnvironment.CARGO_NET_OFFLINE !== 'true') {
      reject('production compile must remain offline');
    }
    const argv = recipe.compile.argv.map((value) => replaceTokens(value, replacements));
    const result = spawn(state.observed.paths.cargo, argv.slice(1), {
      cwd: sourceRoot,
      env: compileEnvironment,
      encoding: null,
      maxBuffer: MAX_OUTPUT + 1,
      timeout: 45 * 60_000,
      windowsHide: true,
      shell: false,
    });
    for (const [name, path] of Object.entries(state.observed.paths)) {
      directTool(path, records.get(name), system);
    }
    if (result.error || result.status !== 0 || result.signal !== null
      || !Buffer.isBuffer(result.stdout) || !Buffer.isBuffer(result.stderr)
      || result.stdout.length + result.stderr.length > MAX_OUTPUT) {
      reject(`Cargo production compile failed with status ${result.status ?? 'unknown'}`);
    }
    implementation.auditCompileSources(seeded.cargoHome, state.rustCaptures);
    const executable = join(targetRoot, target.triple, 'release',
      target.triple === 'x86_64-pc-windows-msvc' ? 'zryna.exe' : 'zryna');
    const metadata = system.inspect(executable);
    if (!metadata.isFile() || metadata.isSymbolicLink() || metadata.size < 64
      || metadata.size > MAX_BINARY || !samePath(system.realPath(executable), executable,
        system.platform)) reject('compiled CLI file identity differs');
    const cli = system.read(executable);
    if (cli.length !== metadata.size) reject('compiled CLI bytes differ');
    return { cli, toolchains: state.toolchains };
  }

  return Object.freeze({ captureReleaseMaterials, compileReleaseCli });
}

const production = createProductionProvisioner();
export const captureReleaseMaterials = production.captureReleaseMaterials;
export const compileReleaseCli = production.compileReleaseCli;
