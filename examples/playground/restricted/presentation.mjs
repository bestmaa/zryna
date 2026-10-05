import { isI32, utf8 } from './limits.mjs';

export function parseI32(text) {
  if (typeof text !== 'string' || !/^(?:0|-?[1-9][0-9]*)$/.test(text)) {
    throw new Error('Enter a whole decimal number from -2147483648 to 2147483647.');
  }
  const value = Number(text);
  if (!isI32(value)) {
    throw new Error('Enter a whole decimal number from -2147483648 to 2147483647.');
  }
  return value;
}

function textElement(document, tag, text) {
  const element = document.createElement(tag);
  element.textContent = text;
  return element;
}

export function selectDiagnostic(source, record) {
  if (record.byte_start === null) return;
  const bytes = utf8(source.value);
  const decoder = new TextDecoder('utf-8', { fatal: true, ignoreBOM: true });
  const start = decoder.decode(bytes.subarray(0, record.byte_start)).length;
  const end = decoder.decode(bytes.subarray(0, record.byte_end)).length;
  source.focus();
  source.setSelectionRange(start, end);
}

export function renderDiagnostics(document, list, report, source) {
  list.replaceChildren();
  for (const record of report.diagnostics) {
    const item = document.createElement('li');
    const location = record.byte_start === null ? 'No source span' :
      `${record.path}:${record.line_start}:${record.column_start}–` +
      `${record.line_end}:${record.column_end} (UTF-8 bytes ${record.byte_start}–${record.byte_end})`;
    item.append(textElement(document, 'p', `${record.severity}: ${record.code} — ${record.message}`));
    if (record.byte_start === null) {
      item.append(textElement(document, 'p', location));
    } else {
      const button = textElement(document, 'button', location);
      button.type = 'button';
      button.addEventListener('click', () => selectDiagnostic(source, record));
      item.append(button);
    }
    if (record.guidance) item.append(textElement(document, 'p', record.guidance));
    list.append(item);
  }
}

export function renderArguments(document, container, arity, initial = []) {
  container.replaceChildren();
  const fields = [];
  for (let index = 0; index < arity; index++) {
    const group = document.createElement('div');
    const input = document.createElement('input');
    input.id = `argument-${index}`;
    input.type = 'text';
    input.inputMode = 'text';
    input.spellcheck = false;
    input.autocomplete = 'off';
    input.value = String(initial[index] ?? 0);
    input.setAttribute('aria-describedby', `argument-help argument-error-${index}`);
    const label = textElement(document, 'label', `Argument ${index + 1} (i32)`);
    label.htmlFor = input.id;
    const error = textElement(document, 'p', '');
    error.id = `argument-error-${index}`;
    error.className = 'input-error';
    group.append(label, input, error);
    container.append(group);
    fields.push({ input, error });
  }
  if (arity === 0) container.append(textElement(document, 'p', 'This export takes no arguments.'));
  return fields;
}

export function readArguments(fields, focus = false) {
  const values = [];
  let firstInvalid = null;
  for (const { input, error } of fields) {
    try {
      values.push(parseI32(input.value));
      input.setAttribute('aria-invalid', 'false');
      error.textContent = '';
    } catch (failure) {
      input.setAttribute('aria-invalid', 'true');
      error.textContent = failure.message;
      firstInvalid ??= input;
    }
  }
  if (firstInvalid && focus) firstInvalid.focus();
  return firstInvalid ? null : values;
}
