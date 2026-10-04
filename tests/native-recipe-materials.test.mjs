import assert from 'node:assert/strict';
import { mkdtemp, readFile, readdir, rm, writeFile } from 'node:fs/promises';
import os from 'node:os';
import path from 'node:path';
import test from 'node:test';

import { describeNativeRecipe } from '../scripts/native-recipe/identity.mjs';
import { verifyRecipeMaterials, verifyRecipeSources } from '../scripts/native-recipe/materials.mjs';
import { preflightNativeRecipe } from '../scripts/native-recipe/preflight.mjs';
import { fixture, hash, rekey } from './native-recipe/fixture.mjs';

test('exact typed input and tool allowlists compare bytes independently of Map enumeration', async () => {
  const { review, request } = await fixture();
  for (const reverse of [false, true]) {
    const ordered = materials => new Map(reverse ? [...materials].reverse() : materials);
    assert.equal(verifyRecipeSources(review, ordered(request.sourceMaterials)), undefined);
    assert.equal(verifyRecipeMaterials(review, {
      toolMaterials: ordered(request.toolMaterials), inputMaterials: ordered(request.inputMaterials),
    }), undefined);
    assert.throws(() => preflightNativeRecipe({ ...request,
      toolMaterials: ordered(request.toolMaterials), inputMaterials: ordered(request.inputMaterials) }),
    /TRUST-NATIVE: native appendix acceptance/);
  }
});

test('missing, extra, substituted and non-byte material reject for every declared identity', async () => {
  const { review, request } = await fixture();
  for (const [field, code] of [['toolMaterials', 'TRUST-TOOL'], ['inputMaterials', 'TRUST-NATIVE']]) {
    for (const key of request[field].keys()) {
      for (const reverse of [false, true]) {
        for (const kind of ['missing', 'extra', 'substituted', 'not-bytes']) {
          const entries = [...request[field]];
          const materials = new Map(reverse ? entries.reverse() : entries);
          if (kind === 'missing') materials.delete(key);
          if (kind === 'extra') materials.set('undeclared', Buffer.from('extra'));
          if (kind === 'substituted') materials.set(key, Buffer.from('changed content'));
          if (kind === 'not-bytes') materials.set(key, { sha256: hash(request[field].get(key)), verified: true });
          assert.throws(() => verifyRecipeMaterials(review, { ...request, [field]: materials }),
            new RegExp(`${code}:`));
        }
      }
    }
  }
});

test('source checksum co-substitution and forged review cannot issue material authority', async () => {
  const { review, request } = await fixture();
  const sources = new Map(request.sourceMaterials);
  sources.set(sources.keys().next().value, Buffer.from('replacement with adjacent checksum'));
  assert.throws(() => verifyRecipeSources(review, sources), /P361-SOURCE/);
  const forged = structuredClone(review);
  forged.bindings.executable.sha256 = hash(Buffer.from('replacement'));
  assert.throws(() => verifyRecipeMaterials(forged, request), /TRUST-NATIVE/);
});

test('changed executable bytes and adjacent approved plan require a fresh independent recipe approval', async () => {
  const { request, document } = await fixture();
  const replacement = Buffer.from('replacement cc with additional authority');
  request.toolMaterials.set('cc', replacement);
  assert.throws(() => preflightNativeRecipe(request), /TRUST-TOOL/);
  document.sourcePlan.hostTools[0].sha256 = hash(replacement);
  request.planBytes = rekey(document);
  assert.throws(() => preflightNativeRecipe(request), /TRUST-NATIVE: independent approval/);
});

test('undeclared typed source, native artifact and duplicate input do not become recipe reads', async () => {
  const { request, document } = await fixture();
  for (const input of [{ kind: 'static', id: 'ambient-lib' }, { kind: 'sysroot', id: 'system-default' },
    { kind: 'source', graphRole: 'target/runtime', package: document.sourcePlan.rootPackage, path: 'src/secret.zry' }]) {
    const changed = structuredClone(document);
    changed.nativeAppendix.compilation.steps[0].inputs.push(input);
    assert.throws(() => describeNativeRecipe(rekey(changed), request.packageAuthority, request.stepId), /P361-NATIVE/);
  }
  const changed = structuredClone(document);
  changed.nativeAppendix.compilation.steps[0].inputs.push(changed.nativeAppendix.compilation.steps[0].inputs[0]);
  assert.throws(() => describeNativeRecipe(rekey(changed), request.packageAuthority, request.stepId), /P361-SCHEMA/);
});

test('denied recipes preserve real prior state in fresh roots and never start supplied tool bytes', async () => {
  for (const reverse of [false, true]) {
    const root = await mkdtemp(path.join(os.tmpdir(), 'zryna-405-denial-'));
    try {
      const prior = Buffer.from('prior lock, cache and published output remain intact\n');
      await Promise.all(['lock', 'cache', 'output'].map(name => writeFile(path.join(root, name), prior)));
      const { request, document } = await fixture();
      const malicious = Buffer.from(`#!/bin/sh\nprintf started > '${path.join(root, 'started')}'\n`);
      request.toolMaterials.set('cc', malicious);
      document.sourcePlan.hostTools[0].sha256 = hash(malicious);
      request.planBytes = rekey(document);
      request.expectedRecipeSha256 = describeNativeRecipe(request.planBytes, request.packageAuthority, request.stepId).recipeSha256;
      if (reverse) request.toolMaterials = new Map([...request.toolMaterials].reverse());
      for (const mode of ['pure-source', 'untrusted-playground', 'trusted-native']) {
        assert.throws(() => preflightNativeRecipe({ ...request, mode }), /TRUST-(?:OPERATION|NATIVE)/);
      }
      assert.deepEqual((await readdir(root)).sort(), ['cache', 'lock', 'output']);
      for (const name of ['lock', 'cache', 'output']) assert.deepEqual(await readFile(path.join(root, name)), prior);
    } finally {
      await rm(root, { recursive: true, force: true });
    }
  }
});
