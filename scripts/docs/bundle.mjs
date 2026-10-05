import { createHash } from 'node:crypto';
import { constants } from 'node:fs';
import {
  lstat,
  mkdir,
  mkdtemp,
  open,
  readdir,
  readFile,
  rename,
  rm,
  writeFile,
} from 'node:fs/promises';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

import Ajv2020 from 'ajv/dist/2020.js';
import { PLAYGROUND_CHANNEL, SEMVER, readSelectedDocsInput, revalidatePlaygroundDocsSource,
  validateProvenance, verifyGitProvenance } from './provenance.mjs';
import { retainProtectedDocsOutput } from './protected-output.mjs';
export { verifyGitProvenance } from './provenance.mjs';

const MODULE_ROOT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..', '..');
const REPOSITORY = 'https://github.com/zryna/zryna';
const MAX_DOCUMENT_BYTES = 2 * 1024 * 1024;
const MAX_TOTAL_BYTES = 32 * 1024 * 1024;
const MAX_DOCUMENTS = 512;
const NO_FOLLOW = constants.O_NOFOLLOW ?? 0;

function hash(bytes) {
  return createHash('sha256').update(bytes).digest('hex');
}

function fail(message) {
  throw new Error(`invalid documentation bundle: ${message}`);
}

function exactKeys(value, keys, label) {
  if (!value || typeof value !== 'object' || Array.isArray(value)) fail(`${label} must be an object`);
  const actual = Object.keys(value).sort();
  const expected = [...keys].sort();
  if (actual.length !== expected.length || actual.some((key, index) => key !== expected[index])) {
    fail(`${label} must contain exactly ${expected.join(', ')}`);
  }
}

function canonicalManifest(manifest) {
  const canonical = {
    schema: manifest.schema,
    version: manifest.version,
    channel: manifest.channel,
    source: {
      repository: manifest.source.repository,
      commit: manifest.source.commit,
      ref: manifest.source.ref,
      version: manifest.source.version,
    },
    documents: manifest.documents.map((document) => ({
      id: document.id,
      path: document.path,
      title: document.title,
      bytes: document.bytes,
      sha256: document.sha256,
    })),
  };
  return Buffer.from(`${JSON.stringify(canonical, null, 2)}\n`);
}

async function readBoundedRegular(filePath, maxBytes) {
  const metadata = await lstat(filePath);
  if (metadata.isSymbolicLink() || !metadata.isFile()) fail(`${filePath} is not a regular file`);
  if (metadata.size > maxBytes) fail(`${filePath} exceeds ${maxBytes} bytes`);
  let handle;
  try {
    handle = await open(filePath, constants.O_RDONLY | NO_FOLLOW);
    const before = await handle.stat();
    const bytes = await handle.readFile();
    const after = await handle.stat();
    if (
      before.dev !== metadata.dev ||
      before.ino !== metadata.ino ||
      before.size !== metadata.size ||
      before.dev !== after.dev ||
      before.ino !== after.ino ||
      before.size !== after.size ||
      before.mtimeMs !== after.mtimeMs ||
      before.ctimeMs !== after.ctimeMs
    ) {
      fail(`${filePath} changed while it was read`);
    }
    return bytes;
  } finally {
    await handle?.close();
  }
}

async function inputBytes(filePath, maximum, protectedSource) {
  return protectedSource ? readSelectedDocsInput(protectedSource, filePath, maximum)
    : readBoundedRegular(filePath, maximum);
}

async function loadJson(filePath, maxBytes = 1024 * 1024, protectedSource) {
  const bytes = await inputBytes(filePath, maxBytes, protectedSource);
  try {
    return JSON.parse(new TextDecoder('utf-8', { fatal: true }).decode(bytes));
  } catch (error) {
    fail(`${filePath} is not strict UTF-8 JSON: ${error.message}`);
  }
}

async function schemaValidator(workspaceRoot, protectedSource) {
  const schema = await loadJson(path.join(workspaceRoot, 'schemas', 'zryna-docs-bundle-v1.schema.json'),
    1024 * 1024, protectedSource);
  return new Ajv2020({ allErrors: true, strict: true }).compile(schema);
}

