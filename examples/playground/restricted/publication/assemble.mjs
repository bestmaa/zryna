import { isAbsolute, relative, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { bytes, sha256 } from '../../../../scripts/distribution/canonical.mjs';
import { decodeJson } from '../json.mjs';
import { exact, fail, isHash } from '../limits.mjs';
import { assembleToolkit } from '../producer.mjs';
import { toolkitRoot } from '../toolkit-schema.mjs';
import { captureBuildFiles, captureBuildInput } from './capture.mjs';
import { captureProtectedSource } from './source.mjs';
import { retainOutput } from './output.mjs';

export async function assembleProtectedToolkit(planBytes, reviewed, materialsRoot, sourceRoot, environment) {
  exact(reviewed, ['version', 'planSha256', 'sourceCommit', 'sourceTree', 'nestedCompiler', 'requiredGates']);
  if (!(planBytes instanceof Uint8Array) || planBytes.length < 1 || planBytes.length > 262144 ||
      reviewed.version !== 1 || !isHash(reviewed.planSha256) || sha256(planBytes) !== reviewed.planSha256) {
    fail('BUILD-POLICY');
  }
  const plan = decodeJson(planBytes, 262144);
  exact(plan, ['format', 'version', 'source', 'files', 'materials', 'nestedCompiler', 'gates']);
  if (plan.format !== 'zryna.playground-build.v1' || plan.version !== 1) fail('BUILD-PLAN');
  const sourceReceipt = captureProtectedSource(sourceRoot,
    { commit: reviewed.sourceCommit, tree: reviewed.sourceTree }, environment);
  if (!bytes(plan.source).equals(bytes(sourceReceipt.source))) fail('BUILD-SOURCE');
  const files = captureBuildFiles(materialsRoot, plan.files);
  const result = await assembleToolkit({ source: sourceReceipt.source, files,
    materials: plan.materials, nestedCompiler: plan.nestedCompiler, gates: plan.gates },
  { sourceCommit: reviewed.sourceCommit, sourceTree: reviewed.sourceTree,
    nestedCompiler: reviewed.nestedCompiler, requiredGates: reviewed.requiredGates });
  // Detect source substitution during capture/compression before creating an output directory.
  if (!bytes(sourceReceipt).equals(bytes(captureProtectedSource(sourceRoot,
    { commit: reviewed.sourceCommit, tree: reviewed.sourceTree }, environment)))) fail('BUILD-SOURCE');
  return { ...result, sourceReceipt };
}

export function retainUnsignedAssembly(root, result, sourceRoot, beforeWrite) {
  const outputs = [[`${toolkitRoot}.tar.gz`, result.archive], ['toolkit-envelope.json', result.envelope],
    ['candidate-policy.json', bytes(result.candidatePolicy)], ['source-receipt.json', bytes(result.sourceReceipt)]];
  // Written last: a partial directory remains review evidence, never a complete candidate.
  const completion = bytes({ format: 'zryna.playground-assembly.v1', version: 1,
    status: 'unsigned-review-candidate', source: result.sourceReceipt.source,
    files: outputs.map(([path, data]) => ({ path, bytes: data.length, sha256: sha256(data) })) });
  retainOutput(root, sourceRoot, outputs, completion, beforeWrite);
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  try {
    const args = process.argv.slice(2);
    if (args.length !== 14 || args[0] !== '--materials' || args[2] !== '--source' ||
        args[4] !== '--plan' || args[6] !== '--plan-bytes' || args[8] !== '--plan-sha256' ||
        args[10] !== '--reviewed' || args[12] !== '--output' ||
        !isAbsolute(args[1]) || !isAbsolute(args[3]) || !isAbsolute(args[13]) ||
        !/^[1-9][0-9]*$/.test(args[7]) || Number(args[7]) > 262144) fail('BUILD-ARGUMENTS');
    const planBytes = captureBuildInput(args[1], args[5], Number(args[7]), args[9]);
    // The independent policy descriptor is provided separately from the computed candidate.
    const descriptor = args[11].split(':');
    if (descriptor.length !== 3 || !/^[1-9][0-9]*$/.test(descriptor[1]) ||
        Number(descriptor[1]) > 262144) fail('BUILD-ARGUMENTS');
    const reviewed = decodeJson(captureBuildInput(args[1], descriptor[0], Number(descriptor[1]), descriptor[2]), 262144);
    const outputFromSource = relative(resolve(args[3]), resolve(args[13]));
    if (outputFromSource === '' || !outputFromSource.startsWith('../')) fail('BUILD-OUTPUT');
    const result = await assembleProtectedToolkit(planBytes, reviewed, args[1], args[3], process.env);
    retainUnsignedAssembly(args[13], result, args[3]);
  } catch (error) {
    console.error(error instanceof Error ? error.message : String(error));
    process.exitCode = 1;
  }
}
