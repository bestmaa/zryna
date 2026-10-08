import assert from 'node:assert/strict';
import { resolve } from 'node:path';
import test from 'node:test';
import { sha256 } from '../scripts/distribution/canonical.mjs';
import {
  acquireQualificationMaterials, fetchQualificationResource,
} from '../scripts/distribution-release/acquire-qualification-materials.mjs';

const COMMIT = 'a'.repeat(40);
const WASMTIME =
  'https://raw.githubusercontent.com/bytecodealliance/wasmtime/7bac2c2775808aaec5d4aa5627a5e447b51102cf/LICENSE';

function response(url, data, changed = {}) {
  const body = new Response(data).body;
  return {
    status: 200, url, body,
    headers: new Headers({ 'content-length': String(data.length) }),
    ...changed,
  };
}

test('bounded fetch retains only one exact identity-encoded response', async () => {
  const data = Buffer.from('material bytes');
  const url = 'https://registry.npmjs.org/example/1.0.0';
  let options;
  const captured = await fetchQualificationResource({
    url, maximum: data.length, size: data.length, digest: sha256(data),
  }, { fetchImpl: async (requested, supplied) => {
    assert.equal(requested, url);
    options = supplied;
    return response(url, data);
  } });
  assert.deepEqual(captured, data);
  assert.equal(options.redirect, 'error');
  assert.equal(options.headers.accept, 'application/octet-stream');
  assert.equal(options.headers['accept-encoding'], 'identity');

  await assert.rejects(() => fetchQualificationResource({
    url, maximum: data.length - 1,
  }, { fetchImpl: async () => response(url, data) }), /response length differs/);
  await assert.rejects(() => fetchQualificationResource({
    url, maximum: data.length,
  }, { fetchImpl: async () => response(`${url}/redirected`, data) }), /response identity differs/);
});

test('bounded fetch requests and requires an exact JSON representation', async () => {
  const data = Buffer.from('{"name":"example"}');
  const url = 'https://registry.npmjs.org/example/1.0.0';
  let options;
  const descriptor = { url, maximum: data.length, size: data.length, digest: sha256(data),
    accept: 'application/json', label: 'npm metadata example@1.0.0' };
  const captured = await fetchQualificationResource(descriptor, {
    fetchImpl: async (requested, supplied) => {
      assert.equal(requested, url);
      options = supplied;
      return response(url, data, { headers: new Headers({
        'content-length': String(data.length), 'content-type': 'application/json; charset=utf-8',
      }) });
    },
  });
  assert.deepEqual(captured, data);
  assert.equal(options.headers.accept, 'application/json');
  await assert.rejects(() => fetchQualificationResource(descriptor, {
    fetchImpl: async () => response(url, Buffer.from('[object Object]'), {
      headers: new Headers({ 'content-length': '15', 'content-type': 'application/octet-stream' }),
    }),
  }), /npm metadata example@1\.0\.0: response identity differs/);
  await assert.rejects(() => fetchQualificationResource(descriptor, {
    fetchImpl: async () => response(url, Buffer.from('[object Object]'), {
      headers: new Headers({ 'content-length': '15', 'content-type': 'application/json' }),
    }),
  }), /npm metadata example@1\.0\.0: response length differs \(declared 15; expected 18\)/);
  await assert.rejects(() => fetchQualificationResource(descriptor, {
    fetchImpl: async () => response(url, data, {
      headers: new Headers({ 'content-length': '9'.repeat(512),
        'content-type': 'application/json' }),
    }),
  }), /response length differs \(declared invalid; expected 18\)/);
});

