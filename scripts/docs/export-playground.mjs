import { isAbsolute, relative, resolve, sep } from 'node:path';
import { fileURLToPath } from 'node:url';
import { bytes } from '../distribution/canonical.mjs';
import { exportDocsBundle } from './bundle.mjs';
import { capturePlaygroundDocsSource, PLAYGROUND_CHANNEL, PLAYGROUND_REF,
  revalidatePlaygroundDocsSource } from './provenance.mjs';
import { retainProtectedDocsOutput } from './protected-output.mjs';

export async function exportPlaygroundDocs({ workspaceRoot, output, evidenceOutput, sourceCommit,
  sourceTree, environment = process.env }) {
  const separated = (left, right) => relative(resolve(left), resolve(right)).startsWith(`..${sep}`);
  if (![workspaceRoot, output, evidenceOutput].every(isAbsolute) ||
      !separated(output, evidenceOutput) || !separated(evidenceOutput, output)) {
    throw new Error('protected documentation requires separate absolute source/output/evidence directories');
  }
  const selection = capturePlaygroundDocsSource(workspaceRoot, { commit: sourceCommit, tree: sourceTree }, environment);
  const result = await exportDocsBundle({ workspaceRoot, output, channel: PLAYGROUND_CHANNEL,
    sourceCommit, sourceRef: PLAYGROUND_REF, protectedSource: selection,
    verifyGit: true, enforceWorkspaceOutput: false });
  const receipt = revalidatePlaygroundDocsSource(selection, workspaceRoot, sourceCommit, PLAYGROUND_REF);
  retainProtectedDocsOutput(evidenceOutput, workspaceRoot, [['source-receipt.json', bytes(receipt)]]);
  return { ...result, sourceReceipt: receipt };
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  try {
    const args = process.argv.slice(2);
    const names = ['--source', '--source-commit', '--source-tree', '--output', '--evidence-output'];
    if (args.length !== names.length * 2 || names.some((name, index) => args[index * 2] !== name)) {
      throw new Error('expected fixed --source, --source-commit, --source-tree, --output and --evidence-output arguments');
    }
    const result = await exportPlaygroundDocs({ workspaceRoot: args[1], sourceCommit: args[3],
      sourceTree: args[5], output: args[7], evidenceOutput: args[9] });
    process.stdout.write(`${result.manifestSha256}\n`);
  } catch (error) {
    console.error(error instanceof Error ? error.message : String(error));
    process.exitCode = 1;
  }
}
