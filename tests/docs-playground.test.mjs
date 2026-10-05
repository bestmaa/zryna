import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { mkdir, readFile, writeFile } from 'node:fs/promises';
import path from 'node:path';
import test from 'node:test';
import { exportDocsBundle, validateDocsBundle } from '../scripts/docs/bundle.mjs';
import { exportPlaygroundDocs } from '../scripts/docs/export-playground.mjs';
import { capturePlaygroundDocsSource, PLAYGROUND_CHANNEL, PLAYGROUND_REF, verifyGitProvenance,
  readSelectedDocsInput, validateProvenance } from '../scripts/docs/provenance.mjs';
import { documents, fixture } from './docs-playground/fixture.mjs';

const linux = process.platform === 'linux';
const digest = data => createHash('sha256').update(data).digest('hex');
const selection = f => capturePlaygroundDocsSource(f.source, { commit: f.commit, tree: f.tree }, f.environment);
const options = (f, protectedSource) => ({ workspaceRoot: f.source, output: f.output,
  channel: PLAYGROUND_CHANNEL, sourceCommit: f.commit, sourceRef: PLAYGROUND_REF,
  verifyGit: true, enforceWorkspaceOutput: false, protectedSource });
const route = f => exportPlaygroundDocs({ workspaceRoot: f.source, output: f.output,
  evidenceOutput: f.evidence, sourceCommit: f.commit, sourceTree: f.tree, environment: f.environment });
const expectations = (f, sha256) => ({ expectedManifestSha256: sha256, expectedChannel: PLAYGROUND_CHANNEL,
  expectedSourceCommit: f.commit, expectedSourceRef: PLAYGROUND_REF });

test('ordinary docs provenance uses an explicit fixture environment and retains production workflow guards', async t => {
  const f = await fixture(t), sourceRef = 'refs/heads/main';
  // Select the simulated environment for this call only; never rewrite process.env or the guard.
  verifyGitProvenance(f.source, f.commit, sourceRef, {});
  const workflow = { GITHUB_ACTIONS: 'true', GITHUB_SHA: f.commit, GITHUB_REF: sourceRef };
  verifyGitProvenance(f.source, f.commit, sourceRef, workflow);
  for (const mutation of [{ GITHUB_SHA: 'f'.repeat(40) }, { GITHUB_REF: PLAYGROUND_REF }]) {
    assert.throws(() => verifyGitProvenance(f.source, f.commit, sourceRef,
      { ...workflow, ...mutation }), /authenticated workflow context/);
  }
  assert.throws(() => verifyGitProvenance(f.source, 'f'.repeat(40), sourceRef, workflow), /checked-out HEAD/);
  assert.throws(() => verifyGitProvenance(f.source, f.commit, PLAYGROUND_REF, {}), /checked-out branch/);
  await writeFile(path.join(f.source, 'docs/M6_TOOLING.md'), 'dirty fixture\n');
  assert.throws(() => verifyGitProvenance(f.source, f.commit, sourceRef, workflow), /tracked compiler input is dirty/);
});

test('next and compiler-version provenance stay distinct from the fixed playground channel', () => {
  const sourceCommit = '1'.repeat(40);
  validateProvenance({ channel: 'next', sourceCommit, sourceRef: 'refs/heads/main', sourceVersion: '0.2.3' });
  validateProvenance({ channel: '0.2.3', sourceCommit, sourceRef: 'refs/tags/v0.2.3', sourceVersion: '0.2.3' });
  validateProvenance({ channel: PLAYGROUND_CHANNEL, sourceCommit, sourceRef: PLAYGROUND_REF, sourceVersion: '0.2.3' });
  for (const [channel, sourceRef, sourceVersion] of [['next', PLAYGROUND_REF, '0.2.3'],
    ['0.2.3', PLAYGROUND_REF, '0.2.3'], [PLAYGROUND_CHANNEL, 'refs/heads/main', '0.2.3'],
    [PLAYGROUND_CHANNEL, PLAYGROUND_REF, '0.1.0'], ['playground-0.1.1', PLAYGROUND_REF, '0.2.3']]) {
    assert.throws(() => validateProvenance({ channel, sourceCommit, sourceRef, sourceVersion }));
  }
});