export async function loadRegistry(workspaceRoot = MODULE_ROOT, protectedSource) {
  const registry = await loadJson(path.join(workspaceRoot, 'docs', 'website-bundle-v1.json'),
    1024 * 1024, protectedSource);
  exactKeys(registry, ['schemaVersion', 'bundleSchema', 'documents'], 'registry');
  if (registry.schemaVersion !== 1 || registry.bundleSchema !== 'zryna.docs.bundle.v1') {
    fail('registry version or bundle schema is unsupported');
  }
  if (!Array.isArray(registry.documents) || registry.documents.length === 0) {
    fail('registry documents must be non-empty');
  }
  if (registry.documents.length > MAX_DOCUMENTS) fail('registry document budget exceeded');
  const ids = new Set();
  const sources = new Set();
  const paths = new Set();
  let previousPath = '';
  for (const [index, document] of registry.documents.entries()) {
    exactKeys(document, ['id', 'source', 'path', 'title'], `registry document #${index}`);
    for (const key of ['id', 'source', 'path', 'title']) {
      if (typeof document[key] !== 'string' || document[key].length === 0) {
        fail(`registry document #${index} ${key} must be non-empty`);
      }
    }
    if (
      path.isAbsolute(document.source) ||
      document.source.includes('\\') ||
      document.source.split('/').includes('..') ||
      !document.source.endsWith('.md')
    ) {
      fail(`registry document #${index} source is not a portable Markdown path`);
    }
    if (!document.path.startsWith('documents/') || !document.path.endsWith('.md')) {
      fail(`registry document #${index} output path is invalid`);
    }
    if (previousPath && previousPath >= document.path) fail('registry paths must be ASCII-sorted');
    previousPath = document.path;
    if (ids.has(document.id) || sources.has(document.source) || paths.has(document.path)) {
      fail('registry ids, sources, and output paths must be unique');
    }
    ids.add(document.id);
    sources.add(document.source);
    paths.add(document.path);
  }
  return registry;
}

async function sourceVersion(workspaceRoot, protectedSource) {
  const packageDocument = await loadJson(path.join(workspaceRoot, 'package.json'), 1024 * 1024, protectedSource);
  if (!SEMVER.test(packageDocument.version)) fail('package.json version is not canonical');
  return packageDocument.version;
}

async function ensureSafeOutputParent(workspaceRoot, outputPath) {
  const allowedRoot = path.resolve(workspaceRoot, '.zryna', 'out', 'docs');
  const resolvedOutput = path.resolve(outputPath);
  const relative = path.relative(allowedRoot, resolvedOutput);
  if (relative === '' || relative.startsWith('..') || path.isAbsolute(relative)) {
    fail('output must be a child of .zryna/out/docs');
  }
  let current = workspaceRoot;
  for (const segment of ['.zryna', 'out', 'docs', ...relative.split(path.sep).slice(0, -1)]) {
    current = path.join(current, segment);
    try {
      const metadata = await lstat(current);
      if (metadata.isSymbolicLink() || !metadata.isDirectory()) fail(`${current} is not a real directory`);
    } catch (error) {
      if (error.code !== 'ENOENT') throw error;
      await mkdir(current);
    }
  }
  return resolvedOutput;
}

async function scanFiles(root, relative = '', files = []) {
  const entries = await readdir(path.join(root, relative), { withFileTypes: true });
  entries.sort((left, right) => left.name.localeCompare(right.name, 'en'));
  for (const entry of entries) {
    const child = relative ? path.posix.join(relative, entry.name) : entry.name;
    const metadata = await lstat(path.join(root, ...child.split('/')));
    if (metadata.isSymbolicLink() || (!metadata.isFile() && !metadata.isDirectory())) {
      fail(`bundle entry ${child} is not a regular file or directory`);
    }
    if (metadata.isDirectory()) await scanFiles(root, child, files);
    else files.push(child);
    if (files.length > 514) fail('bundle file budget exceeded');
  }
  return files;
}

