import {
  maxFiles,
  maxJsonContainers,
  maxJsonDepth,
  maxJsonFields,
  protocolVersion,
} from './configuration.mjs';
import {
  failBudget,
  failRequest,
} from './errors.mjs';

function isRecord(value) {
  return value !== null && typeof value === 'object' && !Array.isArray(value);
}

function requireExactKeys(value, allowed, label) {
  if (!isRecord(value)) failRequest(`${label} must be an object`);
  const actual = Object.keys(value).sort();
  const expected = [...allowed].sort();
  if (actual.length !== expected.length || actual.some((key, index) => key !== expected[index])) {
    failRequest(`${label} contains unknown or missing fields`);
  }
}

function requireRequestId(value) {
  if (!Number.isSafeInteger(value) || value < 0 || value > 0xffff_ffff) {
    failRequest('request id must be an unsigned 32-bit integer');
  }
}

function isRequestId(value) {
  return Number.isSafeInteger(value) && value >= 0 && value <= 0xffff_ffff;
}

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

function validatePortablePath(path) {
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

function validateAnalyzeParams(params) {
  requireExactKeys(params, ['schema_version', 'files'], 'analyze params');
  if (params.schema_version !== protocolVersion || !Array.isArray(params.files)) failRequest('analyze requires a protocol-v4 file list');
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

export {
  isRecord,
  isRequestId,
  rejectDuplicateObjectKeys,
  requireExactKeys,
  requireRequestId,
  validateAnalyzeParams
};
