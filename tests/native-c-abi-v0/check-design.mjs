import { createHash } from 'node:crypto';
import { readFile } from 'node:fs/promises';
import Ajv from 'ajv/dist/2020.js';

const schema = JSON.parse(await readFile(
  new URL('../../schemas/zryna-native-c-declarations-v0.schema.json', import.meta.url), 'utf8',
));
const validate = new Ajv({ strict: true }).compile(schema);
export const validateShape = document => validate(document);
export const sha256 = bytes => createHash('sha256').update(bytes).digest('hex');
export const canonical = value => {
  if (Array.isArray(value)) return `[${value.map(canonical).join(',')}]`;
  if (value !== null && typeof value === 'object') return `{${Object.keys(value).sort()
    .map(key => `${JSON.stringify(key)}:${canonical(value[key])}`).join(',')}}`;
  return JSON.stringify(value);
};
export const wire = value => `${canonical(value)}\n`;
export const declarationDigest = bytes => sha256(Buffer.concat([
  Buffer.from('ZRYNA-NATIVE-C-DECLARATION-V0\0'), Buffer.from(bytes),
]));

function reject(code, detail) { throw new Error(`${code}:${detail}`); }
function requireThat(condition, code, detail) { if (!condition) reject(code, detail); }
function orderedUnique(values, code, detail) {
  requireThat(values.every((value, index) => index === 0 || values[index - 1] < value), code, detail);
}
function stringBytes(value) {
  if (typeof value === 'string') return Buffer.byteLength(value);
  if (Array.isArray(value)) return value.reduce((sum, child) => sum + stringBytes(child), 0);
  if (value && typeof value === 'object') return Object.values(value)
    .reduce((sum, child) => sum + stringBytes(child), 0);
  return 0;
}
function checkDepth(bytes) {
  let depth = 0;
  let quoted = false;
  let escaped = false;
  for (const character of bytes) {
    if (quoted) {
      if (escaped) escaped = false;
      else if (character === '\\') escaped = true;
      else if (character === '"') quoted = false;
    } else if (character === '"') quoted = true;
    else if (character === '{' || character === '[') {
      depth += 1;
      requireThat(depth <= 16, 'ZRYNA-C4107', 'wire-depth');
    } else if (character === '}' || character === ']') depth -= 1;
  }
}