for (const sourceRef of ['refs/heads/main', 'refs/tags/v0.2.3', undefined]) {
  test(`acquires exact materials for ${sourceRef ?? 'legacy default'}`, async () => {
    const keys = Buffer.from('keys');
    const metadata = Buffer.from('metadata');
    const npmArchive = Buffer.from('npm archive');
    const nodeArchive = Buffer.from('node archive');
    const crate = Buffer.from('crate archive');
    const upstreamLicense = Buffer.from('upstream license');
    const urls = new Map([
      ['https://registry.npmjs.org/-/npm/v1/keys', keys],
      ['https://registry.npmjs.org/example/1.0.0', metadata],
      ['https://registry.npmjs.org/example/-/example-1.0.0.tgz', npmArchive],
      ['https://nodejs.org/dist/v22.22.1/node-v22.22.1-linux-x64.tar.xz', nodeArchive],
      ['https://static.crates.io/crates/example-crate/example-crate-1.2.3.crate', crate],
      [WASMTIME, upstreamLicense],
    ]);
    const fetched = [];
    const accepts = [];
    const fetchImpl = async (url, options) => {
      fetched.push(url);
      accepts.push(options.headers.accept);
      const data = urls.get(url);
      assert(data, `unexpected request ${url}`);
      return response(url, data, { headers: new Headers({
        'content-length': String(data.length), 'content-type': options.headers.accept,
      }) });
    };
    const source = new Map([
      ['LICENSE', Buffer.from('root license')],
      ['NOTICE', Buffer.from('root notice')],
      ['adapters/typescript-6/src/limits-v3.mjs', Buffer.from('limits 3')],
      ['adapters/typescript-6/src/limits-v4.mjs', Buffer.from('limits 4')],
      ['adapters/typescript-6/src/v4/boundary/configuration.mjs', Buffer.from('v4/boundary/configuration.mjs')],
      ['adapters/typescript-6/src/v4/boundary/dispatch.mjs', Buffer.from('v4/boundary/dispatch.mjs')],
      ['adapters/typescript-6/src/v4/boundary/errors.mjs', Buffer.from('v4/boundary/errors.mjs')],
      ['adapters/typescript-6/src/v4/boundary/request.mjs', Buffer.from('v4/boundary/request.mjs')],
      ['adapters/typescript-6/src/v4/boundary/transport.mjs', Buffer.from('v4/boundary/transport.mjs')],
      ['adapters/typescript-6/src/v4/syntax/constructions.mjs', Buffer.from('v4/syntax/constructions.mjs')],
      ['adapters/typescript-6/src/v4/syntax/data-declarations.mjs', Buffer.from('v4/syntax/data-declarations.mjs')],
      ['adapters/typescript-6/src/v4/syntax/diagnostics.mjs', Buffer.from('v4/syntax/diagnostics.mjs')],
      ['adapters/typescript-6/src/v4/syntax/expression-arena.mjs', Buffer.from('v4/syntax/expression-arena.mjs')],
      ['adapters/typescript-6/src/v4/syntax/expressions.mjs', Buffer.from('v4/syntax/expressions.mjs')],
      ['adapters/typescript-6/src/v4/syntax/functions.mjs', Buffer.from('v4/syntax/functions.mjs')],
      ['adapters/typescript-6/src/v4/syntax/imports.mjs', Buffer.from('v4/syntax/imports.mjs')],
      ['adapters/typescript-6/src/v4/syntax/matches.mjs', Buffer.from('v4/syntax/matches.mjs')],
      ['adapters/typescript-6/src/v4/syntax/names.mjs', Buffer.from('v4/syntax/names.mjs')],
      ['adapters/typescript-6/src/v4/syntax/source.mjs', Buffer.from('v4/syntax/source.mjs')],
      ['adapters/typescript-6/src/v4/syntax/spans.mjs', Buffer.from('v4/syntax/spans.mjs')],
      ['adapters/typescript-6/src/v4/syntax/statements.mjs', Buffer.from('v4/syntax/statements.mjs')],
      ['adapters/typescript-6/src/v4/syntax/tokens.mjs', Buffer.from('v4/syntax/tokens.mjs')],
      ['adapters/typescript-6/src/v4/syntax/types.mjs', Buffer.from('v4/syntax/types.mjs')],
      ['adapters/typescript-6/src/worker-v3.mjs', Buffer.from('worker 3')],
      ['adapters/typescript-6/src/worker-v4.mjs', Buffer.from('worker 4')],
      ['adapters/typescript-6/src/worker.mjs', Buffer.from('worker')],
    ]);
    if (sourceRef !== 'refs/heads/main') {
      for (const path of source.keys()) {
        if (path.startsWith('adapters/typescript-6/src/v4/')) source.delete(path);
      }
    }
    const object = 'b'.repeat(40);
    const spawn = (_git, args) => {
      const command = args[0];
      if (command === 'ls-tree') {
        const path = args.at(-1);
        return { status: 0, stdout: Buffer.from(`100644 blob ${object}\t${path}\0`) };
      }
      if (command === 'cat-file' && args[1] === '-s') {
        const path = [...source.keys()][spawn.sizeIndex++];
        spawn.pending = path;
        return { status: 0, stdout: Buffer.from(`${source.get(path).length}\n`) };
      }
      if (command === 'cat-file' && args[1] === 'blob') {
        return { status: 0, stdout: source.get(spawn.pending) };
      }
      assert.fail(`unexpected git command ${args.join(' ')}`);
    };
    spawn.sizeIndex = 0;
    const npmDescriptor = {
      keys: { url: [...urls.keys()][0], size: keys.length, sha256: sha256(keys) },
      packages: [{ metadata: { url: [...urls.keys()][1], size: metadata.length,
        sha256: sha256(metadata) }, tarball: { url: [...urls.keys()][2], size: npmArchive.length,
        sha256: sha256(npmArchive) } }],
    };
    const rustRecord = { name: 'example-crate', version: '1.2.3', crateSha256: sha256(crate),
      files: [{ origin: WASMTIME, size: upstreamLicense.length, sha256: sha256(upstreamLicense) }] };
    const selected = {
      npm: { path: 'licenses/typescript-LICENSE.txt', mode: 0o644, data: Buffer.from('npm') },
      node: { path: 'runtime/node/bin/node', mode: 0o755, data: Buffer.from('node') },
      rust: { path: 'licenses/rust/example-1.2.3/LICENSE', mode: 0o644, data: Buffer.from('rust') },
    };
    const adapters = {
      typeScriptMaterialDescriptors: () => npmDescriptor,
      captureTypeScriptMaterials(inputs) {
        assert.deepEqual(inputs, [{ metadata, keys, archive: npmArchive }]);
        return [selected.npm];
      },
      nodeTarget: () => ({ archive: 'node-v22.22.1-linux-x64.tar.xz',
        archiveSize: nodeArchive.length, archiveSha256: sha256(nodeArchive) }),
      async captureNodeMaterials(target, archive, capability) {
        assert.equal(target, 'x86_64-unknown-linux-gnu');
        assert.deepEqual(archive, nodeArchive);
        assert.equal(capability.name, 'selectors');
        return [selected.node];
      },
      rustMaterials: () => [rustRecord],
      captureRustMaterials(target, captures, license) {
        assert.equal(target, 'x86_64-unknown-linux-gnu');
        assert.deepEqual(captures, [{ identity: 'example-crate-1.2.3', archive: crate }]);
        assert.deepEqual(license, upstreamLicense);
        return [selected.rust];
      },
    };
    const result = await acquireQualificationMaterials({
      sourceRoot: resolve('qualification-source'), sourceCommit: COMMIT, sourceRef,
      target: 'x86_64-unknown-linux-gnu', archiveCapability: { name: 'selectors' },
      fetchImpl, spawn, adapters,
    });
    const files = result.capturedMaterials;
    assert.equal(files.length, source.size + 3);
    assert.deepEqual(result.rustCaptures, [{ identity: 'example-crate-1.2.3', archive: crate }]);
    assert.deepEqual(fetched, [...urls.keys()]);
    assert.deepEqual(accepts, [
      'application/json', 'application/json', 'application/octet-stream',
      'application/octet-stream', 'application/octet-stream', 'application/octet-stream',
    ]);
    assert.deepEqual(files.map(({ path }) => path), files.map(({ path }) => path).sort());
    assert.deepEqual(files.find(({ path }) => path === 'LICENSE').data, source.get('LICENSE'));
  });
}
