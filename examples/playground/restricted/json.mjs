// Closed bounded JSON decoding retains duplicate-key rejection before constructing objects.
import { limits, fail, utf8 } from './limits.mjs';

export function decodeJson(bytes, maxBytes) {
  if (!(bytes instanceof Uint8Array) || bytes.length > maxBytes) fail('BYTES');
  let text;
  try { text = new TextDecoder('utf-8', { fatal: true, ignoreBOM: true }).decode(bytes); }
  catch { fail('UTF8'); }
  let index = 0;
  let work = 0;
  const space = () => { while (/[ \t\r\n]/.test(text[index] ?? '\0')) index++; };

  function string() {
    const start = index++;
    while (index < text.length) {
      const char = text[index++];
      if (char === '"') {
        let value;
        try { value = JSON.parse(text.slice(start, index)); }
        catch { fail('JSON'); }
        utf8(value);
        return value;
      }
      if (char === '\\') index++;
    }
    fail('JSON');
  }

  function value(depth) {
    if (depth > limits.depth || ++work > limits.work) fail('JSON-LIMIT');
    space();
    const char = text[index];
    if (char === '"') return string();
    if (char === '{') {
      index++;
      const object = Object.create(null);
      space();
      if (text[index] === '}') { index++; return object; }
      let count = 0;
      while (index < text.length) {
        if (++count > limits.collection) fail('JSON-LIMIT');
        space();
        if (text[index] !== '"') fail('JSON');
        const key = string();
        if (Object.hasOwn(object, key)) fail('DUPLICATE');
        space();
        if (text[index++] !== ':') fail('JSON');
        object[key] = value(depth + 1);
        space();
        const separator = text[index++];
        if (separator === '}') return object;
        if (separator !== ',') fail('JSON');
      }
      fail('JSON');
    }
    if (char === '[') {
      index++;
      const array = [];
      space();
      if (text[index] === ']') { index++; return array; }
      while (index < text.length) {
        if (array.length >= limits.collection) fail('JSON-LIMIT');
        array.push(value(depth + 1));
        space();
        const separator = text[index++];
        if (separator === ']') return array;
        if (separator !== ',') fail('JSON');
      }
      fail('JSON');
    }
    for (const [literal, decoded] of [['true', true], ['false', false], ['null', null]]) {
      if (text.startsWith(literal, index)) { index += literal.length; return decoded; }
    }
    const number = text.slice(index).match(/^-?(?:0|[1-9][0-9]*)/);
    if (!number) fail('JSON');
    index += number[0].length;
    const result = Number(number[0]);
    if (!Number.isSafeInteger(result) || Object.is(result, -0)) fail('NUMBER');
    return result;
  }

  const result = value(0);
  space();
  if (index !== text.length) fail('JSON');
  return result;
}
