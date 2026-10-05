import assert from 'node:assert/strict';
import { test } from 'node:test';
import { parseI32, readArguments, renderArguments, renderDiagnostics }
  from '../examples/playground/restricted/presentation.mjs';

// A deliberately inert DOM double tests presentation, not compiler or browser acceptance.
class Element {
  constructor(tag) { this.tagName = tag; this.children = []; this.attributes = {}; this.events = {}; }
  set innerHTML(value) { throw new Error(`HTML parsing attempted: ${value}`); }
  set textContent(value) { this.text = String(value); this.children = []; }
  get textContent() { return (this.text ?? '') + this.children.map(child => child.textContent).join(''); }
  append(...children) { this.children.push(...children); }
  replaceChildren(...children) { this.text = ''; this.children = children; }
  setAttribute(key, value) { this.attributes[key] = String(value); }
  addEventListener(key, action) { this.events[key] = action; }
  focus() { this.focused = true; }
  setSelectionRange(start, end) { this.selection = [start, end]; }
}
const document = { createElement: tag => new Element(tag) };

test('canonical signed i32 input admits inclusive bounds and rejects alternate spellings', () => {
  assert.equal(parseI32('-2147483648'), -2147483648);
  assert.equal(parseI32('2147483647'), 2147483647);
  assert.equal(parseI32('0'), 0);
  for (const text of ['-0', '00', '-01', '+1', ' 1', '1 ', '1\n', '1e2', '1.0',
    '0x10', 'NaN', '', '2147483648', '-2147483649', '９', '9'.repeat(400), null]) {
    assert.throws(() => parseI32(text), /whole decimal number/);
  }
});

test('diagnostic markup remains literal and UTF-8 locations select Unicode source units', () => {
  const source = new Element('textarea');
  source.value = '\ufeff😀\r\néx';
  const list = new Element('ul');
  const message = '<img src=x onerror="throw 1">';
  const guidance = '<script>window.compromised = true</script>';
  const report = { schema_version: 1, diagnostics: [{ code: 'TS1005', severity: 'error',
    path: 'src/main.zry', byte_start: 9, byte_end: 11, line_start: 2, column_start: 1,
    line_end: 2, column_end: 2, message, guidance }] };
  renderDiagnostics(document, list, report, source);
  assert.equal(list.children.length, 1);
  const item = list.children[0];
  assert.deepEqual(item.children.map(child => child.tagName), ['p', 'button', 'p']);
  assert.ok(item.textContent.includes(message));
  assert.ok(item.textContent.includes(guidance));
  assert.ok(item.textContent.includes('src/main.zry:2:1–2:2 (UTF-8 bytes 9–11)'));
  item.children[1].events.click();
  assert.equal(source.focused, true);
  assert.deepEqual(source.selection, [5, 6]);
  assert.equal(source.value, '\ufeff😀\r\néx');
});

test('spanless diagnostics do not invent a source location or navigation control', () => {
  const list = new Element('ul');
  renderDiagnostics(document, list, { schema_version: 1, diagnostics: [{ code: 'ZRYNA-M1004',
    severity: 'error', path: null, byte_start: null, byte_end: null, line_start: null,
    column_start: null, line_end: null, column_end: null, message: 'Rejected', guidance: '' }] },
  new Element('textarea'));
  assert.deepEqual(list.children[0].children.map(child => child.tagName), ['p', 'p']);
  assert.ok(list.textContent.includes('No source span'));
  renderDiagnostics(document, list, { schema_version: 1, diagnostics: [] }, new Element('textarea'));
  assert.equal(list.children.length, 0);
});

test('arity controls have associated labels and errors and focus the first invalid input', () => {
  const container = new Element('div');
  const fields = renderArguments(document, container, 3, [-2147483648, 2, 2147483647]);
  assert.equal(fields.length, 3);
  assert.deepEqual(readArguments(fields), [-2147483648, 2, 2147483647]);
  assert.equal(container.children[1].children[0].htmlFor, 'argument-1');
  assert.equal(fields[1].input.attributes['aria-describedby'], 'argument-help argument-error-1');
  fields[0].input.value = '-0';
  fields[2].input.value = '2147483648';
  assert.equal(readArguments(fields, true), null);
  assert.equal(fields[0].input.focused, true);
  assert.equal(fields[2].input.focused, undefined);
  assert.equal(fields[0].input.attributes['aria-invalid'], 'true');
  assert.match(fields[2].error.textContent, /whole decimal/);
  fields[0].input.value = '0';
  fields[2].input.value = '2147483647';
  assert.deepEqual(readArguments(fields), [0, 2, 2147483647]);
  assert.equal(fields[0].error.textContent, '');
  assert.equal(fields[0].input.attributes['aria-invalid'], 'false');
  renderArguments(document, container, 0);
  assert.match(container.textContent, /no arguments/);
});
