// Isolated protocol-v5 common; inherited v4 forms retain their source-only meaning.
import ts from '@typescript/typescript6';
import { PROTOCOL_V5_LIMITS } from '../limits-v5.mjs';

function boundedTestLimit(name, productionLimit) {
  if (process.env.NODE_ENV !== 'test') return productionLimit;
  const configured = process.env[`ZRYNA_TEST_${name}`];
  if (configured === undefined) return productionLimit;
  const value = Number(configured);
  if (!Number.isSafeInteger(value) || value < 1 || value > productionLimit) {
    throw new Error(`invalid lowered test limit for ${name}`);
  }
  return value;
}

export const protocolVersion = 5;
const expectedProviderVersion = '6.0.3';
export const providerVersion = ts.version;
if (providerVersion !== expectedProviderVersion) {
  throw new Error(`the TypeScript provider must be exactly ${expectedProviderVersion}`);
}

export const maxRequestBytes = boundedTestLimit('REQUEST_BYTES', PROTOCOL_V5_LIMITS.requestBytes);
export const maxResponseBytes = boundedTestLimit('RESPONSE_BYTES', PROTOCOL_V5_LIMITS.responseBytes);
export const maxFiles = boundedTestLimit('FILES', PROTOCOL_V5_LIMITS.files);
export const maxSourceFileBytes = boundedTestLimit('SOURCE_FILE_BYTES', PROTOCOL_V5_LIMITS.sourceFileBytes);
export const maxSourceBytes = boundedTestLimit('SOURCE_BYTES', PROTOCOL_V5_LIMITS.sourceBytes);
export const maxFunctionsPerFile = boundedTestLimit('FUNCTIONS_PER_FILE', 4096);
export const maxFunctionsPerProject = boundedTestLimit('FUNCTIONS_PER_PROJECT', 16_384);
export const maxImportsPerFile = boundedTestLimit('IMPORTS_PER_FILE', 4096);
export const maxImportsPerProject = boundedTestLimit('IMPORTS_PER_PROJECT', 65_536);
export const maxBindingsPerImport = boundedTestLimit('BINDINGS_PER_IMPORT', 256);
export const maxBindingsPerProject = boundedTestLimit('BINDINGS_PER_PROJECT', 65_536);
export const maxParametersPerFunction = boundedTestLimit('PARAMETERS_PER_FUNCTION', 256);
export const maxParametersPerProject = boundedTestLimit('PARAMETERS_PER_PROJECT', 262_144);
export const maxBlocksPerFunction = boundedTestLimit('BLOCKS_PER_FUNCTION', 4096);
export const maxBlocksPerProject = boundedTestLimit('BLOCKS_PER_PROJECT', 65_536);
export const maxStatementsPerFunction = boundedTestLimit('STATEMENTS_PER_FUNCTION', 4096);
export const maxStatementsPerProject = boundedTestLimit('STATEMENTS_PER_PROJECT', 65_536);
export const maxExpressionsPerFunction = boundedTestLimit('EXPRESSIONS_PER_FUNCTION', 16_384);
export const maxExpressionsPerProject = boundedTestLimit('EXPRESSIONS_PER_PROJECT', 262_144);
export const maxLocalsPerFunction = boundedTestLimit('LOCALS_PER_FUNCTION', 4096);
export const maxLocalsPerProject = boundedTestLimit('LOCALS_PER_PROJECT', 65_536);
export const maxNesting = boundedTestLimit('NESTING', 128);
export const maxCallArguments = boundedTestLimit('CALL_ARGUMENTS', 256);
export const maxNominalDeclarationsPerModule = boundedTestLimit(
  'NOMINAL_DECLARATIONS_PER_MODULE',
  boundedTestLimit('NOMINAL_DECLARATIONS', PROTOCOL_V5_LIMITS.nominalDeclarationsPerModule),
);
export const maxNominalDeclarationsPerProject = boundedTestLimit('NOMINAL_DECLARATIONS_PER_PROJECT', PROTOCOL_V5_LIMITS.nominalDeclarationsPerProject);
export const maxMembersPerDeclaration = boundedTestLimit('MEMBERS_PER_DECLARATION', PROTOCOL_V5_LIMITS.membersPerDeclaration);
export const maxMembersPerProject = boundedTestLimit('MEMBERS_PER_PROJECT', PROTOCOL_V5_LIMITS.membersPerProject);
export const maxTypeSyntaxNodesPerModule = boundedTestLimit(
  'TYPE_SYNTAX_NODES_PER_MODULE',
  boundedTestLimit('TYPE_SYNTAX_NODES', PROTOCOL_V5_LIMITS.typeSyntaxNodesPerModule),
);
export const maxTypeSyntaxNodesPerProject = boundedTestLimit('TYPE_SYNTAX_NODES_PER_PROJECT', PROTOCOL_V5_LIMITS.typeSyntaxNodesPerProject);
export const maxTypeSyntaxNesting = boundedTestLimit('TYPE_SYNTAX_NESTING', PROTOCOL_V5_LIMITS.typeSyntaxNesting);
export const maxObjectInitializersPerConstruction = boundedTestLimit('OBJECT_INITIALIZERS_PER_CONSTRUCTION', PROTOCOL_V5_LIMITS.objectInitializersPerConstruction);
export const maxArrayElementsPerConstruction = boundedTestLimit('ARRAY_ELEMENTS_PER_CONSTRUCTION', PROTOCOL_V5_LIMITS.arrayElementsPerConstruction);
export const maxConstructionOperandsPerProject = boundedTestLimit(
  'CONSTRUCTION_OPERANDS_PER_PROJECT',
  boundedTestLimit('AGGREGATE_OPERANDS', PROTOCOL_V5_LIMITS.constructionOperandsPerProject),
);
export const maxMatchArmsPerExpression = boundedTestLimit('MATCH_ARMS_PER_EXPRESSION', PROTOCOL_V5_LIMITS.matchArmsPerExpression);
export const maxMatchArmsPerProject = boundedTestLimit('MATCH_ARMS_PER_PROJECT', PROTOCOL_V5_LIMITS.matchArmsPerProject);
export const maxFixedArrayLength = PROTOCOL_V5_LIMITS.fixedArrayLength;
export const maxFixedArrayLengthSpellingBytes = PROTOCOL_V5_LIMITS.fixedArrayLengthSpellingBytes;
const maxDiagnostics = PROTOCOL_V5_LIMITS.diagnostics;
const maxDiagnosticCharacters = 4096;
const maxIdentifierBytes = 128;
export const maxIntegerSpellingBytes = 64;
export const maxJsonDepth = 8;
export const maxJsonContainers = maxFiles + 4;
export const maxJsonFields = maxFiles * 2 + 8;

