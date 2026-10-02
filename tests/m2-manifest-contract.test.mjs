import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import test from 'node:test';

import { M2_QUICK_COMMANDS } from '../scripts/run-m2-quick.mjs';

async function text(relative) {
  return readFile(new URL(`../${relative}`, import.meta.url), 'utf8');
}

test('documents one explicit profile without weakening default M1', async () => {
  const [cli, manifest, conformance, status, roadmap] = await Promise.all([
    text('docs/CLI.md'),
    text('docs/M2_MANIFEST_V2.md'),
    text('docs/M2_CONFORMANCE.md'),
    text('docs/STATUS.md'),
    text('docs/ROADMAP.md'),
  ]);
  const normalizedManifest = manifest.replace(/\s+/g, ' ');
  const normalizedConformance = conformance.replace(/\s+/g, ' ');

  for (const required of [
    '`--profile control-flow-v1`',
    'omission means M1',
    '`zryna-manifest-v1.json`',
    '`zryna-manifest-v2.json`',
    '`ZRYNA-N4002`',
    'no partial JavaScript/WebAssembly bundle',
  ]) {
    assert.ok(cli.includes(required), `CLI contract is missing: ${required}`);
  }

  for (const required of [
    'Omitting `--profile` selects the existing protocol-v2 `I32V1` path',
    '`version`, exactly `2`',
    '`profile`, exactly `zryna-control-flow-v1`',
    '`ZRYNA-M2-GRAPH\\0`',
    'Sources, edges, targets, artifacts, results, and diagnostics',
    'The successful rename is the only commit point.',
    'no fallback and no partial portable-target bundle',
    'Issue #56 provides the separate executable fixed-oracle registry',
  ]) {
    assert.ok(normalizedManifest.includes(required), `manifest-v2 contract is missing: ${required}`);
  }

  assert.match(status, /exact `--profile control-flow-v1`[\s\S]*deterministic \[`zryna-manifest-v2\.json`\]/);
  for (const required of [
    'fixed external oracle',
    'all 37 historical resource limits',
    '`ZRYNA-F1103`',
    '`m2:check` uses bounded output',
    'Issue #57',
  ]) {
    assert.ok(normalizedConformance.includes(required), `M2 conformance is missing: ${required}`);
  }
  assert.match(roadmap, /^\| #55 \| .* \| #47, #51, #52, #54 \| complete \|$/m);
  assert.match(roadmap, /^\| #56 \| .* \| #55 \| complete \|$/m);
  assert.match(roadmap, /^\| #57 \| .* \| #56 \| external closure \|$/m);
  assert.match(roadmap, /Website synchronization and live provenance are external evidence tracked by #57/);
});

test('website documentation registry exports the profile and manifest authority', async () => {
  const registry = JSON.parse(await text('docs/website-bundle-v1.json'));
  const manifestEntry = registry.documents.find(({ id }) => id === 'reference/m2-manifest-v2');
  assert.deepEqual(manifestEntry, {
    id: 'reference/m2-manifest-v2',
    source: 'docs/M2_MANIFEST_V2.md',
    path: 'documents/reference/m2-manifest-v2.md',
    title: 'M2 manifest and atomic bundles',
  });
  assert.ok(registry.documents.some(({ id }) => id === 'reference/cli'));
  assert.ok(registry.documents.some(({ id }) => id === 'reference/m2-conformance'));
  assert.ok(registry.documents.some(({ id }) => id === 'status/current'));
  assert.ok(registry.documents.some(({ id }) => id === 'status/roadmap'));
});

test('the quick gate includes the focused public manifest/profile contract', async () => {
  const packageDocument = JSON.parse(await text('package.json'));
  assert.equal(packageDocument.scripts['m2:quick'], 'node scripts/run-m2-quick.mjs');
  assert.ok(M2_QUICK_COMMANDS.some(({ args }) =>
    args.includes('tests/m2-manifest-contract.test.mjs')));
  assert.equal(
    packageDocument.scripts['docs:check'],
    'node --test tests/docs-bundle.test.mjs tests/m2-contract.test.mjs tests/m2-manifest-contract.test.mjs tests/m3-contract.test.mjs tests/m3-public-docs.test.mjs tests/m7-generic-spec-fixtures.test.mjs tests/m7-generic-instance-spec-fixtures.test.mjs tests/m7-generic-review-spec.test.mjs tests/m7-generic-resource-spec.test.mjs tests/m7-generic-owned-layout-spec.test.mjs tests/m7-generic-key-spec.test.mjs',
  );
});
