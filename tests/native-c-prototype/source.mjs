// Disposable source projection. It constructs no protocol, IR, MIR or compiler authority.
const reservedExports = new Set(('arguments await break case catch class const constructor continue debugger '
  + 'default delete do else enum eval export extends false finally for function if implements import in '
  + 'instanceof interface let new null package private protected prototype public return static super switch '
  + 'then this throw true try typeof var void while with yield __proto__').split(' '));
export const allowedExportName = name => /^[A-Za-z_][A-Za-z0-9_]{0,114}$/.test(name)
  && !reservedExports.has(name);
const primitiveArity = new Map([
  ['borrowBytes', 1], ['borrowUtf8', 1], ['byteLength', 1], ['outI32', 0],
  ['outHandle', 1], ['outBytes', 0], ['outCount', 0], ['readI32', 1],
  ['takeHandle', 1], ['takeBytes', 2], ['copyBytes', 1], ['release', 2], ['foreignError', 2],
]);
export function reject(detail, token = { start: 0, end: 0 }, code = 'ZRYNA-C4106') {
  throw Object.assign(new Error(`${code}:${detail}`), { code, start: token.start, end: token.end });
}

function lex(input) {
  const bytes = Buffer.from(input);
  if (bytes.length > 2 * 1024 * 1024) reject('source-bytes', undefined, 'ZRYNA-C4107');
  try { new TextDecoder('utf-8', { fatal: true }).decode(bytes); }
  catch { reject('source-utf8'); }
  const tokens = [];
  let offset = 0;
  const ascii = byte => (byte >= 65 && byte <= 90) || (byte >= 97 && byte <= 122) || byte === 95;
  const digit = byte => byte >= 48 && byte <= 57;
  while (offset < bytes.length) {
    const start = offset;
    if ([9, 10, 13, 32].includes(bytes[offset])) { offset += 1; continue; }
    if (bytes[offset] === 47 && bytes[offset + 1] === 47) {
      while (offset < bytes.length && bytes[offset] !== 10) offset += 1;
      continue;
    }
    if (bytes[offset] === 47 && bytes[offset + 1] === 42) {
      const end = bytes.indexOf('*/', offset + 2);
      if (end === -1) reject('unterminated-comment', { start, end: bytes.length });
      offset = end + 2;
      continue;
    }
    let kind = 'punctuation';
    if (ascii(bytes[offset])) {
      kind = 'name';
      while (ascii(bytes[offset]) || digit(bytes[offset])) offset += 1;
    } else if (digit(bytes[offset])) {
      kind = 'integer';
      while (digit(bytes[offset])) offset += 1;
    } else if (bytes[offset] === 34) {
      kind = 'key';
      offset += 1;
      while (offset < bytes.length && bytes[offset] !== 34) {
        if (bytes[offset] < 32 || bytes[offset] > 126 || bytes[offset] === 92)
          reject('escaped-or-nonascii-key', { start, end: offset + 1 });
        offset += 1;
      }
      if (offset === bytes.length) reject('unterminated-key', { start, end: offset });
      offset += 1;
    } else if (bytes.subarray(offset, offset + 3).equals(Buffer.from('!=='))) offset += 3;
    else if ('(){}:;,.<>+-='.includes(String.fromCharCode(bytes[offset]))) offset += 1;
    else reject('unsupported-token', { start, end: start + 1 });
    const text = bytes.subarray(start, offset).toString();
    if ((kind === 'name' && text.length > 128) || (kind === 'key' && text.length > 259))
      reject('token-bytes', { start, end: offset }, 'ZRYNA-C4107');
    tokens.push({ kind, text, start, end: offset });
    if (tokens.length > 262144) reject('tokens', tokens.at(-1), 'ZRYNA-C4107');
  }
  tokens.push({ kind: 'end', text: '', start: offset, end: offset });
  return { bytes, tokens };
}

