export async function boundedBytes(path, limit, fetcher = fetch) {
  const response = await fetcher(path, { cache: 'no-store', credentials: 'omit' });
  const length = response.headers.get('content-length');
  if (!response.ok || (length !== null && Number(length) > limit) || !response.body) {
    throw new Error('Artifact unavailable or oversized');
  }
  const reader = response.body.getReader();
  const chunks = [];
  let size = 0;
  try {
    while (true) {
      const { done, value } = await reader.read();
      if (done) break;
      size += value.byteLength;
      if (size > limit) throw new Error('Artifact oversized');
      chunks.push(value);
    }
  } finally {
    await reader.cancel();
    reader.releaseLock();
  }
  const bytes = new Uint8Array(size);
  let offset = 0;
  for (const chunk of chunks) { bytes.set(chunk, offset); offset += chunk.byteLength; }
  return bytes;
}

export function verifyManifest(manifest, sourceDigest, identity) {
  if (manifest?.version !== 1 || manifest.profile !== 'zryna-browser-component-v1' ||
      manifest.command !== 'build' || manifest.entrypoint !== 'examples/universal/add.zry' ||
      manifest.stem !== 'playground-add' || manifest.source_sha256 !== sourceDigest ||
      manifest.browser?.revision !== identity.revision ||
      manifest.browser?.world !== identity.world ||
      manifest.browser?.component_sha256 !== identity.componentSha256 ||
      manifest.browser?.interface_sha256 !== identity.interfaceSha256 ||
      identity.world !== 'zryna:capability-profiles/browser@0.1.0' ||
      !Array.isArray(manifest.targets) || manifest.targets.length !== 1 ||
      manifest.targets[0] !== 'component' ||
      !Array.isArray(manifest.artifacts) || manifest.artifacts.length !== 3 ||
      manifest.artifacts[0]?.path !== 'component/playground-add.wasm' ||
      manifest.artifacts[0]?.sha256 !== identity.componentSha256 ||
      manifest.artifacts[1]?.path !== 'component/playground-add.mjs' ||
      manifest.artifacts[2]?.path !== 'component/playground-add.d.mts') {
    throw new Error('Browser bundle identity mismatch');
  }
}
