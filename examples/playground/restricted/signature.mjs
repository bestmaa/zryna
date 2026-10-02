// Startup verification executes only a separately authenticated static verifier snapshot.
import { spawnSync } from 'node:child_process';
import { closeSync, constants, fstatSync, mkdtempSync, openSync, readSync,
  rmdirSync, unlinkSync, writeFileSync } from 'node:fs';
import { isAbsolute, join, posix } from 'node:path';
import { sha256 } from '../../../scripts/distribution/canonical.mjs';
import { exact, fail, isHash } from './limits.mjs';
import { toolkitIdentity, toolkitIssuer } from './toolkit-schema.mjs';

function release(descriptors, primary) {
  const errors = primary ? [primary] : [];
  for (const fd of descriptors.reverse()) {
    try { closeSync(fd); } catch (error) { errors.push(error); }
  }
  if (errors.length === 1) throw errors[0];
  if (errors.length) throw new AggregateError(errors, 'PLAYGROUND-VERIFIER-CLEANUP');
}

export function captureVerifier(policy) {
  exact(policy, ['version', 'path', 'bytes', 'sha256', 'trustedRootBytes', 'trustedRootSha256']);
  if (policy.version !== '3.0.5' || process.platform !== 'linux' || process.arch !== 'x64' ||
      !isAbsolute(policy.path) || posix.normalize(policy.path) !== policy.path ||
      !/^\/[A-Za-z0-9._/-]+$/.test(policy.path) ||
      !Number.isSafeInteger(policy.bytes) || policy.bytes < 64 || policy.bytes > 268435456 ||
      !isHash(policy.sha256) || !isHash(policy.trustedRootSha256) ||
      !Number.isSafeInteger(policy.trustedRootBytes) || policy.trustedRootBytes < 1 ||
      policy.trustedRootBytes > 262144) fail('VERIFIER-POLICY');
  const descriptors = [];
  let primary;
  let snapshot;
  try {
    let parent = openSync('/', constants.O_RDONLY | constants.O_DIRECTORY | constants.O_NOFOLLOW);
    descriptors.push(parent);
    const parts = policy.path.slice(1).split('/');
    for (const part of parts.slice(0, -1)) {
      parent = openSync(`/proc/self/fd/${parent}/${part}`,
        constants.O_RDONLY | constants.O_DIRECTORY | constants.O_NOFOLLOW);
      descriptors.push(parent);
    }
    const fd = openSync(`/proc/self/fd/${parent}/${parts.at(-1)}`, constants.O_RDONLY | constants.O_NOFOLLOW);
    descriptors.push(fd);
    const before = fstatSync(fd, { bigint: true });
    if (!before.isFile() || before.uid !== 0n || before.size !== BigInt(policy.bytes) ||
        before.mode & 0o6022n || !(before.mode & 0o111n)) fail('VERIFIER-MATERIAL');
    snapshot = Buffer.alloc(policy.bytes);
    let offset = 0;
    while (offset < snapshot.length) {
      const count = readSync(fd, snapshot, offset, Math.min(65536, snapshot.length - offset), offset);
      if (!count) fail('VERIFIER-MATERIAL');
      offset += count;
    }
    const after = fstatSync(fd, { bigint: true });
    if (['dev', 'ino', 'size', 'mode', 'uid', 'mtimeNs', 'ctimeNs'].some(key => before[key] !== after[key]) ||
        sha256(snapshot) !== policy.sha256) fail('VERIFIER-MATERIAL');
    validateStaticVerifier(snapshot);
  } catch (error) { primary = error; }
  release(descriptors, primary);
  return snapshot;
}

export function validateStaticVerifier(input) {
  if (!Buffer.isBuffer(input) || input.length < 64 ||
      !input.subarray(0, 7).equals(Buffer.from([0x7f, 69, 76, 70, 2, 1, 1])) ||
      ![2, 3].includes(input.readUInt16LE(16)) || input.readUInt16LE(18) !== 62) fail('VERIFIER-ELF');
  const offset = Number(input.readBigUInt64LE(32));
  const width = input.readUInt16LE(54), count = input.readUInt16LE(56);
  if (!Number.isSafeInteger(offset) || offset < 64 || width !== 56 || count < 1 || count > 256 ||
      offset + width * count > input.length) fail('VERIFIER-ELF');
  for (let index = 0; index < count; index++) {
    const type = input.readUInt32LE(offset + index * width);
    // A shared-library loader or dynamic dependency would require another authenticated closure.
    if (type === 2 || type === 3) fail('VERIFIER-DYNAMIC');
  }
}

