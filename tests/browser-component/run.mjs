import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import { pathToFileURL } from 'node:url';
import { createRequire } from 'node:module';
import { verifyBrowser } from '../scalar-host/browser-fixture.mjs';

const require = createRequire(import.meta.url);
const pin = JSON.parse(await readFile(new URL('../scalar-host/browser-pin.json', import.meta.url)));
const [loaderPath, componentPath, browserRoot] = process.argv.slice(2);
const loader = await readFile(loaderPath, 'utf8');
const bytes = await readFile(componentPath);
const examplePage = await readFile(new URL('../../examples/browser-component/index.html', import.meta.url), 'utf8');
const module = await import(pathToFileURL(loaderPath));
const hostileSource = loader.replace('"logical":"add"', '"logical":"odd"');
assert.notEqual(hostileSource, loader, 'binding fixture must alter one sealed export');

async function corpus(instantiate, componentBytes) {
  const component = await instantiate(componentBytes);
  const actual = [component.add(20, 22), component.add(2147483647, 1),
    component.add(-2147483648, -1), component.add(0, 0), component.add(-7, 2)];
  const rejected = [];
  for (const args of [['20', 22], [20.5, 22], [-0, 22], [null, 22],
    [undefined, 22], [20n, 22], [new Number(20), 22], [20], [20, 22, 0]]) {
    try { component.add(...args); rejected.push(false); }
    catch (error) { rejected.push(error instanceof TypeError || error instanceof RangeError); }
  }
  const substituted = new Uint8Array(componentBytes);
  substituted[substituted.length - 1] ^= 1;
  try { await instantiate(substituted); rejected.push(false); }
  catch (error) { rejected.push(error.message === 'ZRYNA-B3992'); }
  try { await instantiate(new Uint8Array(componentBytes.length + 1)); rejected.push(false); }
  catch (error) { rejected.push(error.message === 'ZRYNA-B3991'); }
  try { await instantiate(new (class extends Uint8Array {})(componentBytes)); rejected.push(false); }
  catch (error) { rejected.push(error.message === 'ZRYNA-B3991'); }
  try { await instantiate(componentBytes, { deadlineMs: 0 }); rejected.push(false); }
  catch (error) { rejected.push(error.message === 'ZRYNA-B3993'); }
  return { actual, rejected };
}

const node = await corpus(module.instantiateBrowserComponent, new Uint8Array(bytes));
const hostileNode = await import(`data:text/javascript;base64,${Buffer.from(hostileSource).toString('base64')}`);
try { await hostileNode.instantiateBrowserComponent(new Uint8Array(bytes)); node.rejected.push(false); }
catch (error) { node.rejected.push(error.message === 'ZRYNA-B3995'); }
assert.deepEqual(node, { actual: [42, -2147483648, 2147483647, 0, -5],
  rejected: Array(14).fill(true) });
if (browserRoot) {
  const executablePath = await verifyBrowser(browserRoot);
  assert.equal(require('playwright-core/package.json').version, pin.runnerVersion);
  const { chromium } = require('playwright-core');
  let browser;
  let context;
  let page;
  let unexpected = 0;
  try {
    browser = await chromium.launch({ executablePath, headless: true, chromiumSandbox: true,
      timeout: 10000, args: ['--disable-background-networking'] });
    assert.equal(browser.version(), pin.browserVersion);
    context = await browser.newContext({ serviceWorkers: 'block' });
    await context.route('**/*', route => {
      if (route.request().isNavigationRequest() && route.request().url() === 'https://browser.invalid/') {
        return route.fulfill({ contentType: 'text/html', body: '<!doctype html><title>Browser component</title>' });
      }
      if (route.request().isNavigationRequest() &&
          route.request().url() === 'https://browser.invalid/examples/browser-component/') {
        return route.fulfill({ contentType: 'text/html', body: examplePage });
      }
      if (route.request().url() ===
          'https://browser.invalid/.zryna/out/add-browser.build/component/add-browser.mjs') {
        return route.fulfill({ contentType: 'text/javascript', body: loader });
      }
      unexpected++;
      return route.abort();
    });
    page = await context.newPage();
    page.on('worker', () => { unexpected++; });
    await page.goto('https://browser.invalid/');
    const browserResult = await page.evaluate(async ({ source, hostileSource, component }) => {
      const url = URL.createObjectURL(new Blob([source], { type: 'text/javascript' }));
      try {
        const loaded = await import(url);
        const bytes = new Uint8Array(component);
        const instance = await loaded.instantiateBrowserComponent(bytes);
        const actual = [instance.add(20, 22), instance.add(2147483647, 1),
          instance.add(-2147483648, -1), instance.add(0, 0), instance.add(-7, 2)];
        const rejected = [];
        for (const args of [['20', 22], [20.5, 22], [-0, 22], [null, 22],
          [undefined, 22], [20n, 22], [new Number(20), 22], [20], [20, 22, 0]]) {
          try { instance.add(...args); rejected.push(false); }
          catch (error) { rejected.push(error instanceof TypeError || error instanceof RangeError); }
        }
        const substituted = new Uint8Array(bytes);
        substituted[substituted.length - 1] ^= 1;
        try { await loaded.instantiateBrowserComponent(substituted); rejected.push(false); }
        catch (error) { rejected.push(error.message === 'ZRYNA-B3992'); }
        try { await loaded.instantiateBrowserComponent(new Uint8Array(bytes.length + 1)); rejected.push(false); }
        catch (error) { rejected.push(error.message === 'ZRYNA-B3991'); }
        try { await loaded.instantiateBrowserComponent(new (class extends Uint8Array {})(bytes)); rejected.push(false); }
        catch (error) { rejected.push(error.message === 'ZRYNA-B3991'); }
        try { await loaded.instantiateBrowserComponent(bytes, { deadlineMs: 0 }); rejected.push(false); }
        catch (error) { rejected.push(error.message === 'ZRYNA-B3993'); }
        const hostileUrl = URL.createObjectURL(new Blob([hostileSource], { type: 'text/javascript' }));
        try {
          const hostile = await import(hostileUrl);
          try { await hostile.instantiateBrowserComponent(bytes); rejected.push(false); }
          catch (error) { rejected.push(error.message === 'ZRYNA-B3995'); }
        } finally { URL.revokeObjectURL(hostileUrl); }
        return { actual, rejected };
      } finally { URL.revokeObjectURL(url); }
    }, { source: loader, hostileSource, component: Array.from(bytes) });
    assert.deepEqual(browserResult, node);
    await page.goto('https://browser.invalid/examples/browser-component/');
    await page.setInputFiles('#component', { name: 'add-browser.wasm',
      mimeType: 'application/wasm', buffer: bytes });
    await page.waitForFunction(() => document.querySelector('#result')?.textContent?.includes('"value"'),
      undefined, { timeout: 5000 });
    assert.deepEqual(JSON.parse(await page.locator('#result').textContent()),
      { value: 42, rejected: [true, true, true, true] });
    assert.equal(unexpected, 0);
  } finally {
    const failures = [];
    for (const resource of [page, context, browser]) if (resource) {
      try { await resource.close(); } catch (error) { failures.push(error); }
    }
    if (failures.length) throw new Error('browser fixture cleanup could not be confirmed');
  }
  await verifyBrowser(browserRoot);
}
console.log(JSON.stringify({ node, browser: Boolean(browserRoot) }));