test('portable consumer validates an independent canonical playground bundle fixture', async t => {
  const f = await fixture(t);
  const values = [
    ['reference/m6-conformance', 'documents/reference/m6-conformance.md', 'M6 tooling conformance v1',
      '# Conformance fixture\n\n[Support](../../docs/M6_TOOLING.md).\n'],
    ['reference/m6-tooling', 'documents/reference/m6-tooling.md', 'M6 tooling support and evidence',
      '# Support fixture\n\n[Contract](../spec/tooling/M6_CONFORMANCE_V1.md).\n'],
  ];
  const manifest = { schema: 'zryna.docs.bundle.v1', version: 1, channel: 'playground-0.1.0',
    source: { repository: 'https://github.com/zryna/zryna', commit: f.commit,
      ref: 'refs/tags/playground-v0.1.0', version: '0.2.3' },
    documents: values.map(([id, path, title, text]) => ({ id, path, title,
      bytes: Buffer.byteLength(text), sha256: digest(Buffer.from(text)) })) };
  await mkdir(path.join(f.output, 'documents/reference'), { recursive: true });
  for (const [_id, portable, _title, text] of values) await writeFile(path.join(f.output, portable), text);
  const data = Buffer.from(JSON.stringify(manifest, null, 2) + '\n'), sha256 = digest(data);
  await writeFile(path.join(f.output, 'manifest.json'), data);
  await writeFile(path.join(f.output, 'manifest.sha256'), `${sha256}  manifest.json\n`);
  const validated = await validateDocsBundle(f.output, { expectedManifestSha256: sha256,
    expectedChannel: 'playground-0.1.0', expectedSourceCommit: f.commit,
    expectedSourceRef: 'refs/tags/playground-v0.1.0' }, f.source);
  assert.deepEqual(validated.manifest, manifest);
});

test('simulated protected context with real Git objects exports exact documents and a separate source receipt', { skip: !linux }, async t => {
  const f = await fixture(t), result = await route(f);
  assert.equal(result.manifest.channel, 'playground-0.1.0');
  assert.deepEqual(result.manifest.source, { repository: 'https://github.com/zryna/zryna', commit: f.commit,
    ref: 'refs/tags/playground-v0.1.0', version: '0.2.3' });
  assert.equal(result.sourceReceipt.source.tree, f.tree);
  assert.equal(result.sourceReceipt.workflow.path, '.github/workflows/playground-release.yml');
  assert.equal(result.sourceReceipt.workflow.sha256, digest(await readFile(path.join(f.source, result.sourceReceipt.workflow.path))));
  assert.deepEqual(JSON.parse(await readFile(path.join(f.evidence, 'source-receipt.json'))), result.sourceReceipt);
  assert.deepEqual(result.manifest.documents.map(value => value.id), ['reference/m6-conformance', 'reference/m6-tooling']);
  for (const document of documents) {
    assert.deepEqual(await readFile(path.join(f.output, document.path)), await readFile(path.join(f.source, document.source)));
  }
  const checked = await validateDocsBundle(f.output, expectations(f, result.manifestSha256), f.source);
  assert.equal(checked.manifestSha256, result.manifestSha256);
  assert.equal(f.git(['status', '--porcelain=v1', '--untracked-files=all', '--ignored=matching']), '');
});

test('selected ordinary blobs authenticate package, registry, schema and documents despite temporary filesystem substitution', async t => {
  const f = await fixture(t), retained = selection(f);
  for (const name of ['package.json', 'docs/website-bundle-v1.json', 'schemas/zryna-docs-bundle-v1.schema.json', ...documents.map(item => item.source)]) {
    const file = path.join(f.source, name), original = await readFile(file);
    await writeFile(file, 'untrusted temporary snapshot\n');
    assert.deepEqual(readSelectedDocsInput(retained, file, 2097152), original);
    await writeFile(file, original);
    const independent = readSelectedDocsInput(retained, file, 2097152);
    independent.fill(0);
    assert.deepEqual(readSelectedDocsInput(retained, file, 2097152), original);
  }
  if (linux) {
    const result = await exportDocsBundle(options(f, retained));
    assert.equal(result.manifest.source.version, '0.2.3');
  }
});

