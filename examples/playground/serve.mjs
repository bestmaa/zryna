import { createServer } from 'node:http';
import { readFile, stat } from 'node:fs/promises';
import { createHash } from 'node:crypto';

const bundle = new URL('../../.zryna/out/playground-add.build/', import.meta.url);
const artifactNames = ['component/playground-add.wasm',
  'component/playground-add.mjs', 'component/playground-add.d.mts'];
const limits = [2 * 1024 * 1024, 65536, 65536];

function digest(bytes) { return createHash('sha256').update(bytes).digest('hex'); }

const routes = new Map([
  ['/examples/playground/', ['index.html', 'text/html; charset=utf-8', 16384]],
  ['/examples/playground/app.mjs', ['app.mjs', 'text/javascript; charset=utf-8', 16384]],
  ['/examples/playground/controller.mjs', ['controller.mjs', 'text/javascript; charset=utf-8', 16384]],
  ['/examples/playground/corpus.mjs', ['corpus.mjs', 'text/javascript; charset=utf-8', 8192]],
  ['/examples/playground/worker.mjs', ['worker.mjs', 'text/javascript; charset=utf-8', 16384]],
  ['/examples/playground/transport.mjs', ['transport.mjs', 'text/javascript; charset=utf-8', 16384]],
  ['/examples/universal/add.zry', ['../universal/add.zry', 'text/plain; charset=utf-8', 4096]],
  ['/.zryna/out/playground-add.build/zryna-browser-manifest-v1.json',
    ['../../.zryna/out/playground-add.build/zryna-browser-manifest-v1.json',
      'application/json; charset=utf-8', 16384]],
  ['/.zryna/out/playground-add.build/component/playground-add.mjs',
    ['../../.zryna/out/playground-add.build/component/playground-add.mjs',
      'text/javascript; charset=utf-8', 65536]],
  ['/.zryna/out/playground-add.build/component/playground-add.wasm',
    ['../../.zryna/out/playground-add.build/component/playground-add.wasm',
      'application/wasm', 2 * 1024 * 1024]],
]);

// Retain exactly the checked bytes so a later file replacement cannot change a response.
const retained = new Map();
for (const [route, [relativePath, , limit]] of routes) {
  const path = new URL(relativePath, import.meta.url);
  if ((await stat(path)).size > limit) throw new Error('Playground input oversized');
  const bytes = await readFile(path);
  if (bytes.length > limit) throw new Error('Playground input oversized');
  retained.set(route, bytes);
}
const bundleRoute = '/.zryna/out/playground-add.build/';
const manifest = JSON.parse(retained.get(`${bundleRoute}zryna-browser-manifest-v1.json`));
const source = retained.get('/examples/universal/add.zry');
if (manifest.version !== 1 || manifest.profile !== 'zryna-browser-component-v1' ||
    manifest.command !== 'build' || manifest.entrypoint !== 'examples/universal/add.zry' ||
    manifest.stem !== 'playground-add' || manifest.source_sha256 !== digest(source) ||
    !Array.isArray(manifest.artifacts) || manifest.artifacts.length !== 3) {
  throw new Error('Browser bundle manifest does not match the fixed source');
}
for (const [index, name] of artifactNames.entries()) {
  let bytes = retained.get(`${bundleRoute}${name}`);
  if (!bytes) {
    const path = new URL(name, bundle);
    if ((await stat(path)).size > limits[index]) throw new Error('Browser artifact oversized');
    bytes = await readFile(path);
  }
  if (bytes.length > limits[index] || manifest.artifacts[index]?.path !== name ||
      manifest.artifacts[index]?.sha256 !== digest(bytes)) {
    throw new Error('Browser bundle artifact does not match its manifest');
  }
}
if (manifest.browser?.component_sha256 !== manifest.artifacts[0].sha256 ||
    manifest.browser?.world !== 'zryna:capability-profiles/browser@0.1.0') {
  throw new Error('Browser bundle identity mismatch');
}

const securityHeaders = {
  'Content-Security-Policy': "default-src 'none'; script-src 'self' 'wasm-unsafe-eval'; worker-src 'self'; connect-src 'self'; base-uri 'none'; form-action 'none'",
  'Cross-Origin-Resource-Policy': 'same-origin',
  'Referrer-Policy': 'no-referrer',
  'X-Content-Type-Options': 'nosniff',
  'Cache-Control': 'no-store',
};

createServer((request, response) => {
  const route = routes.get(request.url);
  if (request.method !== 'GET' || !route) {
    response.writeHead(404, securityHeaders).end();
    return;
  }
  const bytes = retained.get(request.url);
  response.writeHead(200, { ...securityHeaders, 'Content-Type': route[1],
    'Content-Length': bytes.length });
  response.end(bytes);
}).listen(8001, '127.0.0.1', () => {
  console.log('Open http://127.0.0.1:8001/examples/playground/');
});
