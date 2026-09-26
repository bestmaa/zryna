import { createServer } from 'node:http';
import { readFile } from 'node:fs/promises';

const page = new URL('./index.html', import.meta.url);
const loader = new URL('../../.zryna/out/add-browser.build/component/add-browser.mjs', import.meta.url);
const routes = new Map([
  ['/examples/browser-component/', [page, 'text/html; charset=utf-8']],
  ['/.zryna/out/add-browser.build/component/add-browser.mjs', [loader, 'text/javascript; charset=utf-8']],
]);

createServer(async (request, response) => {
  const route = routes.get(request.url);
  if (!route || request.method !== 'GET') {
    response.writeHead(404).end();
    return;
  }
  try {
    const [file, contentType] = route;
    const bytes = await readFile(file);
    response.writeHead(200, { 'Content-Type': contentType, 'X-Content-Type-Options': 'nosniff' });
    response.end(bytes);
  } catch {
    response.writeHead(404).end();
  }
}).listen(8000, '127.0.0.1', () => {
  console.log('Open http://127.0.0.1:8000/examples/browser-component/');
});
