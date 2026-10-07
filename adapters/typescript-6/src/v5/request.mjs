// Isolated protocol-v5 request; inherited v4 forms retain their source-only meaning.
import ts from '@typescript/typescript6';
import process from 'node:process';
import { once } from 'node:events';
import { TextDecoder } from 'node:util';
import {
  protocolVersion,
  providerVersion,
  maxRequestBytes,
  maxResponseBytes,
  maxFiles,
  maxJsonDepth,
  maxJsonContainers,
  maxJsonFields,
  AdapterError,
  failRequest,
  failBudget,
  isRecord,
  requireExactKeys,
  requireRequestId,
  isRequestId,
  validatePortablePath,
  compactText,
  DiagnosticCollector
} from './common.mjs';
import {
  normalizeSource
} from './declarations.mjs';

function rejectDuplicateObjectKeys(text) {
  const stack = [];
  let containers = 0;
  let fields = 0;
  for (let index = 0; index < text.length; index += 1) {
    const character = text[index];
    if (character === '{' || character === '[') {
      containers += 1;
      if (stack.length >= maxJsonDepth) failBudget('request exceeds the JSON depth limit');
      if (containers > maxJsonContainers) failBudget('request exceeds the JSON container limit');
      stack.push(character === '{' ? { kind: 'object', keys: new Set() } : { kind: 'array' });
      continue;
    }
    if (character === '}' || character === ']') {
      stack.pop();
      continue;
    }
    if (character !== '"') continue;
    const start = index;
    index += 1;
    let escaped = false;
    while (index < text.length) {
      const current = text[index];
      if (escaped) escaped = false;
      else if (current === '\\') escaped = true;
      else if (current === '"') break;
      index += 1;
    }
    if (index >= text.length) return;
    let next = index + 1;
    while (/\s/.test(text[next] ?? '')) next += 1;
    const frame = stack.at(-1);
    if (text[next] !== ':' || frame?.kind !== 'object') continue;
    fields += 1;
    if (fields > maxJsonFields) failBudget('request exceeds the JSON field limit');
    let key;
    try {
      key = JSON.parse(text.slice(start, index + 1));
    } catch {
      return;
    }
    if (frame.keys.has(key)) failRequest('request contains a duplicate object field');
    frame.keys.add(key);
  }
}

function validateAnalyzeParams(params) {
  requireExactKeys(params, ['schema_version', 'files'], 'analyze params');
  if (params.schema_version !== protocolVersion || !Array.isArray(params.files)) failRequest('analyze requires a protocol-v5 file list');
  if (params.files.length > maxFiles) failBudget('request exceeds the source-file limit');
  const identities = new Set();
  const files = params.files.map((input, index) => {
    requireExactKeys(input, ['path', 'text'], `source file ${index}`);
    const path = validatePortablePath(input.path);
    if (typeof input.text !== 'string') failRequest(`source file ${index} text must be a string`);
    const identity = path.toLowerCase();
    if (identities.has(identity)) failRequest('source paths collide under portable identity');
    identities.add(identity);
    return { path, text: input.text };
  });
  return files.sort((a, b) => a.path < b.path ? -1 : a.path > b.path ? 1 : 0);
}

function handle(request) {
  if (!isRecord(request)) failRequest('request must be an object');
  requireRequestId(request.id);
  if (request.method === 'handshake') {
    requireExactKeys(request, ['id', 'method'], 'handshake request');
    return {
      id: request.id,
      result: {
        provider: 'typescript-6', provider_version: providerVersion, protocol_version: protocolVersion,
        capabilities: {
          module_resolution: false, semantic_diagnostics: false,
          control_flow_v1: true, data_ownership_syntax_v1: true, bounded_generics_syntax_v1: true,
        },
      },
    };
  }
  if (request.method === 'analyze') {
    requireExactKeys(request, ['id', 'method', 'params'], 'analyze request');
    const files = validateAnalyzeParams(request.params);
    const collector = new DiagnosticCollector();
    const budgets = {
      sourceBytes: 0, imports: 0, bindings: 0, functions: 0, parameters: 0,
      blocks: 0, statements: 0, expressions: 0, locals: 0,
      dataDeclarations: 0, members: 0, types: 0, aggregateOperands: 0, matchArms: 0,
    };
    const normalized = files.map((file, index) => normalizeSource(file, index, collector, budgets));
    return { id: request.id, result: { schema_version: protocolVersion, files: normalized, diagnostics: collector.finish() } };
  }
  failRequest('request method is unsupported');
}

function errorResponse(id, error) {
  return { id: isRequestId(id) ? id : null, error: { code: error instanceof AdapterError ? error.code : 'ZRYNA-F1003', message: compactText(error instanceof Error ? error.message : String(error)) } };
}

async function writeResponse(response) {
  let serialized = JSON.stringify(response);
  if (Buffer.byteLength(serialized, 'utf8') > maxResponseBytes) serialized = JSON.stringify(errorResponse(response?.id, new AdapterError('ZRYNA-F1002', 'response exceeds the byte limit')));
  if (!process.stdout.write(`${serialized}\n`)) await once(process.stdout, 'drain');
}

async function processLine(bytes) {
  let request;
  try {
    let text;
    try { text = new TextDecoder('utf-8', { fatal: true }).decode(bytes); }
    catch { failRequest('request is not valid UTF-8'); }
    if (!text.trim()) return;
    rejectDuplicateObjectKeys(text);
    try { request = JSON.parse(text); }
    catch { failRequest('request is not valid JSON'); }
    await writeResponse(handle(request));
  } catch (error) {
    await writeResponse(errorResponse(request?.id, error));
  }
}

let lineParts = [];
let lineBytes = 0;
let discardingOversizedLine = false;
for await (const chunk of process.stdin) {
  let cursor = 0;
  while (cursor < chunk.length) {
    const newline = chunk.indexOf(0x0a, cursor);
    const end = newline === -1 ? chunk.length : newline;
    const part = chunk.subarray(cursor, end);
    if (!discardingOversizedLine) {
      if (lineBytes + part.length > maxRequestBytes) {
        lineParts = [];
        lineBytes = 0;
        discardingOversizedLine = true;
      } else if (part.length > 0) {
        lineParts.push(part);
        lineBytes += part.length;
      }
    }
    if (newline === -1) break;
    if (discardingOversizedLine) await writeResponse(errorResponse(null, new AdapterError('ZRYNA-F1002', 'request exceeds the byte limit')));
    else await processLine(Buffer.concat(lineParts, lineBytes));
    lineParts = [];
    lineBytes = 0;
    discardingOversizedLine = false;
    cursor = newline + 1;
  }
}
if (discardingOversizedLine) await writeResponse(errorResponse(null, new AdapterError('ZRYNA-F1002', 'request exceeds the byte limit')));
else if (lineBytes > 0) await processLine(Buffer.concat(lineParts, lineBytes));
