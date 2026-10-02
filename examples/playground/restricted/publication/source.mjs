// A protected tag selects source; assembly never infers authority from its own output.
import { spawnSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { isAbsolute, resolve } from 'node:path';
import { sha256 } from '../../../../scripts/distribution/canonical.mjs';
import { exact, fail } from '../limits.mjs';

const ref = 'refs/tags/playground-v0.1.0';
const workflow = '.github/workflows/playground-release.yml';
const objectId = /^[a-f0-9]{40}$/;
const maximum = 262144;

export function protectedGitEnvironment() {
  return { ...Object.fromEntries(Object.entries(process.env).filter(([name]) =>
    !name.toUpperCase().startsWith('GIT_'))), GIT_NO_REPLACE_OBJECTS: '1', GIT_OPTIONAL_LOCKS: '0' };
}

function git(root, args, binary = false) {
  const result = spawnSync('git', ['--no-replace-objects', '-c', 'core.fsmonitor=false', ...args],
    { cwd: root, shell: false, windowsHide: true, env: protectedGitEnvironment(),
    encoding: binary ? null : 'utf8', timeout: 5000, killSignal: 'SIGKILL', maxBuffer: maximum });
  if (result.error || result.status !== 0 || result.signal ||
      Buffer.byteLength(result.stdout ?? '') > maximum ||
      Buffer.byteLength(result.stderr ?? '') > maximum) fail('BUILD-SOURCE');
  return result.stdout;
}

function line(root, args) {
  const output = git(root, args);
  if (!output.endsWith('\n') || output.slice(0, -1).includes('\n') || output.includes('\r')) {
    fail('BUILD-SOURCE');
  }
  return output.slice(0, -1);
}

function object(root, id, kind) {
  const size = line(root, ['cat-file', '-s', id]);
  if (!/^[1-9][0-9]*$/.test(size) || Number(size) > maximum) fail('BUILD-SOURCE');
  const data = git(root, ['cat-file', kind, id], true);
  const digest = createHash('sha1').update(`${kind} ${data.length}\0`).update(data).digest('hex');
  if (data.length !== Number(size) || digest !== id) fail('BUILD-SOURCE');
  return data;
}

function tagCommit(data) {
  const separator = data.indexOf(Buffer.from('\n\n'));
  if (separator < 1 || separator > 65536) fail('BUILD-SOURCE');
  let header;
  try { header = new TextDecoder('utf-8', { fatal: true }).decode(data.subarray(0, separator)); }
  catch { fail('BUILD-SOURCE'); }
  const fields = new Map();
  for (const row of header.split('\n')) {
    const split = row.indexOf(' ');
    if (split < 1 || fields.has(row.slice(0, split))) fail('BUILD-SOURCE');
    fields.set(row.slice(0, split), row.slice(split + 1));
  }
  if (fields.get('type') !== 'commit' || fields.get('tag') !== ref.slice(10) ||
      !fields.get('tagger') || !objectId.test(fields.get('object') ?? '')) fail('BUILD-SOURCE');
  return fields.get('object');
}

export function captureProtectedSource(root, reviewed, environment = process.env) {
  exact(reviewed, ['commit', 'tree']);
  if (!objectId.test(reviewed.commit) || !objectId.test(reviewed.tree) || !isAbsolute(root) ||
      environment.GITHUB_EVENT_NAME !== 'push' || environment.GITHUB_REPOSITORY !== 'zryna/zryna' ||
      environment.GITHUB_REF !== ref || environment.GITHUB_REF_TYPE !== 'tag' ||
      environment.GITHUB_REF_NAME !== 'playground-v0.1.0' || environment.GITHUB_REF_PROTECTED !== 'true' ||
      environment.GITHUB_SHA !== reviewed.commit || environment.GITHUB_WORKFLOW_SHA !== reviewed.commit ||
      environment.GITHUB_WORKFLOW_REF !== `zryna/zryna/${workflow}@${ref}`) fail('BUILD-CONTEXT');
  if (resolve(line(root, ['rev-parse', '--show-toplevel'])) !== resolve(root) ||
      git(root, ['status', '--porcelain=v1', '--untracked-files=all', '--ignored=matching']).length) {
    fail('BUILD-SOURCE');
  }
  const tagObject = line(root, ['show-ref', '--verify', '--hash', ref]);
  if (!objectId.test(tagObject) || line(root, ['cat-file', '-t', tagObject]) !== 'tag') fail('BUILD-SOURCE');
  const commit = tagCommit(object(root, tagObject, 'tag'));
  if (commit !== reviewed.commit || commit === tagObject ||
      line(root, ['rev-parse', `${ref}^{commit}`]) !== commit ||
      line(root, ['rev-parse', 'HEAD']) !== commit || line(root, ['cat-file', '-t', commit]) !== 'commit' ||
      line(root, ['show', '-s', '--format=%T', commit]) !== reviewed.tree) fail('BUILD-SOURCE');
  const epoch = line(root, ['show', '-s', '--format=%ct', commit]);
  if (!/^(0|[1-9][0-9]*)$/.test(epoch) || Number(epoch) > 0xffffffff) fail('BUILD-SOURCE');
  const entry = git(root, ['ls-tree', '-z', '--full-tree', commit, '--', workflow], true);
  const match = /^100644 blob ([a-f0-9]{40})\t\.github\/workflows\/playground-release\.yml\0$/.exec(
    entry.toString('ascii'));
  if (!match) fail('BUILD-SOURCE');
  const workflowBytes = object(root, match[1], 'blob');
  if (line(root, ['show-ref', '--verify', '--hash', ref]) !== tagObject ||
      line(root, ['rev-parse', 'HEAD']) !== commit ||
      git(root, ['status', '--porcelain=v1', '--untracked-files=all', '--ignored=matching']).length) {
    fail('BUILD-SOURCE');
  }
  return { format: 'zryna.playground-source-receipt.v1', version: 1,
    source: { repository: 'https://github.com/zryna/zryna', ref, commit, tree: reviewed.tree,
      sourceDateEpoch: Number(epoch) }, tagObject,
    workflow: { path: workflow, object: match[1], bytes: workflowBytes.length, sha256: sha256(workflowBytes) } };
}