// Checks review records and independently supplied fixture bytes only. No compiler or target executes.
export function checkDesign(bytes, sourceFiles, headerBytes) {
  requireThat(Buffer.byteLength(bytes) <= 1048576, 'ZRYNA-C4107', 'wire-bytes');
  if (Buffer.isBuffer(bytes)) {
    try { bytes = new TextDecoder('utf-8', { fatal: true }).decode(bytes); }
    catch { reject('ZRYNA-C4100', 'utf8'); }
  }
  checkDepth(bytes);
  let document;
  try { document = JSON.parse(bytes); } catch { reject('ZRYNA-C4100', 'json'); }
  requireThat(wire(document) === bytes, 'ZRYNA-C4100', 'canonical-wire-or-duplicate-key');
  for (const [values, limit, metric] of [[document?.sources, 256, 'sources'],
    [document?.libraries, 16, 'libraries'], [document?.operations, 256, 'operations'],
    [document?.sites, 4096, 'sites']]) if (Array.isArray(values))
    requireThat(values.length <= limit, 'ZRYNA-C4107', metric);
  for (const operation of Array.isArray(document?.operations) ? document.operations : []) {
    for (const [values, limit, metric] of [[operation?.parameters, 16, 'parameters'],
      [operation?.resources, 8, 'resources'], [operation?.statuses, 16, 'statuses']])
      if (Array.isArray(values)) requireThat(values.length <= limit, 'ZRYNA-C4107', metric);
  }
  for (const library of Array.isArray(document?.libraries) ? document.libraries : []) {
    for (const values of [library?.kinds, library?.allocators]) if (Array.isArray(values))
      requireThat(values.length <= 16, 'ZRYNA-C4107', 'library-resources');
  }
  for (const site of Array.isArray(document?.sites) ? document.sites : [])
    if (typeof site?.spelling === 'string') requireThat(Buffer.byteLength(site.spelling) <= 4096,
      'ZRYNA-C4107', 'primitive-spelling-bytes');
  if (!validate(document)) reject(validate.errors.some(error => error.keyword === 'maxLength')
    ? 'ZRYNA-C4107' : 'ZRYNA-C4101', JSON.stringify(validate.errors));
  requireThat(stringBytes(document) <= 65536, 'ZRYNA-C4107', 'string-value-bytes');
  orderedUnique(document.sources.map(source => source.path), 'ZRYNA-C4102', 'source-order');
  orderedUnique(document.libraries.map(library => library.id), 'ZRYNA-C4102', 'library-order');
  orderedUnique(document.operations.map(operation => operation.key), 'ZRYNA-C4102', 'operation-order');
  const sources = new Map(document.sources.map(source => [source.path, source]));
  for (const source of document.sources) {
    requireThat(!source.path.includes('//') && !source.path.startsWith('/'), 'ZRYNA-C4106', 'path');
    requireThat(sourceFiles.has(source.path) && sha256(sourceFiles.get(source.path)) === source.sha256,
      'ZRYNA-C4106', 'source-bytes');
  }
  const libraries = new Map(document.libraries.map(library => [library.id, library]));
  const operations = new Map(document.operations.map(operation => [operation.key, operation]));
  const symbolKeys = document.operations.map(operation => operation.symbol.toLowerCase());
  requireThat(new Set(symbolKeys).size === symbolKeys.length, 'ZRYNA-C4102', 'symbol-collision');
  for (const library of document.libraries) {
    requireThat(library.id.split('@').at(-1) === library.version, 'ZRYNA-C4102', 'library-version');
    requireThat(library.headerSha256 === sha256(headerBytes), 'ZRYNA-C4102', 'header-digest');
    const policies = document.operations.filter(operation => operation.library === library.id)
      .map(({ sourceBinding, ...policy }) => policy);
    requireThat(library.policySha256 === sha256(wire({ kinds: library.kinds,
      allocators: library.allocators, operations: policies })), 'ZRYNA-C4102', 'policy-digest');
    orderedUnique(library.kinds, 'ZRYNA-C4102', 'kind-order');
    orderedUnique(library.allocators.map(allocator => allocator.id), 'ZRYNA-C4102', 'allocator-order');
    for (const allocator of library.allocators) {
      const create = operations.get(allocator.create);
      const release = operations.get(allocator.release);
      requireThat(library.kinds.includes(allocator.kind) && create?.library === library.id
        && release?.library === library.id && allocator.kind.startsWith(`${library.id}/`)
        && allocator.id.startsWith(`${library.id}/`)
        && create.resources.some(resource => resource.allocator === allocator.id
          && resource.kind === allocator.kind && resource.access === 'create')
        && release.resources.some(resource => resource.allocator === allocator.id
          && resource.kind === allocator.kind && resource.access === 'consume'),
      'ZRYNA-C4105', 'allocator-reference');
    }
  }
  for (const [ordinal, operation] of document.operations.entries()) {
    const library = libraries.get(operation.library);
    if (operation.direction === 'export') {
      requireThat(operation.mode === 'direct' && operation.effects === 'total'
        && operation.resources.length === 0 && operation.parameters.every(parameter =>
          ['c-i32', 'c-int', 'bool32'].includes(parameter.abi))
        && ['c-i32', 'c-int', 'bool32'].includes(operation.result), 'ZRYNA-C4104', 'export-surface');
      requireThat(operation.logicalName.length <= 115, 'ZRYNA-C4107', 'export-name-bytes');
      requireThat(operation.symbol === `zryna_c_v0_e_${operation.logicalName}`,
        'ZRYNA-C4102', 'export-symbol');
    } else requireThat(library && operation.key === `${library.id}/${operation.symbol}`,
      'ZRYNA-C4102', 'import-library');
    requireThat(new Set(operation.parameters.map(parameter => parameter.name)).size
      === operation.parameters.length, 'ZRYNA-C4104', 'duplicate-parameter');
    const outputSlots = operation.parameters.flatMap((parameter, index) =>
      parameter.abi.endsWith('-out') ? [index] : []);
    if (operation.mode === 'status') {
      requireThat(operation.result === 'c-i32' && operation.statuses.length > 0,
        'ZRYNA-C4104', 'status-carrier');
      orderedUnique(operation.statuses.map(status => status.code), 'ZRYNA-C4105', 'status-order');
      const success = operation.statuses[0];
      requireThat(success.code === 0 && success.kind === 'success' && success.condition === 'success'
        && canonical(success.initialized) === canonical(outputSlots)
        && canonical(success.newOwners) === canonical(operation.resources.flatMap((resource, index) =>
          resource.access === 'create' ? [index] : [])), 'ZRYNA-C4105', 'success-outputs');
      for (const status of operation.statuses.slice(1)) requireThat(status.kind === 'recoverable'
        && status.condition !== 'success' && status.initialized.length === 0 && status.newOwners.length === 0,
      'ZRYNA-C4105', 'failure-atomicity');
      for (const status of operation.statuses) {
        if (status.condition === 'negative-first-i32') requireThat(
          ['c-i32', 'c-int'].includes(operation.parameters[0]?.abi), 'ZRYNA-C4105', 'negative-status-input');
        if (status.condition.includes('over-limit')) requireThat(operation.resources.some(resource =>
          resource.kind === 'borrowed-bytes'), 'ZRYNA-C4105', 'length-status-input');
        if (status.condition.includes('allocation')) requireThat(operation.resources.some(resource =>
          resource.access === 'create'), 'ZRYNA-C4105', 'allocation-status-owner');
      }
    } else requireThat(operation.statuses.length === 0 && outputSlots.length === 0
      && ((operation.mode === 'void') === (operation.result === 'unit')),
    'ZRYNA-C4104', 'direct-or-void');
    operation.resources.forEach((resource, index) => {
      requireThat(resource.slots.every(slot => operation.parameters[slot]?.resource === index),
        'ZRYNA-C4105', 'resource-slot');
      const allocator = library?.allocators.find(item => item.id === resource.allocator);
      if (resource.allocator === 'none') requireThat(resource.kind === 'borrowed-bytes'
        && resource.release === 'none' && resource.ownerBefore === 'caller'
        && resource.ownerAfter === 'caller' && resource.borrowEnd === 'return'
        && resource.maxBytes !== null && resource.encoding !== 'none'
        && resource.access === 'read',
      'ZRYNA-C4105', 'borrow-policy');
      else requireThat(allocator && allocator.kind === resource.kind && allocator.release === resource.release,
        'ZRYNA-C4105', 'allocator-pair');
      if (resource.access === 'read') requireThat(resource.ownerBefore === 'caller'
        && resource.ownerAfter === 'caller' && resource.borrowEnd === 'return'
        && !resource.fresh && !resource.releasableOnMalformed && resource.validPointerGuarantee,
      'ZRYNA-C4105', 'read-owner-policy');
      if (resource.access === 'create') requireThat(resource.ownerBefore === 'none'
        && resource.ownerAfter === 'caller' && resource.fresh && allocator?.create === operation.key,
      'ZRYNA-C4105', 'create-policy');
      if (resource.access === 'consume') requireThat(resource.ownerBefore === 'caller'
        && resource.ownerAfter === 'consumed' && resource.release === operation.key,
      'ZRYNA-C4105', 'consume-policy');
      const types = resource.slots.map(slot => operation.parameters[slot]?.abi);
      const expected = resource.kind === 'borrowed-bytes'
        ? ['bytes-in', 'count']
        : allocator?.category === 'handle'
          ? [resource.access === 'create' ? 'handle-out' : 'handle-in']
          : (resource.access === 'create' ? ['bytes-owned-out', 'count-out'] : ['bytes-release']);
      requireThat(canonical(types) === canonical(expected), 'ZRYNA-C4105', 'resource-signature');
      if (types.includes('handle-in') || types.includes('handle-out'))
        requireThat(resource.nullRule === 'nonnull' && resource.maxBytes === null
          && resource.encoding === 'none', 'ZRYNA-C4105', 'handle-policy');
      else requireThat(resource.maxBytes !== null && resource.encoding !== 'none',
        'ZRYNA-C4105', 'byte-policy');
      if (resource.expectedLengthSlot !== null) requireThat(resource.access === 'create'
        && allocator?.category === 'bytes'
        && operation.parameters[resource.expectedLengthSlot]?.abi === 'count',
      'ZRYNA-C4105', 'expected-length-slot');
    });
    operation.parameters.forEach((parameter, index) => {
      if (parameter.resource !== null) requireThat(operation.resources[parameter.resource]?.slots.includes(index),
        'ZRYNA-C4105', 'parameter-resource');
      if (!['c-i32', 'c-int', 'bool32', 'i32-out'].includes(parameter.abi))
        requireThat(parameter.resource !== null, 'ZRYNA-C4105', 'pointer-or-count-policy');
    });
    for (const status of operation.statuses) {
      requireThat(status.newOwners.every(index => operation.resources[index]?.access === 'create'),
        'ZRYNA-C4105', 'created-owner-index');
    }
    const binding = operation.sourceBinding;
    const source = sourceFiles.get(binding.path);
    requireThat(sources.get(binding.path)?.sha256 === binding.sha256 && source
      && binding.ordinal === ordinal && binding.start < binding.end && binding.end <= source.length,
    'ZRYNA-C4106', 'operation-binding');
    const declaration = source.subarray(binding.start, binding.end).toString('utf8');
    requireThat(operation.direction === 'import' ? declaration.includes(`"${operation.key}"`)
      : declaration.startsWith(`export function ${operation.logicalName}(`) && declaration.endsWith('}'),
    'ZRYNA-C4106', 'declaration-spelling');
    if (operation.direction === 'import') requireThat(document.sites.some(site =>
      site.operation === operation.key && ['rawCall', 'release'].includes(site.primitive)
      && site.path === binding.path && site.start === binding.start && site.end === binding.end),
    'ZRYNA-C4106', 'declaration-site');
  }
  let previousSite;
  for (const site of document.sites) {
    const source = sourceFiles.get(site.path);
    requireThat(source && sources.get(site.path)?.sha256 === site.sourceSha256
      && site.start < site.end && site.end <= source.length
      && source.subarray(site.start, site.end).toString('utf8') === site.spelling,
    'ZRYNA-C4106', 'site-source');
    requireThat(site.spelling.startsWith(`Ffi.${site.primitive}(`), 'ZRYNA-C4106', 'primitive-spelling');
    const key = [site.path, site.start, site.end];
    if (previousSite) requireThat(previousSite[0] < key[0]
      || (previousSite[0] === key[0] && (previousSite[1] < key[1]
        || (previousSite[1] === key[1] && previousSite[2] < key[2]))), 'ZRYNA-C4106', 'site-order');
    previousSite = key;
    const needsOperation = ['rawCall', 'release', 'foreignError'].includes(site.primitive);
    requireThat(needsOperation === (site.operation !== null), 'ZRYNA-C4106', 'site-operation');
    requireThat(site.safety === (site.primitive === 'rawCall' ? 'unsafe-raw' : 'safe'),
      'ZRYNA-C4106', 'unsafe-marker');
    if (needsOperation) {
      const operation = operations.get(site.operation);
      requireThat(operation?.direction === 'import'
        && site.spelling.startsWith(`Ffi.${site.primitive}("${site.operation}"`),
      'ZRYNA-C4106', 'operation-literal');
      if (site.primitive === 'release') requireThat(operation.mode === 'void', 'ZRYNA-C4105', 'release-call');
      if (site.primitive === 'foreignError') requireThat(operation.mode === 'status', 'ZRYNA-C4105', 'error-call');
    }
  }
  return declarationDigest(bytes);
}
