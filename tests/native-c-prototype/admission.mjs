import { checkDesign } from '../native-c-abi-v0/check-design.mjs';
import { inspectSources, scalarPrototypeHeader } from './declarations.mjs';

// Explicit test inputs only. No ambient source/header discovery, backend, linker or selector.
export function inspectPrototype(bytes, sources, header, target = 'x86_64-unknown-linux-gnu') {
  const declarationDigest = checkDesign(bytes, sources, header);
  const document = JSON.parse(Buffer.from(bytes).toString('utf8'));
  const source = inspectSources(document, sources, target);
  return { ...source, declarationDigest, header: scalarPrototypeHeader(document) };
}
