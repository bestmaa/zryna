import assert from 'node:assert/strict';
import test from 'node:test';

import { preflightNativeProvenance } from '../scripts/native-recipe/provenance.mjs';
import { fixture, provenance } from './native-recipe/fixture.mjs';

test('matching output hashes remain unauthenticated and cannot authorize publication or reuse', async () => {
  const { review } = await fixture();
  const { candidate, outputMaterials } = provenance(review);
  assert.throws(() => preflightNativeProvenance(review, candidate, outputMaterials),
    /TRUST-PROVENANCE: authenticated builder, execution, isolation and output-audit evidence is unavailable/);
});

test('missing, stale, cross-plan and permissive-mode provenance rejects at the binding boundary', async () => {
  const { review } = await fixture();
  const { candidate, outputMaterials } = provenance(review);
  for (const changed of [undefined, {}, { ...candidate, recipeSha256: '0'.repeat(64) },
    { ...candidate, planKey: '0'.repeat(64) }, { ...candidate, mode: 'pure-source' },
    { ...candidate, mode: 'untrusted-playground' }, { ...candidate, outputs: [] }]) {
    assert.throws(() => preflightNativeProvenance(review, changed, outputMaterials),
      /TRUST-PROVENANCE: provenance does not bind/);
  }
  assert.throws(() => preflightNativeProvenance(structuredClone(review), candidate, outputMaterials), /TRUST-NATIVE/);
});

test('signed and self-reported isolation claims never become observed policy evidence', async () => {
  const { review } = await fixture();
  const { candidate, outputMaterials } = provenance(review);
  for (const claims of [{ signature: 'valid', authenticated: true }, { isolation: 'verified' },
    { environmentCleared: true, processGroup: true, network: 'disabled' }, { ffiPassed: true }]) {
    assert.throws(() => preflightNativeProvenance(review, { ...candidate, ...claims }, outputMaterials),
      /TRUST-PROVENANCE: provenance does not bind/);
  }
});

test('output target/path/size/hash and extra authority are independently rejected', async () => {
  const { review } = await fixture();
  const { candidate, outputMaterials } = provenance(review);
  const changes = [
    output => { output.target = 'javascript'; }, output => { output.path = '../escape.elf'; },
    output => { output.path = 'native/other.elf'; }, output => { output.size = -1; },
    output => { output.size = 1073741825; }, output => { output.sha256 = 'A'.repeat(64); },
    output => { output.policy = 'trusted'; },
  ];
  for (const change of changes) {
    const changed = structuredClone(candidate);
    change(changed.outputs[0]);
    assert.throws(() => preflightNativeProvenance(review, changed, outputMaterials),
      /TRUST-PROVENANCE: output metadata/);
  }
  for (const materials of [new Map(), new Map([...outputMaterials, ['extra', Buffer.from('extra')]])]) {
    assert.throws(() => preflightNativeProvenance(review, candidate, materials), /TRUST-PROVENANCE: provenance does not bind/);
  }
});

test('output substitution or detached path inventory rejects instead of becoming an audited result', async () => {
  const { review } = await fixture();
  const { candidate, outputMaterials } = provenance(review);
  for (const materials of [new Map([['native/app.elf', Buffer.from('tampered')]]),
    new Map([['native/different.elf', outputMaterials.get('native/app.elf')]])]) {
    assert.throws(() => preflightNativeProvenance(review, candidate, materials), /TRUST-PROVENANCE: output bytes/);
  }
  const changed = structuredClone(candidate);
  changed.outputs[0].size += 1;
  assert.throws(() => preflightNativeProvenance(review, changed, outputMaterials), /TRUST-PROVENANCE: output bytes/);
});