export function toolkitVerificationArguments(sourceCommit, root) {
  return verificationArguments(sourceCommit, root, false);
}

function verificationArguments(sourceCommit, root, compiler) {
  if (typeof sourceCommit !== 'string' || !/^[a-f0-9]{40}$/.test(sourceCommit)) fail('SIGNATURE-SOURCE');
  const identity = compiler ? 'https://github.com/zryna/zryna/.github/workflows/release.yml@refs/tags/v0.2.3' : toolkitIdentity;
  const ref = compiler ? 'refs/tags/v0.2.3' : 'refs/tags/playground-v0.1.0';
  return ['verify-blob', '--offline', '--new-bundle-format=true',
    '--insecure-ignore-tlog=false', '--private-infrastructure=false',
    '--insecure-ignore-sct=false', '--trusted-root', join(root, 'trusted-root.json'),
    '--bundle', join(root, 'bundle.json'), '--certificate-identity', identity,
    '--certificate-oidc-issuer', toolkitIssuer, '--certificate-github-workflow-sha', sourceCommit,
    '--certificate-github-workflow-repository', 'zryna/zryna',
    '--certificate-github-workflow-ref', ref,
    '--certificate-github-workflow-trigger', 'push', join(root, 'envelope.json')];
}

export function verifyToolkitSignature(envelope, bundle, verifierPolicy, trustedRoot, sourceCommit) {
  return verifySignature(envelope, bundle, verifierPolicy, trustedRoot, sourceCommit, false);
}

export function verifyCompilerSignature(envelope, bundle, verifierPolicy, trustedRoot, sourceCommit) {
  return verifySignature(envelope, bundle, verifierPolicy, trustedRoot, sourceCommit, true);
}

function verifySignature(envelope, bundle, verifierPolicy, trustedRoot, sourceCommit, compiler) {
  if (!(envelope instanceof Uint8Array) || envelope.length < 1 || envelope.length > 262144 ||
      !(bundle instanceof Uint8Array) || bundle.length < 1 || bundle.length > 1048576 ||
      !(trustedRoot instanceof Uint8Array) || trustedRoot.length !== verifierPolicy.trustedRootBytes ||
      sha256(trustedRoot) !== verifierPolicy.trustedRootSha256) fail('SIGNATURE-MATERIAL');
  // Validate the independently selected source before acquiring any executable.
  verificationArguments(sourceCommit, '/tmp', compiler);
  const executable = captureVerifier(verifierPolicy);
  const root = mkdtempSync('/tmp/zryna-playground-verify-');
  const owned = [];
  let primary;
  try {
    for (const [name, data, mode] of [['cosign', executable, 0o500], ['envelope.json', envelope, 0o400],
      ['bundle.json', bundle, 0o400], ['trusted-root.json', trustedRoot, 0o400]]) {
      const path = join(root, name);
      const descriptor = openSync(path, constants.O_WRONLY | constants.O_CREAT | constants.O_EXCL, mode);
      owned.push(path);
      let writeError;
      try { writeFileSync(descriptor, data); } catch (error) { writeError = error; }
      release([descriptor], writeError);
    }
    const result = spawnSync(join(root, 'cosign'), verificationArguments(sourceCommit, root, compiler),
      { shell: false, cwd: root, env: { LANG: 'C.UTF-8', HOME: root },
      timeout: 20000, killSignal: 'SIGKILL', maxBuffer: 65536 });
    if (result.error || result.status !== 0 || result.signal ||
        (result.stdout?.length ?? 0) + (result.stderr?.length ?? 0) > 65536) fail('SIGNATURE');
  } catch (error) { primary = error; }
  const errors = primary ? [primary] : [];
  for (const path of owned.reverse()) {
    try { unlinkSync(path); } catch (error) { errors.push(error); }
  }
  try { rmdirSync(root); } catch (error) { errors.push(error); }
  if (errors.length === 1) throw errors[0];
  if (errors.length) throw new AggregateError(errors, 'PLAYGROUND-SIGNATURE-CLEANUP');
}