export class AdapterError extends Error {
  constructor(code, message) {
    super(message);
    this.code = code;
  }
}

export function failRequest(message) {
  throw new AdapterError('ZRYNA-F1001', message);
}

export function failBudget(message) {
  throw new AdapterError('ZRYNA-F1002', message);
}

export function failInvariant(message) {
  throw new AdapterError('ZRYNA-F1003', message);
}

export function isRecord(value) {
  return value !== null && typeof value === 'object' && !Array.isArray(value);
}

export function requireExactKeys(value, allowed, label) {
  if (!isRecord(value)) failRequest(`${label} must be an object`);
  const actual = Object.keys(value).sort();
  const expected = [...allowed].sort();
  if (actual.length !== expected.length || actual.some((key, index) => key !== expected[index])) {
    failRequest(`${label} contains unknown or missing fields`);
  }
}

export function requireRequestId(value) {
  if (!Number.isSafeInteger(value) || value < 0 || value > 0xffff_ffff) {
    failRequest('request id must be an unsigned 32-bit integer');
  }
}

export function isRequestId(value) {
  return Number.isSafeInteger(value) && value >= 0 && value <= 0xffff_ffff;
}

export function validatePortablePath(path) {
  if (typeof path !== 'string' || path.length === 0) failRequest('source path must be a string');
  if (!/^[\x20-\x7e]+$/.test(path) || Buffer.byteLength(path, 'utf8') > 1024) {
    failRequest('source path must be bounded printable ASCII');
  }
  if (path.startsWith('/') || path.includes('\\')) {
    failRequest('source path must be workspace-relative and use forward slashes');
  }
  const components = path.split('/');
  if (components.length > 32) failRequest('source path exceeds the component limit');
  for (const component of components) {
    if (
      component.length === 0 || component === '.' || component === '..' ||
      component.length > 255 || component.endsWith('.') || component.endsWith(' ') ||
      /[<>:"|?*]/.test(component)
    ) failRequest('source path contains a non-portable component');
    const stem = component.split('.')[0].toLowerCase();
    if (/^(con|prn|aux|nul|com[1-9]|lpt[1-9])$/.test(stem)) {
      failRequest('source path contains a reserved device name');
    }
  }
  return path;
}

const utf8OffsetMaps = new WeakMap();

export function buildUtf8OffsetMap(sourceFile) {
  const offsets = new Uint32Array(sourceFile.text.length + 1);
  let bytes = 0;
  for (let index = 0; index < sourceFile.text.length; index += 1) {
    offsets[index] = bytes;
    const codeUnit = sourceFile.text.charCodeAt(index);
    if (codeUnit <= 0x7f) bytes += 1;
    else if (codeUnit <= 0x7ff) bytes += 2;
    else if (codeUnit >= 0xd800 && codeUnit <= 0xdbff) {
      offsets[index + 1] = 0xffff_ffff;
      index += 1;
      bytes += 4;
    } else bytes += 3;
  }
  offsets[sourceFile.text.length] = bytes;
  utf8OffsetMaps.set(sourceFile, offsets);
}

function utf8ByteOffset(sourceFile, offset) {
  const offsets = utf8OffsetMaps.get(sourceFile);
  if (!offsets || !Number.isInteger(offset) || offset < 0 || offset > sourceFile.text.length) {
    failInvariant('TypeScript returned an invalid UTF-16 source offset');
  }
  if (offsets[offset] === 0xffff_ffff) failInvariant('TypeScript returned an offset inside a surrogate pair');
  return offsets[offset];
}

export function spanFromOffsets(sourceFile, file, start, end) {
  return { file, start: utf8ByteOffset(sourceFile, start), end: utf8ByteOffset(sourceFile, end) };
}

export function nodeSpan(node, sourceFile, file) {
  return spanFromOffsets(sourceFile, file, node.getStart(sourceFile), node.getEnd());
}

export function compactText(value) {
  const text = String(value).replaceAll(/\s+/g, ' ').trim();
  return [...text].slice(0, maxDiagnosticCharacters).join('') || 'provider diagnostic';
}

function compareDiagnostics(left, right) {
  const a = left.location.kind === 'source' ? left.location.span : null;
  const b = right.location.kind === 'source' ? right.location.span : null;
  const ak = [a?.file ?? -1, a?.start ?? -1, a?.end ?? -1, left.code, left.message, left.guidance];
  const bk = [b?.file ?? -1, b?.start ?? -1, b?.end ?? -1, right.code, right.message, right.guidance];
  for (let index = 0; index < ak.length; index += 1) {
    if (ak[index] < bk[index]) return -1;
    if (ak[index] > bk[index]) return 1;
  }
  return 0;
}

export class DiagnosticCollector {
  #diagnostics = [];
  #truncated = false;

  add(diagnostic) {
    if (this.#diagnostics.length < maxDiagnostics - 1) this.#diagnostics.push(diagnostic);
    else this.#truncated = true;
  }

  located(code, sourceFile, file, node, message, guidance) {
    this.add({
      code,
      severity: 'error',
      location: { kind: 'source', span: nodeSpan(node, sourceFile, file) },
      message: compactText(message),
      guidance: compactText(guidance),
    });
  }

  unsupported(node, sourceFile, file, context) {
    const kind = ts.SyntaxKind[node.kind] ?? `kind ${node.kind}`;
    const span = nodeSpan(node, sourceFile, file);
    throw new AdapterError(
      'ZRYNA-F2002',
      `${context} uses unsupported syntax '${kind}' at file ${span.file} bytes ${span.start}..${span.end}`,
    );
  }

  finish() {
    this.#diagnostics.sort(compareDiagnostics);
    if (this.#truncated) {
      this.#diagnostics.push({
        code: 'ZRYNA-F2003', severity: 'error', location: { kind: 'global' },
        message: 'frontend diagnostics exceeded the deterministic limit',
        guidance: 'reduce unsupported or malformed source before analysis',
      });
    }
    return this.#diagnostics;
  }
}