export async function validateDocsBundle(bundleRoot, expectations, workspaceRoot = MODULE_ROOT, protectedSource) {
  exactKeys(
    expectations,
    ['expectedManifestSha256', 'expectedChannel', 'expectedSourceCommit', 'expectedSourceRef'],
    'validation expectations',
  );
  const rootMetadata = await lstat(bundleRoot);
  if (rootMetadata.isSymbolicLink() || !rootMetadata.isDirectory()) fail('bundle root is not a real directory');
  const manifestBytes = await readBoundedRegular(path.join(bundleRoot, 'manifest.json'), 1024 * 1024);
  const actualManifestSha256 = hash(manifestBytes);
  if (actualManifestSha256 !== expectations.expectedManifestSha256) fail('manifest authentication failed');
  const checksum = await readBoundedRegular(path.join(bundleRoot, 'manifest.sha256'), 128);
  if (checksum.toString('utf8') !== `${actualManifestSha256}  manifest.json\n`) {
    fail('manifest checksum file is not canonical');
  }
  let manifest;
  try {
    manifest = JSON.parse(new TextDecoder('utf-8', { fatal: true }).decode(manifestBytes));
  } catch (error) {
    fail(`manifest is not strict UTF-8 JSON: ${error.message}`);
  }
  const validateSchema = await schemaValidator(workspaceRoot, protectedSource);
  if (!validateSchema(manifest)) fail(`manifest schema failed: ${validateSchema.errors[0]?.message}`);
  if (!manifestBytes.equals(canonicalManifest(manifest))) fail('manifest serialization is not canonical');
  if (
    manifest.channel !== expectations.expectedChannel ||
    manifest.source.commit !== expectations.expectedSourceCommit ||
    manifest.source.ref !== expectations.expectedSourceRef
  ) {
    fail('manifest provenance does not match authenticated expectations');
  }
  validateProvenance({
    channel: manifest.channel,
    sourceCommit: manifest.source.commit,
    sourceRef: manifest.source.ref,
    sourceVersion: manifest.source.version,
  });
  let totalBytes = 0;
  const expectedFiles = new Set(['manifest.json', 'manifest.sha256']);
  let previousPath = '';
  for (const document of manifest.documents) {
    if (previousPath && previousPath >= document.path) fail('manifest documents are not ASCII-sorted');
    previousPath = document.path;
    if (expectedFiles.has(document.path)) fail('manifest document path is duplicated');
    expectedFiles.add(document.path);
    const bytes = await readBoundedRegular(
      path.join(bundleRoot, ...document.path.split('/')),
      MAX_DOCUMENT_BYTES,
    );
    new TextDecoder('utf-8', { fatal: true }).decode(bytes);
    if (bytes.byteLength !== document.bytes || hash(bytes) !== document.sha256) {
      fail(`document ${document.path} does not match its manifest record`);
    }
    totalBytes += bytes.byteLength;
    if (totalBytes > MAX_TOTAL_BYTES) fail('aggregate document byte budget exceeded');
  }
  const actualFiles = await scanFiles(bundleRoot);
  if (
    actualFiles.length !== expectedFiles.size ||
    actualFiles.some((file) => !expectedFiles.has(file))
  ) {
    fail('bundle contains missing or unlisted files');
  }
  return { manifest, manifestSha256: actualManifestSha256 };
}