export function parseSource(input) {
  const { bytes, tokens } = lex(input);
  let cursor = 0;
  let depth = 0;
  const peek = () => tokens[cursor];
  const take = text => {
    const token = peek();
    if (token.text !== text) reject(`expected-${text}`, token);
    cursor += 1;
    return token;
  };
  const name = () => {
    const token = peek();
    if (token.kind !== 'name' || ['Ffi', 'function', 'const', 'return', 'if', 'export', 'true', 'false']
      .includes(token.text)) reject('binding-name', token);
    cursor += 1;
    return token.text;
  };
  function type() {
    const token = peek();
    if (token.kind !== 'name') reject('type', token);
    cursor += 1;
    if (token.text === 'Vec') { take('<'); take('i32'); take('>'); return 'Vec<i32>'; }
    if (!['i32', 'bool', 'String', 'FfiBytes', 'FfiI32Out', 'FfiHandleOut',
      'FfiBytesOut', 'FfiCountOut', 'FfiHandle', 'FfiOwnedBytes'].includes(token.text)) reject('type', token);
    return token.text;
  }
  function expression() {
    let left = atom();
    let additions = 0;
    while (peek().text === '+') {
      if (++additions > 127) reject('expression-depth', peek(), 'ZRYNA-C4107');
      take('+');
      const right = atom();
      left = { kind: 'add', left, right, start: left.start, end: right.end };
    }
    return left;
  }
  function atom() {
    if (++depth > 128) reject('expression-depth', peek(), 'ZRYNA-C4107');
    const start = peek().start;
    let node;
    if (peek().text === 'Ffi') {
      take('Ffi'); take('.');
      const primitive = peek().text;
      if (!primitiveArity.has(primitive) && primitive !== 'rawCall') reject('primitive', peek());
      cursor += 1; take('(');
      const args = [];
      if (peek().text !== ')') {
        do {
          args.push(expression());
          if (args.length > 17) reject('argument-count', peek(), 'ZRYNA-C4107');
          if (peek().text !== ',') break;
          take(',');
        } while (true);
      }
      const end = take(')').end;
      if (primitiveArity.has(primitive) && args.length !== primitiveArity.get(primitive))
        reject('primitive-arity', { start, end });
      node = { kind: 'call', primitive, args, start, end };
    } else if (peek().kind === 'key') {
      const token = tokens[cursor++];
      const value = token.text.slice(1, -1);
      if (!/^[A-Za-z0-9_/@.-]+$/.test(value)) reject('key', token);
      node = { kind: 'key', value, start, end: token.end };
    } else if (peek().kind === 'integer' || peek().text === '-') {
      const sign = peek().text === '-' ? (take('-'), -1) : 1;
      const token = peek();
      if (token.kind !== 'integer' || !/^(0|[1-9][0-9]*)$/.test(token.text)) reject('integer', token);
      cursor += 1;
      const value = sign * Number(token.text);
      if (!Number.isSafeInteger(value) || value < -2147483648 || value > 2147483647)
        reject('i32-range', token, 'ZRYNA-C4104');
      node = { kind: 'i32', value, start, end: token.end };
    } else if (['true', 'false'].includes(peek().text)) {
      const token = tokens[cursor++];
      node = { kind: 'bool', value: token.text === 'true', start, end: token.end };
    } else {
      const end = peek().end;
      node = { kind: 'local', name: name(), start, end };
    }
    depth -= 1;
    return node;
  }
  function statement() {
    const start = peek().start;
    if (peek().text === 'const') {
      take('const'); const binding = name(); take(':'); const annotation = type(); take('=');
      const value = expression(); const end = take(';').end;
      return { kind: 'const', name: binding, type: annotation, value, start, end };
    }
    if (peek().text === 'return') {
      take('return'); const value = expression(); const end = take(';').end;
      return { kind: 'return', value, start, end };
    }
    if (peek().text === 'if') {
      take('if'); take('('); const status = name(); take('!=='); take('0'); take(')'); take('{');
      take('return'); const value = expression(); take(';'); const end = take('}').end;
      return { kind: 'guard', status, value, start, end };
    }
    const value = expression(); const end = take(';').end;
    return { kind: 'expression', value, start, end };
  }
  const functions = [];
  const functionNames = new Set();
  while (peek().kind !== 'end') {
    const start = peek().start;
    const exported = peek().text === 'export';
    if (exported) take('export');
    take('function'); const binding = name(); take('(');
    if (functionNames.has(binding)) reject('duplicate-function', peek());
    functionNames.add(binding);
    const parameters = [];
    if (peek().text !== ')') {
      do {
        const binding = name(); take(':'); parameters.push({ name: binding, type: type() });
        if (parameters.length > 16) reject('parameters', peek(), 'ZRYNA-C4107');
        if (peek().text !== ',') break;
        take(',');
      } while (true);
    }
    take(')'); take(':'); const result = type(); take('{');
    const body = [];
    while (peek().text !== '}') {
      body.push(statement());
      if (body.length > 4096) reject('statements', peek(), 'ZRYNA-C4107');
    }
    const end = take('}').end;
    functions.push({ name: binding, exported, parameters, result, body, start, end });
    if (functions.length > 256) reject('functions', peek(), 'ZRYNA-C4107');
  }
  return { bytes, functions };
}