export function normalizedIdentifier(node, sourceFile, file, collector, context) {
  const spelling = node.getText(sourceFile);
  if (
    spelling !== node.text || !/^[A-Za-z_][A-Za-z0-9_]*$/.test(spelling) ||
    Buffer.byteLength(spelling, 'utf8') > maxIdentifierBytes || defensiveNames.has(spelling)
  ) {
    collector.unsupported(node, sourceFile, file, context);
    return null;
  }
  return { text: spelling, span: nodeSpan(node, sourceFile, file) };
}

export function findToken(node, kind, sourceFile) {
  const children = node.getChildren(sourceFile);
  const direct = children.find((child) => child.kind === kind);
  if (direct) return direct;
  for (const child of children) {
    const nested = findToken(child, kind, sourceFile);
    if (nested) return nested;
  }
  return null;
}

export function requiredToken(node, kind, sourceFile, label) {
  const token = findToken(node, kind, sourceFile);
  if (!token) failInvariant(`TypeScript omitted ${label}`);
  return token;
}

export function enforceParserNesting(text) {
  const scanner = ts.createScanner(ts.ScriptTarget.Latest, true, ts.LanguageVariant.Standard, text);
  let depth = 0;
  for (let token = scanner.scan(); token !== ts.SyntaxKind.EndOfFileToken; token = scanner.scan()) {
    if (token === ts.SyntaxKind.OpenBraceToken || token === ts.SyntaxKind.OpenBracketToken || token === ts.SyntaxKind.OpenParenToken) {
      depth += 1;
      if (depth > maxNesting) failBudget('source exceeds the nesting limit');
    } else if (token === ts.SyntaxKind.CloseBraceToken || token === ts.SyntaxKind.CloseBracketToken || token === ts.SyntaxKind.CloseParenToken) {
      depth = Math.max(0, depth - 1);
    }
  }
}

export const defensiveNames = new Set(['constructor', 'prototype', '__proto__']);

export function dataName(node, sourceFile, file, collector, context) {
  const name = normalizedIdentifier(node, sourceFile, file, collector, context);
  if (!name) return null;
  if (Buffer.byteLength(name.text, 'utf8') > maxIdentifierBytes || defensiveNames.has(name.text)) {
    collector.unsupported(node, sourceFile, file, context);
    return null;
  }
  return name;
}

