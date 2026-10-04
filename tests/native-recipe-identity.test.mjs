import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import test from 'node:test';

import { describeNativeRecipe, reviewedDocument } from '../scripts/native-recipe/identity.mjs';
import { preflightNativeRecipe } from '../scripts/native-recipe/preflight.mjs';
import { fixture, rekey, wire } from './native-recipe/fixture.mjs';

test('review identity uses independent canonical hashing and deeply immutable plan bindings', async () => {
  const { review, request, document } = await fixture();
  const independent = createHash('sha256').update('ZRYNA-NATIVE-RECIPE-PREFLIGHT-V0\0review\0')
    .update(wire(review.bindings)).digest('hex');
  assert.equal(review.recipeSha256, independent);
  assert.equal(review.bindings.executable.name, 'cc');
  assert.equal(review.bindings.objectOutput, 'sample-object');
  assert.equal(review.bindings.invocation.adapter, 'zryna.native-compile-adapter.v0');
  assert.equal(review.bindings.inputAllowlist.length, 3);
  assert.throws(() => { review.bindings.executable.sha256 = '0'.repeat(64); }, TypeError);
  assert.throws(() => { review.bindings.inputAllowlist.pop(); }, TypeError);
  request.planBytes.fill(0);
  document.sourcePlan.hostTools[0].name = 'replaced';
  assert.equal(review.bindings.executable.name, 'cc');
  assert.throws(() => reviewedDocument(structuredClone(review)), /TRUST-NATIVE/);
});

test('unknown caller mode wins before malformed plan or package-selected authority', async () => {
  const { request } = await fixture();
  for (const mode of [undefined, '', 'TRUSTED-NATIVE', 'unrestricted-local', {}, 'trusted-native\0']) {
    assert.throws(() => preflightNativeRecipe({ ...request, mode, planBytes: Buffer.from('invalid') }), /TRUST-MODE/);
  }
  for (const mode of ['pure-source', 'untrusted-playground']) {
    assert.throws(() => preflightNativeRecipe({ ...request, mode }), /TRUST-OPERATION/);
  }
});

test('malformed source authority and plan retain earlier #168/#361 rejection', async () => {
  const { request, document } = await fixture();
  assert.throws(() => preflightNativeRecipe({ ...request, packageAuthority: structuredClone(request.packageAuthority) }), /P361-IDENTITY/);
  assert.throws(() => preflightNativeRecipe({ ...request, planBytes: Buffer.from(` ${request.planBytes}`) }), /P361-WIRE/);
  document.sourcePlan.compiler.sha256 = '0'.repeat(64);
  assert.throws(() => preflightNativeRecipe({ ...request, planBytes: rekey(document) }), /P361-TOOLCHAIN/);
});

test('absent recipe, raw arguments, path executable, output escape and accepted status fail closed', async () => {
  const { request, document } = await fixture();
  assert.throws(() => preflightNativeRecipe({ ...request, stepId: 'undeclared' }), /TRUST-NATIVE/);
  const changes = [
    doc => { doc.nativeAppendix.compilation.steps[0].invocation.arguments = ['-include', '/secret']; },
    doc => { doc.nativeAppendix.compilation.steps[0].tool = '/bin/sh'; },
    doc => { doc.nativeAppendix.linking.output = '../escape.elf'; },
    doc => { doc.nativeAppendix.status = 'accepted'; },
    doc => { doc.nativeAppendix.compilation.steps[0].invocation.mode = 'shell'; },
  ];
  for (const change of changes) {
    const changed = structuredClone(document);
    change(changed);
    assert.throws(() => preflightNativeRecipe({ ...request, planBytes: rekey(changed) }), /P361-SCHEMA/);
  }
});

test('approval cannot be transplanted after any plan or invocation identity changes', async () => {
  const { request, document, review } = await fixture();
  const changes = [
    doc => { doc.sourcePlan.executionPolicy.configurationSha256 = '0'.repeat(64); },
    doc => { doc.sourcePlan.host.environment[1].value = '1'; },
    doc => { doc.nativeAppendix.abi.ownershipModel = 'different-resources'; },
    doc => { doc.nativeAppendix.compilation.steps[0].output = 'other-object';
      doc.nativeAppendix.linking.inputs[0].id = 'other-object'; },
    doc => { doc.nativeAppendix.linking.inputs.reverse(); },
    doc => { doc.sourcePlan.outputs[1].path = 'native/other.elf';
      doc.nativeAppendix.linking.output = 'native/other.elf'; },
    doc => { doc.sourcePlan.hostTools[1].version = '0.2.0'; },
  ];
  for (const change of changes) {
    const changed = structuredClone(document);
    change(changed);
    const planBytes = rekey(changed);
    const next = describeNativeRecipe(planBytes, request.packageAuthority, request.stepId);
    assert.notEqual(next.recipeSha256, review.recipeSha256);
    assert.throws(() => preflightNativeRecipe({ ...request, planBytes }), /TRUST-NATIVE: independent approval/);
  }
  for (const expectedRecipeSha256 of [undefined, '', 'A'.repeat(64), '0'.repeat(64)]) {
    assert.throws(() => preflightNativeRecipe({ ...request, expectedRecipeSha256 }), /TRUST-NATIVE: independent approval/);
  }
});

test('exact review and approval, including forged FFI/isolation claims, grant no execution', async () => {
  const { request } = await fixture();
  for (const claims of [{}, { isolation: true, ffiConformance: true, appendixAccepted: true },
    { sandbox: { network: 'disabled', filesystem: 'isolated' }, authenticated: true }]) {
    assert.throws(() => preflightNativeRecipe({ ...request, ...claims }), /TRUST-NATIVE: native appendix acceptance/);
  }
});