export async function exportDocsBundle({
  workspaceRoot = MODULE_ROOT,
  output,
  channel,
  sourceCommit,
  sourceRef,
  verifyGit = true,
  enforceWorkspaceOutput = true,
  protectedSource,
}) {
  const playground = channel === PLAYGROUND_CHANNEL;
  if (playground) {
    if (!verifyGit || enforceWorkspaceOutput) fail('protected playground export requires verified selection and external output');
    revalidatePlaygroundDocsSource(protectedSource, workspaceRoot, sourceCommit, sourceRef);
    const relativeOutput = path.relative(path.resolve(workspaceRoot), path.resolve(output));
    if (!relativeOutput.startsWith(`..${path.sep}`)) fail('playground output must be outside the selected source');
  } else if (protectedSource) fail('protected selection is only for the playground channel');
  const version = await sourceVersion(workspaceRoot, protectedSource);
  validateProvenance({ channel, sourceCommit, sourceRef, sourceVersion: version });
  if (verifyGit && !playground) verifyGitProvenance(workspaceRoot, sourceCommit, sourceRef);
  const registry = await loadRegistry(workspaceRoot, protectedSource);
  const documents = [];
  const contents = new Map();
  let totalBytes = 0;
  for (const item of registry.documents) {
    const bytes = await inputBytes(path.join(workspaceRoot, ...item.source.split('/')), MAX_DOCUMENT_BYTES, protectedSource);
    new TextDecoder('utf-8', { fatal: true }).decode(bytes);
    totalBytes += bytes.byteLength;
    if (totalBytes > MAX_TOTAL_BYTES) fail('aggregate document byte budget exceeded');
    contents.set(item.path, bytes);
    documents.push({
      id: item.id,
      path: item.path,
      title: item.title,
      bytes: bytes.byteLength,
      sha256: hash(bytes),
    });
  }
  const manifest = {
    schema: 'zryna.docs.bundle.v1',
    version: 1,
    channel,
    source: { repository: REPOSITORY, commit: sourceCommit, ref: sourceRef, version },
    documents,
  };
  const validateSchema = await schemaValidator(workspaceRoot, protectedSource);
  if (!validateSchema(manifest)) fail(`generated manifest schema failed: ${validateSchema.errors[0]?.message}`);
  const manifestBytes = canonicalManifest(manifest);
  const manifestSha256 = hash(manifestBytes);
  if (playground) revalidatePlaygroundDocsSource(protectedSource, workspaceRoot, sourceCommit, sourceRef);
  if (playground) {
    const documents = [...contents];
    retainProtectedDocsOutput(path.resolve(output), path.resolve(workspaceRoot),
      [...documents, ['manifest.json', manifestBytes], ['manifest.sha256', Buffer.from(`${manifestSha256}  manifest.json\n`)]],
      index => { if (index === documents.length) revalidatePlaygroundDocsSource(protectedSource, workspaceRoot, sourceCommit, sourceRef); });
    await validateDocsBundle(output, { expectedManifestSha256: manifestSha256, expectedChannel: channel,
      expectedSourceCommit: sourceCommit, expectedSourceRef: sourceRef }, workspaceRoot, protectedSource);
    revalidatePlaygroundDocsSource(protectedSource, workspaceRoot, sourceCommit, sourceRef);
    return { output: path.resolve(output), manifest, manifestSha256 };
  }
  const outputPath = enforceWorkspaceOutput
    ? await ensureSafeOutputParent(workspaceRoot, output)
    : path.resolve(output);
  try {
    await lstat(outputPath);
    fail('output already exists');
  } catch (error) {
    if (error.code !== 'ENOENT') throw error;
  }
  await mkdir(path.dirname(outputPath), { recursive: true });
  const temporary = await mkdtemp(path.join(path.dirname(outputPath), '.zryna-docs-'));
  try {
    for (const [portablePath, bytes] of contents) {
      const destination = path.join(temporary, ...portablePath.split('/'));
      await mkdir(path.dirname(destination), { recursive: true });
      await writeFile(destination, bytes, { flag: 'wx' });
    }
    await writeFile(path.join(temporary, 'manifest.json'), manifestBytes, { flag: 'wx' });
    await writeFile(
      path.join(temporary, 'manifest.sha256'),
      `${manifestSha256}  manifest.json\n`,
      { flag: 'wx' },
    );
    await validateDocsBundle(
      temporary,
      {
        expectedManifestSha256: manifestSha256,
        expectedChannel: channel,
        expectedSourceCommit: sourceCommit,
        expectedSourceRef: sourceRef,
      },
      workspaceRoot,
      protectedSource,
    );
    if (playground) revalidatePlaygroundDocsSource(protectedSource, workspaceRoot, sourceCommit, sourceRef);
    await rename(temporary, outputPath);
  } catch (error) {
    await rm(temporary, { recursive: true, force: true });
    throw error;
  }
  return { output: outputPath, manifest, manifestSha256 };
}

export const compilerWorkspaceRoot = MODULE_ROOT;
