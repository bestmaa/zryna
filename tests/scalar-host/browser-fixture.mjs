import { createHash } from 'node:crypto';
import { createRequire } from 'node:module';
import { readFile, lstat, opendir, open, mkdir, rm } from 'node:fs/promises';
import path from 'node:path';
import { verifyFixtureBinding } from './binding.mjs';

const require = createRequire(import.meta.url);
const sha256 = bytes => createHash('sha256').update(bytes).digest('hex');
const pin = JSON.parse(await readFile(new URL('./browser-pin.json', import.meta.url), 'utf8'));

export async function verifyBrowser(root) {
  const selected = pin.platforms[`${process.platform}-${process.arch}`];
  if (!selected || !/^[a-f0-9]{64}$/.test(selected.archiveSha256 ?? '') ||
      !/^[a-f0-9]{64}$/.test(selected.inventorySha256 ?? '')) {
    throw new Error('real-browser fixture has no reviewed platform/integrity pin');
  }
  if (typeof root !== 'string' || !path.isAbsolute(root)) throw new Error('absolute fixture root required');
  const inventoryPath = path.join(root, '..', 'inventory.json');
  const inventoryState = await lstat(inventoryPath);
  if (!inventoryState.isFile() || inventoryState.isSymbolicLink() || inventoryState.size > 8 * 1024 * 1024) {
    throw new Error('unsafe or oversized browser inventory');
  }
  const inventoryBytes = await readFile(inventoryPath);
  if (inventoryBytes.length > 8 * 1024 * 1024 || sha256(inventoryBytes) !== selected.inventorySha256) {
    throw new Error('reviewed browser inventory differs');
  }
  const inventory = JSON.parse(inventoryBytes);
  if (!Array.isArray(inventory) || inventory.length > 30000) throw new Error('invalid inventory');
  const actual = [];
  let total = 0;
  let visited = 0;
  async function walk(directory, depth) {
    if (depth > 32) throw new Error('browser fixture depth exceeded');
    const state = await lstat(directory);
    if (!state.isDirectory() || state.isSymbolicLink()) throw new Error('unsafe browser directory');
    for await (const entry of await opendir(directory)) {
      if (++visited > 30000) throw new Error('browser fixture entry count exceeded');
      const name = entry.name;
      const file = path.join(directory, name);
      const before = await lstat(file);
      if (before.isSymbolicLink()) throw new Error('browser fixture link rejected');
      if (before.isDirectory()) { await walk(file, depth + 1); continue; }
      if (!before.isFile() || actual.length >= 30000) throw new Error('invalid browser entry');
      total += before.size;
      if (total > 1024 * 1024 * 1024) throw new Error('browser fixture size exceeded');
      const handle = await open(file, 'r');
      let hash;
      try {
        const opened = await handle.stat();
        if (opened.ino !== before.ino || opened.dev !== before.dev) throw new Error('changed file');
        const digest = createHash('sha256');
        let bytes = 0;
        for await (const chunk of handle.createReadStream({ autoClose: false })) {
          bytes += chunk.length;
          if (bytes > before.size) throw new Error('browser file grew during verification');
          digest.update(chunk);
        }
        const after = await handle.stat();
        if (after.size !== before.size || after.mtimeMs !== before.mtimeMs ||
            after.ctimeMs !== before.ctimeMs) throw new Error('mutable browser fixture');
        hash = digest.digest('hex');
      } finally { await handle.close(); }
      actual.push({ path: path.relative(root, file).split(path.sep).join('/'),
        bytes: before.size, sha256: hash });
    }
  }
  await walk(root, 0);
  actual.sort((a, b) => a.path < b.path ? -1 : a.path > b.path ? 1 : 0);
  if (JSON.stringify(actual) !== JSON.stringify(inventory)) throw new Error('browser bytes differ');
  if (!actual.some(entry => /license|notice|credits|\/ABOUT$/i.test(entry.path))) throw new Error('missing notices');
  return path.join(root, selected.executable);
}

export async function runBrowserFixture(input) {
  if (process.version !== 'v22.22.1') throw new Error('pinned Node required');
  verifyFixtureBinding(input, 'js-browser');
  const executablePath = await verifyBrowser(input.browserRoot);
  if (typeof input.runtimeDirectory !== 'string' || !path.isAbsolute(input.runtimeDirectory)) {
    throw new Error('absolute private runtime directory required');
  }
  await mkdir(input.runtimeDirectory, { mode: 0o700 });
  for (const name of ['TMPDIR', 'TMP', 'TEMP']) process.env[name] = input.runtimeDirectory;
  if (require('playwright-core/package.json').version !== pin.runnerVersion) {
    throw new Error('browser runner version differs');
  }
  const { chromium } = require('playwright-core');
  let browser;
  let context;
  let page;
  let requests = 0;
  let workers = 0;
  let report;
  try {
    browser = await chromium.launch({ executablePath, headless: true, chromiumSandbox: true,
      timeout: 10000, args: ['--disable-background-networking'] });
    if (browser.version() !== pin.browserVersion) throw new Error('browser version differs');
    context = await browser.newContext({ serviceWorkers: 'block' });
    await context.route('**/*', route => {
      if (route.request().isNavigationRequest() && route.request().url() === 'https://scalar.invalid/') {
        return route.fulfill({ contentType: 'text/html', body: '<!doctype html><title>Scalar fixture</title>' });
      }
      requests++;
      return route.abort();
    });
    page = await context.newPage();
    page.on('worker', () => { workers++; });
    await page.goto('https://scalar.invalid/', { timeout: 5000 });
    report = await page.evaluate(async fixture => {
      const bytes = new TextEncoder().encode(fixture.source);
      const digest = Array.from(new Uint8Array(await crypto.subtle.digest('SHA-256', bytes)))
        .map(byte => byte.toString(16).padStart(2, '0')).join('');
      if (digest !== fixture.artifactSha256) throw new Error('browser artifact substitution');
      const call = (0, eval)(`${fixture.call};scalarCall`);
      const corpus = (0, eval)(`${fixture.corpus};scalarCorpus`);
      const url = URL.createObjectURL(new Blob([bytes], { type: 'text/javascript' }));
      try {
        const module = await import(url);
        return { ...corpus(module, fixture.exports, call), artifactSha256: digest,
          interfaceSha256: fixture.interfaceSha256, bindingSha256: fixture.bindingSha256 };
      } finally { URL.revokeObjectURL(url); }
    }, input);
    if (requests !== 0 || workers !== 0) throw new Error('unexpected fixture request or worker');
  } finally {
    const failures = [];
    for (const resource of [page, context, browser]) {
      if (resource) {
        try { await resource.close(); } catch (error) { failures.push(error); }
      }
    }
    try { await rm(input.runtimeDirectory, { recursive: true }); } catch (error) { failures.push(error); }
    if (failures.length) throw new Error('browser fixture cleanup could not be confirmed');
  }
  await verifyBrowser(input.browserRoot);
  return { ...report, browserVersion: pin.browserVersion, runnerVersion: pin.runnerVersion,
    requests, workers, closed: true };
}