test('actual same-size blob replacements cannot substitute documentation or the selected workflow', async t => {
  const f = await fixture(t);
  const workflow = '.github/workflows/playground-release.yml';
  const originals = new Map();
  for (const name of ['package.json', 'docs/M6_TOOLING.md', workflow]) {
    const original = await readFile(path.join(f.source, name));
    originals.set(name, original);
    const substitute = Buffer.from(original); substitute[0] ^= 1;
    const oid = f.git(['rev-parse', `${f.commit}:${name}`]);
    const replacement = f.git(['hash-object', '-w', '--stdin'], substitute);
    assert.notEqual(replacement, oid);
    f.git(['replace', oid, replacement]);
    assert.equal(f.git(['cat-file', '-s', oid]), String(original.length));
    assert.equal(f.git(['cat-file', 'blob', oid]), substitute.toString().trim());
  }
  assert.equal(f.git(['status', '--porcelain=v1', '--untracked-files=all', '--ignored=matching']), '');
  const retained = selection(f);
  for (const [name, original] of originals) {
    assert.deepEqual(readSelectedDocsInput(retained, path.join(f.source, name), 2097152), original);
  }
  if (linux) {
    const result = await route(f);
    assert.equal(result.sourceReceipt.workflow.sha256, digest(originals.get(workflow)));
    assert.deepEqual(await readFile(path.join(f.output, 'documents/reference/m6-tooling.md')),
      originals.get('docs/M6_TOOLING.md'));
  }
});

test('actual selected-tree replacement cannot change ordinary document blob selection', async t => {
  const f = await fixture(t), name = 'docs/M6_TOOLING.md';
  const original = await readFile(path.join(f.source, name));
  const substitute = Buffer.from(original); substitute[0] ^= 1;
  const oid = f.git(['rev-parse', `${f.commit}:${name}`]);
  const replacement = f.git(['hash-object', '-w', '--stdin'], substitute);
  const docsTree = f.git(['rev-parse', `${f.commit}:docs`]);
  const changedDocs = f.git(['mktree'], f.git(['cat-file', '-p', docsTree]).replace(oid, replacement) + '\n');
  const changedRoot = f.git(['mktree'], f.git(['cat-file', '-p', f.tree]).replace(docsTree, changedDocs) + '\n');
  f.git(['replace', f.tree, changedRoot]);
  assert(f.git(['ls-tree', f.commit, '--', name]).includes(replacement));
  const retained = selection(f);
  assert.deepEqual(readSelectedDocsInput(retained, path.join(f.source, name), 2097152), original);
  if (linux) {
    const result = await route(f);
    assert.equal(result.sourceReceipt.source.tree, f.tree);
    assert.deepEqual(await readFile(path.join(f.output, 'documents/reference/m6-tooling.md')), original);
  }
});

test('generic export rejects plain receipts, cloned tokens and verifyGit=false', async t => {
  const f = await fixture(t), retained = selection(f);
  for (const forged of [undefined, {}, structuredClone(retained), { source: { commit: f.commit, tree: f.tree } }]) {
    await assert.rejects(exportDocsBundle(options(f, forged)), /retained protected/);
  }
  await assert.rejects(exportDocsBundle({ ...options(f, retained), verifyGit: false }), /verified selection/);
  await assert.rejects(exportDocsBundle({ ...options(f, retained), enforceWorkspaceOutput: true }), /external output/);
  await assert.rejects(exportDocsBundle({ ...options(f, retained), channel: 'next', sourceRef: 'refs/heads/main' }), /only for the playground/);
});

test('retained selection cannot be relabeled with another root or commit', async t => {
  const f = await fixture(t), other = await fixture(t), retained = selection(f);
  await assert.rejects(exportDocsBundle({ ...options(f, retained), workspaceRoot: other.source }), /selection differs/);
  await assert.rejects(exportDocsBundle({ ...options(f, retained), sourceCommit: 'f'.repeat(40) }), /selection differs/);
});

