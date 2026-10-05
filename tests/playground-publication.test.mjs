import assert from 'node:assert/strict';
import { chmodSync, existsSync, mkdtempSync, mkdirSync, readFileSync, renameSync, rmSync, symlinkSync, truncateSync, unlinkSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { test } from 'node:test';
import { sha256 } from '../scripts/distribution/canonical.mjs';
import { captureBuildFiles, captureBuildInput } from '../examples/playground/restricted/publication/capture.mjs';
import { captureProtectedSource } from '../examples/playground/restricted/publication/source.mjs';
import { assembleProtectedToolkit, retainUnsignedAssembly } from '../examples/playground/restricted/publication/assemble.mjs';

// Filesystem fixtures and rejected context checks establish no protected release or signature pass.
test('unsigned producer rejects an unreviewed plan and wrong tag context before any material capture', async () => {
  const input = Buffer.from('{}');
  await assert.rejects(assembleProtectedToolkit(input, { version: 1, planSha256: 'a'.repeat(64),
    sourceCommit: 'b'.repeat(40), sourceTree: 'c'.repeat(40), nestedCompiler: {}, requiredGates: [] },
  '/missing', '/missing', {}), /BUILD-POLICY/);
  assert.throws(() => captureProtectedSource('/missing',
    { commit: 'b'.repeat(40), tree: 'c'.repeat(40) }, {}), /BUILD-CONTEXT/);
});

test('protected source rejects a substituted source claim before invoking Git', () => {
  const source = { commit: 'a'.repeat(40), tree: 'b'.repeat(40) };
  const environment = { GITHUB_EVENT_NAME: 'push', GITHUB_REPOSITORY: 'zryna/zryna',
    GITHUB_REF: 'refs/tags/playground-v0.1.0', GITHUB_REF_TYPE: 'tag',
    GITHUB_REF_NAME: 'playground-v0.1.0', GITHUB_REF_PROTECTED: 'true', GITHUB_SHA: source.commit,
    GITHUB_WORKFLOW_SHA: source.commit,
    GITHUB_WORKFLOW_REF: 'zryna/zryna/.github/workflows/playground-release.yml@refs/tags/playground-v0.1.0' };
  for (const mutation of [{ GITHUB_EVENT_NAME: 'workflow_dispatch' }, { GITHUB_REPOSITORY: 'other/repository' },
    { GITHUB_REF_PROTECTED: 'false' }, { GITHUB_SHA: 'c'.repeat(40) },
    { GITHUB_WORKFLOW_SHA: 'c'.repeat(40) }, { GITHUB_REF: 'refs/heads/main' },
    { GITHUB_WORKFLOW_REF: environment.GITHUB_WORKFLOW_REF.replace('release.yml', 'other.yml') }]) {
    assert.throws(() => captureProtectedSource('/missing', source, { ...environment, ...mutation }), /BUILD-CONTEXT/);
  }
});

test('Linux captures exact ordinary input bytes and rejects symlink ancestors, leaves and special files',
  { skip: process.platform !== 'linux' }, () => {
    const root = mkdtempSync(join(tmpdir(), 'playground-publication-'));
    try {
      mkdirSync(join(root, 'inputs'));
      const data = Buffer.from('retained build input');
      writeFileSync(join(root, 'inputs', 'file'), data);
      const digest = sha256(data);
      assert.deepEqual(captureBuildInput(root, 'inputs/file', data.length, digest), data);
      assert.throws(() => captureBuildInput(root, 'inputs/file', data.length + 1, digest), /BUILD-INPUT/);
      assert.throws(() => captureBuildInput(root, 'inputs/file', data.length, 'a'.repeat(64)), /BUILD-INPUT/);
      symlinkSync('inputs', join(root, 'alias'));
      symlinkSync('file', join(root, 'inputs', 'link'));
      assert.throws(() => captureBuildInput(root, 'alias/file', data.length, digest));
      assert.throws(() => captureBuildInput(root, 'inputs/link', data.length, digest));
      assert.throws(() => captureBuildInput('/dev', 'null', data.length, digest), /BUILD-INPUT/);
      chmodSync(join(root, 'inputs', 'file'), 0o4644);
      assert.throws(() => captureBuildInput(root, 'inputs/file', data.length, digest), /BUILD-INPUT/);
    } finally { rmSync(root, { recursive: true, force: true }); }
  });

test('file inventory rejects aliases and aggregate limits before touching a missing root', () => {
  const file = { path: 'inputs/a', input: 'a', bytes: 1, sha256: 'a'.repeat(64), mode: 0o644 };
  for (const files of [[{ ...file, bytes: 268435457 }],
    [file, { ...file, path: 'inputs/b' }],
    [{ ...file, path: 'A/file' }, { ...file, path: 'a/other', input: 'b' }],
    [file, { ...file, path: 'inputs/b', input: 'b', bytes: 268435456 },
      { ...file, path: 'inputs/c', input: 'c', bytes: 268435456 }]]) {
    assert.throws(() => captureBuildFiles('/missing', files));
  }
});

const result = () => ({ archive: Buffer.from('archive'), envelope: Buffer.from('envelope'),
  candidatePolicy: { review: 'required' }, sourceReceipt: { source: { commit: 'a'.repeat(40) } } });

test('unsigned assembly keeps complete output hashes and refuses replacement',
  { skip: process.platform !== 'linux' }, () => {
  const root = mkdtempSync(join(tmpdir(), 'playground-publication-output-'));
  try {
    const output = join(root, 'candidate');
    const source = join(root, 'source'); mkdirSync(source);
    retainUnsignedAssembly(output, result(), source);
    const receipt = JSON.parse(readFileSync(join(output, 'assembly-receipt.json')));
    assert.equal(receipt.status, 'unsigned-review-candidate');
    for (const file of receipt.files) {
      const data = readFileSync(join(output, file.path));
      assert.equal(data.length, file.bytes); assert.equal(sha256(data), file.sha256);
    }
    assert.throws(() => retainUnsignedAssembly(output, result(), source), /EEXIST/);
  } finally { rmSync(root, { recursive: true, force: true }); }
});

test('output rejects the exact outside-name symlink into source before creating any source child',
  { skip: process.platform !== 'linux' }, () => {
    const root = mkdtempSync(join(tmpdir(), 'playground-publication-alias-'));
    try {
      const source = join(root, 'source'); mkdirSync(source);
      symlinkSync('source', join(root, 'out-link'));
      assert.throws(() => retainUnsignedAssembly(join(root, 'out-link', 'candidate'), result(), source));
      assert.equal(existsSync(join(source, 'candidate')), false);
      assert.throws(() => retainUnsignedAssembly(join(source, 'candidate'), result(), source), /BUILD-OUTPUT/);
      assert.equal(existsSync(join(source, 'candidate')), false);
    } finally { rmSync(root, { recursive: true, force: true }); }
  });

test('renamed output directory retains only its prior partial bytes and never emits completion elsewhere',
  { skip: process.platform !== 'linux' }, () => {
    const root = mkdtempSync(join(tmpdir(), 'playground-publication-race-'));
    try {
      const source = join(root, 'source'); mkdirSync(source);
      const output = join(root, 'candidate'), retained = join(root, 'retained');
      assert.throws(() => retainUnsignedAssembly(output, result(), source, index => {
        if (index !== 1) return;
        renameSync(output, retained); mkdirSync(output, { mode: 0o700 });
      }), /BUILD-OUTPUT-SUBSTITUTION/);
      assert.equal(readFileSync(join(retained, 'zryna-playground-0.1.0-x86_64-unknown-linux-gnu.tar.gz')).toString(), 'archive');
      for (const directory of [output, retained]) {
        assert.equal(existsSync(join(directory, 'toolkit-envelope.json')), false);
        assert.equal(existsSync(join(directory, 'assembly-receipt.json')), false);
      }
    } finally { rmSync(root, { recursive: true, force: true }); }
  });

test('renamed output ancestor rejects before its first write despite a same-mode replacement hierarchy',
  { skip: process.platform !== 'linux' }, () => {
    const root = mkdtempSync(join(tmpdir(), 'playground-publication-parent-race-'));
    try {
      const source = join(root, 'source'); mkdirSync(source);
      const parent = join(root, 'parent'); mkdirSync(parent);
      const moved = join(root, 'moved');
      const output = join(parent, 'candidate');
      assert.throws(() => retainUnsignedAssembly(output, result(), source, index => {
        if (index !== 0) return;
        renameSync(parent, moved); mkdirSync(parent); mkdirSync(output, { mode: 0o700 });
      }), /BUILD-OUTPUT-SUBSTITUTION/);
      for (const directory of [output, join(moved, 'candidate')]) {
        assert.equal(existsSync(join(directory, 'zryna-playground-0.1.0-x86_64-unknown-linux-gnu.tar.gz')), false);
        assert.equal(existsSync(join(directory, 'assembly-receipt.json')), false);
      }
    } finally { rmSync(root, { recursive: true, force: true }); }
  });

test('real retained-file truncation, same-size mutation, replacement and unlink prevent completion',
  { skip: process.platform !== 'linux' }, () => {
    const root = mkdtempSync(join(tmpdir(), 'playground-publication-leaf-race-'));
    try {
      const source = join(root, 'source'); mkdirSync(source);
      const mutations = [path => truncateSync(path, 1), path => writeFileSync(path, 'changed'),
        path => { unlinkSync(path); writeFileSync(path, 'archive', { mode: 0o600 }); },
        path => unlinkSync(path)];
      for (const [number, mutate] of mutations.entries()) {
        const output = join(root, `candidate-${number}`);
        assert.throws(() => retainUnsignedAssembly(output, result(), source, index => {
          if (index === 1) mutate(join(output, 'zryna-playground-0.1.0-x86_64-unknown-linux-gnu.tar.gz'));
        }));
        assert.equal(existsSync(join(output, 'assembly-receipt.json')), false);
        assert.equal(existsSync(join(output, 'toolkit-envelope.json')), false);
      }
    } finally { rmSync(root, { recursive: true, force: true }); }
  });
