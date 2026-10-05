import { execFileSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import path from 'node:path';
import { captureProtectedSource, protectedGitEnvironment } from '../../examples/playground/restricted/publication/source.mjs';

export const PLAYGROUND_CHANNEL = 'playground-0.1.0';
export const PLAYGROUND_REF = 'refs/tags/playground-v0.1.0';
export const SEMVER = /^(?:0|[1-9][0-9]*)\.(?:0|[1-9][0-9]*)\.(?:0|[1-9][0-9]*)(?:-(?:(?:0|[1-9][0-9]*)|[0-9A-Za-z-]*[A-Za-z-][0-9A-Za-z-]*)(?:\.(?:(?:0|[1-9][0-9]*)|[0-9A-Za-z-]*[A-Za-z-][0-9A-Za-z-]*))*)?$/;
const selected = new WeakMap();
const fail = message => { throw new Error(`invalid documentation bundle: ${message}`); };

export function validateProvenance({ channel, sourceCommit, sourceRef, sourceVersion }) {
  if (!/^[0-9a-f]{40}$/.test(sourceCommit)) fail('source commit must be 40 lowercase hexadecimal characters');
  if (!SEMVER.test(sourceVersion)) fail('source version must be canonical semantic version text');
  if (channel === 'next') {
    if (sourceRef !== 'refs/heads/main') fail('next channel requires refs/heads/main');
    return;
  }
  if (channel === PLAYGROUND_CHANNEL) {
    if (sourceRef !== PLAYGROUND_REF || sourceVersion !== '0.2.3') {
      fail('playground documentation requires its fixed tag and compiler package version 0.2.3');
    }
    return;
  }
  if (!SEMVER.test(channel)) fail('channel must be next or a canonical semantic version');
  if (channel !== sourceVersion || sourceRef !== `refs/tags/v${channel}`) {
    fail('version channels require the matching source version and immutable v-prefixed tag');
  }
}

export function verifyGitProvenance(workspaceRoot, sourceCommit, sourceRef, environment = process.env) {
  const run = args => execFileSync('git', args, { cwd: workspaceRoot, encoding: 'utf8',
    stdio: ['ignore', 'pipe', 'pipe'] }).trim();
  if (run(['rev-parse', 'HEAD']) !== sourceCommit) fail('source commit does not match checked-out HEAD');
  if (run(['status', '--porcelain', '--untracked-files=no']) !== '') fail('tracked compiler input is dirty');
  if (environment.GITHUB_ACTIONS === 'true') {
    if (environment.GITHUB_SHA !== sourceCommit || environment.GITHUB_REF !== sourceRef) {
      fail('source provenance does not match authenticated workflow context');
    }
  } else if (run(['symbolic-ref', '-q', 'HEAD']) !== sourceRef) {
    fail('source ref does not match the checked-out branch');
  }
}

export function capturePlaygroundDocsSource(workspaceRoot, reviewed, environment = process.env) {
  const snapshot = { ...environment };
  const receipt = captureProtectedSource(workspaceRoot, reviewed, snapshot);
  const selection = Object.freeze({});
  selected.set(selection, { root: path.resolve(workspaceRoot), reviewed: { ...reviewed },
    environment: snapshot, receipt, inputs: new Map() });
  return selection;
}

function state(selection) {
  const value = selected.get(selection);
  if (!value) fail('retained protected playground source selection required');
  return value;
}

export function revalidatePlaygroundDocsSource(selection, workspaceRoot, sourceCommit, sourceRef) {
  const value = state(selection);
  if (value.root !== path.resolve(workspaceRoot) || value.reviewed.commit !== sourceCommit || sourceRef !== PLAYGROUND_REF) {
    fail('protected playground source selection differs');
  }
  const receipt = captureProtectedSource(value.root, value.reviewed, value.environment);
  if (JSON.stringify(receipt) !== JSON.stringify(value.receipt)) fail('protected documentation source changed');
  return structuredClone(receipt);
}

export function readSelectedDocsInput(selection, filePath, maximum) {
  const value = state(selection);
  const portable = path.relative(value.root, filePath).split(path.sep).join('/');
  if (!portable || path.isAbsolute(portable) || portable.split('/').includes('..') ||
      portable.includes('\\') || !/^[A-Za-z0-9._/-]+$/.test(portable) ||
      !Number.isSafeInteger(maximum) || maximum < 1 || maximum > 2097152) fail('selected documentation input path or bounds');
  if (!value.inputs.has(portable)) {
    const git = (args, bound) => execFileSync('git',
      ['--no-replace-objects', '-c', 'core.fsmonitor=false', ...args], { cwd: value.root, timeout: 5000,
        maxBuffer: bound + 1, windowsHide: true, env: protectedGitEnvironment(), stdio: ['ignore', 'pipe', 'pipe'] });
    const entry = git(['ls-tree', '-z', '--full-tree', value.reviewed.commit, '--', portable], 4096);
    const match = /^100644 blob ([0-9a-f]{40})\t([^\0]+)\0$/.exec(entry.toString('utf8'));
    if (!match || match[2] !== portable) fail('selected documentation input must be one ordinary Git blob');
    const declared = git(['cat-file', '-s', match[1]], 64).toString('ascii');
    if (!/^(0|[1-9][0-9]*)\n$/.test(declared) || Number(declared) > maximum) fail('selected documentation blob bounds');
    const data = git(['cat-file', 'blob', match[1]], maximum);
    const digest = createHash('sha1').update(`blob ${data.length}\0`).update(data).digest('hex');
    if (data.length !== Number(declared) || digest !== match[1]) fail('selected documentation blob identity changed');
    value.inputs.set(portable, Buffer.from(data));
  }
  const data = value.inputs.get(portable);
  if (data.length > maximum) fail('selected documentation blob bounds');
  return Buffer.from(data);
}