for (const [field, value] of [['GITHUB_EVENT_NAME', 'workflow_dispatch'], ['GITHUB_REPOSITORY', 'other/repository'],
  ['GITHUB_REF_PROTECTED', 'false'], ['GITHUB_SHA', 'f'.repeat(40)], ['GITHUB_WORKFLOW_SHA', 'e'.repeat(40)],
  ['GITHUB_WORKFLOW_REF', 'zryna/zryna/.github/workflows/ci.yml@refs/heads/main']]) {
  test(`simulated context rejects substituted ${field}`, async t => {
    const f = await fixture(t);
    assert.throws(() => capturePlaygroundDocsSource(f.source, { commit: f.commit, tree: f.tree },
      { ...f.environment, [field]: value }), /BUILD-CONTEXT/);
  });
}

for (const name of ['docs/M6_TOOLING.md', 'untracked.tmp', 'ignored.tmp']) {
  test(`protected source rejects dirty ${name}`, async t => {
    const f = await fixture(t);
    await writeFile(path.join(f.source, name), 'changed\n');
    assert.throws(() => selection(f), /BUILD-SOURCE/);
  });
}

test('lightweight tag and wrong independently selected tree reject', async t => {
  const f = await fixture(t, { lightweight: true });
  assert.throws(() => selection(f), /BUILD-SOURCE/);
  const annotated = await fixture(t);
  assert.throws(() => capturePlaygroundDocsSource(annotated.source, { commit: annotated.commit, tree: 'e'.repeat(40) },
    annotated.environment), /BUILD-SOURCE/);
});

for (const name of ['package.json', 'docs/website-bundle-v1.json', 'schemas/zryna-docs-bundle-v1.schema.json', 'docs/M6_TOOLING.md']) {
  test(`selected ${name} must be an ordinary nonexecutable Git blob`, async t => {
    const f = await fixture(t, { executable: name }), retained = selection(f);
    assert.throws(() => readSelectedDocsInput(retained, path.join(f.source, name), 2097152), /ordinary Git blob/);
  });
}

test('selected blob bounds and paths reject independently of a sealed source context', async t => {
  const f = await fixture(t), retained = selection(f);
  assert.throws(() => readSelectedDocsInput(retained, path.join(f.source, 'docs/M6_TOOLING.md'), 1), /blob bounds/);
  assert.throws(() => readSelectedDocsInput(retained, path.join(f.root, 'outside.md'), 2097152), /path or bounds/);
  assert.throws(() => readSelectedDocsInput({}, path.join(f.source, 'package.json'), 2097152), /retained protected/);
});

test('compiler package version remains 0.2.3 rather than inheriting toolkit 0.1.0', { skip: !linux }, async t => {
  const f = await fixture(t, { version: '0.1.0' });
  await assert.rejects(route(f), /compiler package version 0.2.3/);
});

test('resealed consumer metadata cannot relabel channel/ref/compiler version', { skip: !linux }, async t => {
  for (const mutate of [m => { m.source.version = '0.1.0'; }, m => { m.source.ref = 'refs/heads/main'; },
    m => { m.channel = 'playground-0.1.1'; }]) {
    const f = await fixture(t), result = await route(f), manifest = structuredClone(result.manifest);
    mutate(manifest);
    const data = Buffer.from(JSON.stringify(manifest, null, 2) + '\n'), sha256 = digest(data);
    await writeFile(path.join(f.output, 'manifest.json'), data);
    await writeFile(path.join(f.output, 'manifest.sha256'), `${sha256}  manifest.json\n`);
    await assert.rejects(validateDocsBundle(f.output, { ...expectations(f, sha256),
      expectedChannel: manifest.channel, expectedSourceRef: manifest.source.ref }, f.source), /schema failed|playground documentation/);
  }
});

test('dedicated output/evidence directories may not nest or coincide', async t => {
  const f = await fixture(t);
  for (const evidenceOutput of [f.output, path.join(f.output, 'evidence')]) {
    await assert.rejects(exportPlaygroundDocs({ workspaceRoot: f.source, output: f.output, evidenceOutput,
      sourceCommit: f.commit, sourceTree: f.tree, environment: f.environment }), /separate absolute/);
  }
});

test('Windows source metadata checks do not imply Linux protected output support', { skip: linux }, async t => {
  const f = await fixture(t);
  await assert.rejects(route(f), /requires Linux/);
});
