import { execFileSync } from 'node:child_process';
import { chmod, copyFile, mkdir, mkdtemp, rm, writeFile } from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';
import { compilerWorkspaceRoot } from '../../scripts/docs/bundle.mjs';

export const documents = [
  { id: 'reference/m6-conformance', source: 'spec/tooling/M6_CONFORMANCE_V1.md',
    path: 'documents/reference/m6-conformance.md', title: 'M6 tooling conformance v1' },
  { id: 'reference/m6-tooling', source: 'docs/M6_TOOLING.md',
    path: 'documents/reference/m6-tooling.md', title: 'M6 tooling support and evidence' },
];

// Real Git objects with simulated protected workflow context; no hosted publication is exercised.
export async function fixture(context, { version = '0.2.3', executable, lightweight = false } = {}) {
  const root = await mkdtemp(path.join(os.tmpdir(), 'zryna-docs-publication-'));
  context.after(async () => {
    if (path.dirname(root) !== path.resolve(os.tmpdir()) || !path.basename(root).startsWith('zryna-docs-publication-')) {
      throw new Error('fixture cleanup escaped its owned directory');
    }
    await rm(root, { recursive: true, force: true });
  });
  let source = path.join(root, 'source');
  await mkdir(source);
  for (const directory of ['docs', 'spec/tooling', 'schemas', '.github/workflows']) {
    await mkdir(path.join(source, directory), { recursive: true });
  }
  await writeFile(path.join(source, 'package.json'), JSON.stringify({ version }) + '\n');
  await writeFile(path.join(source, '.gitignore'), 'ignored.tmp\n');
  await copyFile(path.join(compilerWorkspaceRoot, 'schemas/zryna-docs-bundle-v1.schema.json'),
    path.join(source, 'schemas/zryna-docs-bundle-v1.schema.json'));
  await chmod(path.join(source, 'schemas/zryna-docs-bundle-v1.schema.json'), 0o644);
  await writeFile(path.join(source, 'docs/website-bundle-v1.json'), JSON.stringify({ schemaVersion: 1,
    bundleSchema: 'zryna.docs.bundle.v1', documents }) + '\n');
  await writeFile(path.join(source, documents[0].source), '# Conformance fixture\n\n[Support](../../docs/M6_TOOLING.md).\n');
  await writeFile(path.join(source, documents[1].source), '# Support fixture\n\n[Contract](../spec/tooling/M6_CONFORMANCE_V1.md).\n');
  await writeFile(path.join(source, '.github/workflows/playground-release.yml'), 'name: Documentation fixture\non: push\n');
  const git = (args, input) => execFileSync('git', args, { cwd: source, encoding: 'utf8', timeout: 5000, input,
    env: { ...process.env, GIT_AUTHOR_DATE: '2000-01-01T00:00:00Z', GIT_COMMITTER_DATE: '2000-01-01T00:00:00Z' },
    stdio: [input === undefined ? 'ignore' : 'pipe', 'pipe', 'pipe'] }).trim();
  git(['init', '-b', 'main']);
  // Git can canonicalize Windows temporary paths differently from os.tmpdir().
  source = path.resolve(git(['rev-parse', '--show-toplevel']));
  git(['config', 'user.name', 'Fixture Contributor']);
  git(['config', 'user.email', 'fixture@example.invalid']);
  git(['config', 'core.autocrlf', 'false']);
  git(['config', 'commit.gpgsign', 'false']);
  git(['config', 'tag.gpgsign', 'false']);
  git(['add', '.']);
  if (executable) {
    await chmod(path.join(source, executable), 0o755);
    git(['update-index', '--chmod=+x', executable]);
  }
  git(['commit', '-m', 'Documentation fixture']);
  const commit = git(['rev-parse', 'HEAD']), tree = git(['show', '-s', '--format=%T', 'HEAD']);
  git(lightweight ? ['tag', 'playground-v0.1.0'] : ['tag', '-a', 'playground-v0.1.0', '-m', 'Documentation fixture']);
  const environment = { GITHUB_EVENT_NAME: 'push', GITHUB_REPOSITORY: 'zryna/zryna',
    GITHUB_REF: 'refs/tags/playground-v0.1.0', GITHUB_REF_TYPE: 'tag', GITHUB_REF_NAME: 'playground-v0.1.0',
    GITHUB_REF_PROTECTED: 'true', GITHUB_SHA: commit, GITHUB_WORKFLOW_SHA: commit,
    GITHUB_WORKFLOW_REF: 'zryna/zryna/.github/workflows/playground-release.yml@refs/tags/playground-v0.1.0' };
  return { root, source, commit, tree, git, environment, output: path.join(root, 'bundle'), evidence: path.join(root, 'evidence') };
}
