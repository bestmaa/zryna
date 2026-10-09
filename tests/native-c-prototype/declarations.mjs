import { createHash } from 'node:crypto';
import { allowedExportName, parseSource, reject } from './source.mjs';
import { inspectFunction } from './resources.mjs';

const digest = bytes => createHash('sha256').update(bytes).digest('hex');
const requireThat = (condition, detail, code = 'ZRYNA-C4106') => {
  if (!condition) reject(detail, undefined, code);
};
const carrier = abi => ({ 'c-i32': 'i32', 'c-int': 'i32', bool32: 'bool' })[abi];

function callsIn(fn) {
  const calls = [];
  const pending = fn.body.map(statement => statement.value);
  while (pending.length) {
    const node = pending.pop();
    if (node.kind === 'call') { calls.push(node); pending.push(...node.args); }
    if (node.kind === 'add') pending.push(node.left, node.right);
  }
  return calls.sort((left, right) => left.start - right.start || left.end - right.end);
}

// Input must first pass the independent sidecar design checks. This adds parsed-source
// correspondence and a restricted token-flow replay; it grants no compiler authority.
export function inspectSources(document, sourceFiles, target = 'x86_64-unknown-linux-gnu') {
  requireThat(target === 'x86_64-unknown-linux-gnu' && document.target === target,
    'unsupported-target', 'ZRYNA-C4103');
  requireThat(document.sources.length <= 256 && document.sites.length <= 4096,
    'source-collections', 'ZRYNA-C4107');
  requireThat(sourceFiles.size === document.sources.length, 'exact-source-set');
  const parsed = new Map();
  const inspections = [];
  const actualSites = [];
  const operations = new Map(document.operations.map(operation => [operation.key, operation]));
  let sourceBytes = 0;
  for (const source of document.sources) {
    const bytes = sourceFiles.get(source.path);
    requireThat(Buffer.isBuffer(bytes) && digest(bytes) === source.sha256, 'source-identity');
    sourceBytes += bytes.length;
    requireThat(sourceBytes <= 8 * 1024 * 1024, 'aggregate-source-bytes', 'ZRYNA-C4107');
    const project = parseSource(bytes);
    parsed.set(source.path, project);
    for (const fn of project.functions) {
      inspections.push({ path: source.path, ...inspectFunction(fn, operations) });
      for (const node of callsIn(fn)) {
        const keyed = ['rawCall', 'release', 'foreignError'].includes(node.primitive);
        actualSites.push({ primitive: node.primitive, safety: node.primitive === 'rawCall' ? 'unsafe-raw' : 'safe',
          operation: keyed ? node.args[0].value : null, path: source.path, sourceSha256: source.sha256,
          start: node.start, end: node.end, spelling: bytes.subarray(node.start, node.end).toString() });
      }
    }
  }
  actualSites.sort((left, right) => left.path < right.path ? -1 : left.path > right.path ? 1
    : left.start - right.start || left.end - right.end);
  requireThat(actualSites.length === document.sites.length, 'complete-primitive-site-set');
  for (const [index, site] of actualSites.entries()) {
    requireThat(Object.entries(site).every(([key, value]) => document.sites[index][key] === value),
      'parsed-primitive-site');
  }
  const boundExports = new Set();
  for (const [ordinal, operation] of document.operations.entries()) {
    const binding = operation.sourceBinding;
    const project = parsed.get(binding.path);
    requireThat(project && binding.ordinal === ordinal && binding.sha256 === digest(project.bytes),
      'operation-source-identity');
    if (operation.direction === 'import') {
      requireThat(actualSites.some(site => site.path === binding.path && site.start === binding.start
        && site.end === binding.end && site.operation === operation.key
        && ['rawCall', 'release'].includes(site.primitive)), 'parsed-import-binding');
    } else {
      const fn = project.functions.find(fn => fn.start === binding.start && fn.end === binding.end);
      requireThat(fn?.exported && fn.name === operation.logicalName, 'parsed-export-binding');
      requireThat(fn.parameters.length === operation.parameters.length
        && fn.parameters.every((parameter, index) => parameter.type === carrier(operation.parameters[index].abi))
        && fn.result === carrier(operation.result), 'export-source-signature', 'ZRYNA-C4104');
      boundExports.add(fn);
    }
  }
  for (const project of parsed.values()) for (const fn of project.functions)
    requireThat(!fn.exported || boundExports.has(fn), 'unbound-export');
  return { state: 'prototype-source-inspected', inspections, sites: actualSites };
}

// Disposable scalar reverse-client header. Only reviewed total scalar export rows are emitted.
// Production header emission must instead consume the future independently sealed export authority.
export function scalarPrototypeHeader(document) {
  const types = { 'c-i32': 'int32_t', 'c-int': 'int', bool32: 'uint32_t' };
  const lines = [];
  const symbols = new Set(document.operations.filter(operation => operation.direction === 'import')
    .map(operation => operation.symbol.toLowerCase()));
  for (const operation of document.operations.filter(operation => operation.direction === 'export')) {
    requireThat(operation.mode === 'direct' && operation.effects === 'total' && operation.resources.length === 0
      && operation.statuses.length === 0 && types[operation.result]
      && operation.parameters.every(parameter => types[parameter.abi]), 'export-surface', 'ZRYNA-C4104');
    requireThat(allowedExportName(operation.logicalName)
      && operation.symbol === `zryna_c_v0_e_${operation.logicalName}`
      && !symbols.has(operation.symbol.toLowerCase()), 'export-symbol', 'ZRYNA-C4102');
    symbols.add(operation.symbol.toLowerCase());
    const parameters = operation.parameters.map(parameter => types[parameter.abi]).join(', ') || 'void';
    lines.push(`${types[operation.result]} ${operation.symbol}(${parameters});`);
  }
  return '#ifndef ZRYNA_NATIVE_C_PROTOTYPE_EXPORTS_H\n#define ZRYNA_NATIVE_C_PROTOTYPE_EXPORTS_H\n'
    + '#include <stdint.h>\n' + lines.join('\n') + '\n#endif\n';
}
