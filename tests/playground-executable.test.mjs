import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import { test } from 'node:test';
import { verifyExecutable, verifyReceipt } from '../examples/playground/restricted/executable.mjs';
import { sha256, utf8 } from '../examples/playground/restricted/limits.mjs';

// Fixed transport fixtures are not evidence of compiler admission or authenticated packaging.
async function fixture() {
  const templateBytes = new Uint8Array(await readFile(new URL('../crates/zryna-driver/src/browser_component/loader.js', import.meta.url)));
  const frame = utf8('fixed supervised-child-output');
  const authority = { compilerSha256: 'a'.repeat(64), toolkitSha256: 'b'.repeat(64),
    policySha256: 'c'.repeat(64), bindingTemplateSha256: await sha256(templateBytes) };
  const receipt = { revision: 1, sourceSha256: 'd'.repeat(64), frameSha256: await sha256(frame),
    compilerSha256: authority.compilerSha256, toolkitSha256: authority.toolkitSha256,
    policySha256: authority.policySha256 };
  const component = new Uint8Array([0, 97, 115, 109, 13, 0, 1, 0, 0, 97, 115, 109, 1, 0, 0, 0]);
  const exports = [{ logical: 'main', component: 'zryna-export-6d61696e', core: 'main', arity: 0 }];
  const compiled = { metadata: { revision: 1, sourceSha256: receipt.sourceSha256, status: 'compiled',
    identity: { revision: 'zryna.browser-component-bindings.v1', world: 'zryna:capability-profiles/browser@0.1.0',
      componentSha256: await sha256(component), interfaceSha256: 'e'.repeat(64), coreOffset: 8, coreBytes: 8 },
    exports, artifacts: [{ bytes: component.length }, { sha256: 'f'.repeat(64) }] }, artifacts: [component] };
  return { frame, authority, receipt, compiled, templateBytes };
}

test('captured compiler/toolkit/policy identity and exact child-frame receipt precede executable use', async () => {
  const f = await fixture();
  assert.doesNotThrow(() => verifyReceipt(f.receipt, f.compiled.metadata, f.authority));
  for (const mutation of [{ revision: 2 }, { sourceSha256: 'e'.repeat(64) },
    { compilerSha256: 'e'.repeat(64) }, { toolkitSha256: 'e'.repeat(64) },
    { policySha256: 'e'.repeat(64) }, { frameSha256: 'invalid' }, { extra: true }]) {
    assert.throws(() => verifyReceipt({ ...f.receipt, ...mutation }, f.compiled.metadata, f.authority));
  }
  await assert.rejects(verifyExecutable(utf8('self-consistent forged child frame'), f.compiled,
    f.receipt, f.authority, f.templateBytes), /EXECUTABLE-AUTHORITY/);
  const substitutedTemplate = f.templateBytes.slice(); substitutedTemplate[0] ^= 1;
  await assert.rejects(verifyExecutable(f.frame, f.compiled, f.receipt, f.authority,
    substitutedTemplate), /EXECUTABLE-AUTHORITY/);
});

test('dummy core-as-component and forged interface identities reject before any import', async () => {
  const f = await fixture();
  const fakeComponent = { ...f.compiled, artifacts: [new Uint8Array([0, 97, 115, 109, 1, 0, 0, 0])] };
  await assert.rejects(verifyExecutable(f.frame, fakeComponent, f.receipt, f.authority,
    f.templateBytes), /COMPONENT-HEADER/);
  await assert.rejects(verifyExecutable(f.frame, f.compiled, f.receipt, f.authority,
    f.templateBytes), /INTERFACE-IDENTITY/);
  const changed = structuredClone(f.compiled); changed.metadata.exports[0].arity = 1;
  await assert.rejects(verifyExecutable(f.frame, changed, f.receipt, f.authority,
    f.templateBytes), /INTERFACE-IDENTITY/);
});
