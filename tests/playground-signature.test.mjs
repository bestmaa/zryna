import assert from 'node:assert/strict';
import { test } from 'node:test';
import { captureVerifier, toolkitVerificationArguments, validateStaticVerifier, verifyToolkitSignature } from '../examples/playground/restricted/signature.mjs';
import { authenticateToolkit, toolkitFile, toolkitMaterials, toolkitNestedCompiler } from '../examples/playground/restricted/toolkit.mjs';

// Synthetic ELF/header and rejected-admission units never run a verification executable.
function elf() {
  const result = Buffer.alloc(120);
  Buffer.from([0x7f, 69, 76, 70, 2, 1, 1]).copy(result);
  result.writeUInt16LE(2, 16); result.writeUInt16LE(62, 18);
  result.writeBigUInt64LE(64n, 32); result.writeUInt16LE(56, 54); result.writeUInt16LE(1, 56);
  result.writeUInt32LE(1, 64);
  return result;
}

test('fixed verifier header policy permits only bounded static x86-64 ELF topology', () => {
  assert.doesNotThrow(() => validateStaticVerifier(elf()));
  for (const change of [buffer => buffer[0] = 0, buffer => buffer[4] = 1,
    buffer => buffer[5] = 2, buffer => buffer[6] = 0,
    buffer => buffer.writeUInt16LE(1, 16), buffer => buffer.writeUInt16LE(183, 18),
    buffer => buffer.writeBigUInt64LE(9007199254740992n, 32),
    buffer => buffer.writeBigUInt64LE(63n, 32), buffer => buffer.writeUInt16LE(55, 54),
    buffer => buffer.writeUInt16LE(0, 56), buffer => buffer.writeUInt16LE(257, 56),
    buffer => buffer.writeUInt32LE(2, 64), buffer => buffer.writeUInt32LE(3, 64)]) {
    const input = elf(); change(input);
    assert.throws(() => validateStaticVerifier(input), /PLAYGROUND-VERIFIER-/);
  }
  for (const input of [new Uint8Array(120), Buffer.alloc(63), elf().subarray(0, 119)]) {
    assert.throws(() => validateStaticVerifier(input), /PLAYGROUND-VERIFIER-/);
  }
});

test('verifier capture rejects malformed independent acquisition pins before opening or running', () => {
  const base = { version: '3.0.5', path: '/reviewed/cosign', bytes: 120, sha256: 'a'.repeat(64),
    trustedRootBytes: 1, trustedRootSha256: 'b'.repeat(64) };
  for (const change of [{ path: 'relative' }, { path: '/reviewed/../cosign' },
    { path: '/reviewed//cosign' }, { path: '/reviewed/cosign\0' }, { bytes: 63 },
    { bytes: 268435457 }, { bytes: 1.5 }, { sha256: 'A'.repeat(64) },
    { trustedRootBytes: 0 }, { trustedRootBytes: 262145 }, { trustedRootSha256: '' },
    { version: '3.0.4' }, { extra: true }]) {
    assert.throws(() => captureVerifier({ ...base, ...change }), /PLAYGROUND-/);
  }
});

test('signature policy selects exact authenticated workflow source claims with all trust checks enabled', () => {
  const commit = 'a'.repeat(40);
  const args = toolkitVerificationArguments(commit, '/tmp/private-signature');
  for (const [flag, expected] of [['--certificate-github-workflow-sha', commit],
    ['--certificate-github-workflow-repository', 'zryna/zryna'],
    ['--certificate-github-workflow-ref', 'refs/tags/playground-v0.1.0'],
    ['--certificate-github-workflow-trigger', 'push'],
    ['--certificate-identity', 'https://github.com/zryna/zryna/.github/workflows/playground-release.yml@refs/tags/playground-v0.1.0'],
    ['--certificate-oidc-issuer', 'https://token.actions.githubusercontent.com']]) {
    assert.equal(args.filter(value => value === flag).length, 1);
    assert.equal(args[args.indexOf(flag) + 1], expected);
  }
  for (const flag of ['--offline', '--new-bundle-format=true', '--insecure-ignore-tlog=false',
    '--private-infrastructure=false', '--insecure-ignore-sct=false']) assert.ok(args.includes(flag));
  assert.ok(!args.includes('--key'));
  for (const source of [null, '', 'A'.repeat(40), 'a'.repeat(39), 'a'.repeat(41)]) {
    assert.throws(() => toolkitVerificationArguments(source, '/tmp/private-signature'), /SIGNATURE-SOURCE/);
  }
});

test('signature carrier and root mismatch reject without acquiring an executable', () => {
  const policy = { trustedRootBytes: 1, trustedRootSha256: 'a'.repeat(64) };
  for (const [envelope, bundle, root] of [[new Uint8Array(), new Uint8Array(1), new Uint8Array(1)],
    [new Uint8Array(262145), new Uint8Array(1), new Uint8Array(1)],
    [new Uint8Array(1), new Uint8Array(), new Uint8Array(1)],
    [new Uint8Array(1), new Uint8Array(1048577), new Uint8Array(1)],
    [new Uint8Array(1), new Uint8Array(1), new Uint8Array(2)],
    [new Uint8Array(1), new Uint8Array(1), new Uint8Array(1)]]) {
    assert.throws(() => verifyToolkitSignature(envelope, bundle, policy, root), /PLAYGROUND-SIGNATURE-MATERIAL/);
  }
});

test('plain metadata and forged object identities cannot manufacture a retained toolkit capability', () => {
  const fake = Object.freeze({ authority: { compilerSha256: 'a'.repeat(64) }, sourceCommit: 'b'.repeat(40) });
  assert.throws(() => toolkitFile(fake, 'bin/zryna-playground-compiler'), /PLAYGROUND-TOOLKIT-CAPABILITY/);
  assert.throws(() => toolkitMaterials(fake), /PLAYGROUND-TOOLKIT-CAPABILITY/);
  assert.throws(() => toolkitNestedCompiler(fake), /PLAYGROUND-TOOLKIT-CAPABILITY/);
});

test('toolkit rejects first extra transport bytes before copying or starting signature verification', async () => {
  const base = { envelope: new Uint8Array(1), bundle: new Uint8Array(1), archive: new Uint8Array(1),
    trustedRoot: new Uint8Array(1), policy: null, verifierPolicy: null };
  for (const change of [{ envelope: new Uint8Array(262145) }, { bundle: new Uint8Array(1048577) },
    { trustedRoot: new Uint8Array(262145) }, { envelope: new Uint8Array() },
    { archive: new Uint8Array() }, { archive: 'not bytes' }]) {
    await assert.rejects(authenticateToolkit({ ...base, ...change }), /PLAYGROUND-TOOLKIT-BYTES/);
  }
});
