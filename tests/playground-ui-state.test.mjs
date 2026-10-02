import assert from 'node:assert/strict';
import { test } from 'node:test';
import { consumeCapability, mountPlayground } from '../examples/playground/restricted/main.mjs';

// These DOM/controller doubles probe UI races; they provide no compiler/browser evidence.
class Element {
  constructor(tag) {
    this.tagName = tag; this.children = []; this.events = {}; this.attributes = {};
    this.disabled = true; this.value = '';
  }
  set innerHTML(value) { throw new Error(`HTML parsing attempted: ${value}`); }
  set textContent(value) { this.text = String(value); this.children = []; }
  get textContent() { return (this.text ?? '') + this.children.map(child => child.textContent).join(''); }
  append(...children) { this.children.push(...children); }
  replaceChildren(...children) { this.text = ''; this.children = children; }
  setAttribute(key, value) { this.attributes[key] = String(value); }
  addEventListener(key, callback) { (this.events[key] ??= []).push(callback); }
  dispatch(key, event = {}) { for (const callback of this.events[key] ?? []) callback(event); }
  focus() { this.focused = true; }
}

function deferred() {
  let resolve; let reject;
  const promise = new Promise((yes, no) => { resolve = yes; reject = no; });
  return { promise, resolve, reject };
}

function fixture() {
  const elements = Object.fromEntries(['preset', 'source', 'byte-count', 'compile', 'cancel',
    'status', 'diagnostic-empty', 'diagnostics', 'export', 'arguments', 'run', 'observation',
    'identity', 'source-error'].map(id => [id, new Element(id)]));
  const document = { getElementById: id => elements[id], createElement: tag => new Element(tag) };
  const compilation = deferred(); const evaluation = deferred();
  const controller = {
    revision: 1, busy: false, cancellations: 0, compileCalls: 0, evaluationCalls: [],
    edit(source) { this.source = source; return ++this.revision; },
    cancel() { this.cancellations++; return this.busy; },
    async compile() {
      this.compileCalls++; this.busy = true;
      try { return await compilation.promise; } finally { this.busy = false; }
    },
    async evaluate(logical, args) {
      this.evaluationCalls.push({ logical, args }); this.busy = true;
      try { return await evaluation.promise; } finally { this.busy = false; }
    },
  };
  const ui = mountPlayground(document, controller);
  const edit = source => { elements.source.value = source; elements.source.dispatch('input'); };
  const result = (overrides = {}) => ({ revision: controller.revision, sourceSha256: 'a'.repeat(64),
    status: 'compiled', report: { schema_version: 1, diagnostics: [] }, failure: null,
    identity: { componentSha256: 'b'.repeat(64) }, exports: [
      { logical: 'add', component: 'zryna-export-616464', core: 'add', arity: 2 },
      { logical: 'zero', component: 'zryna-export-7a65726f', core: 'zero', arity: 0 }], ...overrides });
  return { elements, compilation, evaluation, controller, ui, edit, result };
}

async function compileFixture(f) {
  const pending = f.ui.compile();
  f.compilation.resolve(f.result());
  await pending;
}

test('session capability is consumed before validation and never kept in history state', () => {
  const changes = [];
  const history = { replaceState: (...args) => changes.push(args) };
  assert.equal(consumeCapability({ hash: `#${'a'.repeat(64)}`, pathname: '/playground', search: '?view=source' },
    history), 'a'.repeat(64));
  assert.deepEqual(changes, [[null, '', '/playground?view=source']]);
  assert.throws(() => consumeCapability({ hash: '#<script>', pathname: '/playground', search: '' }, history),
    /CAPABILITY/);
  assert.deepEqual(changes[1], [null, '', '/playground']);
});

test('editing during compilation suppresses old output and holds admission through cleanup', async () => {
  const f = fixture(); const old = f.result();
  const pending = f.ui.compile();
  f.edit('export function changed(): i32 { return 7; }');
  assert.equal(f.elements.compile.disabled, true);
  assert.equal(f.elements.source.disabled, false);
  await f.ui.compile();
  assert.equal(f.controller.compileCalls, 1);
  f.compilation.resolve(old);
  await pending;
  assert.equal(f.elements.compile.disabled, false);
  assert.equal(f.elements.run.disabled, true);
  assert.equal(f.elements.identity.textContent, '');
  assert.equal(f.elements.diagnostics.children.length, 0);
  assert.match(f.elements.status.textContent, /Source edited/);
});

test('an old failure cannot replace edited state but teardown failure ends the session', async () => {
  for (const failure of ['PLAYGROUND-CANCELLED', 'PLAYGROUND-HOST-TEARDOWN']) {
    const f = fixture(); const pending = f.ui.compile();
    f.edit('new source');
    f.compilation.reject(new Error(failure));
    await pending;
    if (failure.endsWith('TEARDOWN')) {
      assert.equal(f.elements.compile.disabled, true);
      assert.equal(f.elements.source.disabled, true);
      assert.match(f.elements.status.textContent, /cleanup could not be confirmed/);
    } else {
      assert.equal(f.elements.compile.disabled, false);
      assert.match(f.elements.status.textContent, /Source edited/);
    }
  }
});

test('cancel blocks a late same-revision success until cleanup settles', async () => {
  const f = fixture(); const pending = f.ui.compile();
  f.elements.cancel.dispatch('click');
  assert.equal(f.elements.cancel.disabled, true);
  assert.equal(f.elements.compile.disabled, true);
  f.compilation.resolve(f.result());
  await pending;
  assert.equal(f.elements.run.disabled, true);
  assert.equal(f.elements.compile.disabled, false);
  assert.match(f.elements.status.textContent, /Cancelled/);
  assert.equal(f.elements.identity.textContent, '');
});

test('UTF-8 byte limit is inclusive and over-limit editing invalidates a compiled revision', async () => {
  const f = fixture();
  await compileFixture(f);
  f.edit('😀'.repeat(1024));
  assert.equal(f.elements['byte-count'].textContent, '4096 / 4096 UTF-8 bytes');
  assert.equal(f.elements.compile.disabled, false);
  assert.equal(f.elements.run.disabled, true);
  f.edit('😀'.repeat(1024) + 'x');
  assert.equal(f.elements.compile.disabled, true);
  assert.match(f.elements['source-error'].textContent, /Remove 1 byte before compiling/);
  assert.equal(f.elements.source.attributes['aria-invalid'], 'true');
  assert.equal(f.elements.identity.textContent, '');
  f.edit('\ud800');
  assert.match(f.elements['byte-count'].textContent, /Invalid Unicode/);
  assert.equal(f.elements.compile.disabled, true);
  f.edit('valid');
  assert.equal(f.elements.compile.disabled, false);
});

test('export arity determines fields and observed values come only from evaluation', async () => {
  const f = fixture();
  await compileFixture(f);
  const groups = f.elements.arguments.children;
  assert.equal(groups.length, 2);
  assert.equal(groups[0].children[1].value, '20');
  assert.equal(groups[1].children[1].value, '22');
  assert.equal(f.elements.observation.textContent, 'No observation.');
  groups[0].children[1].value = '-2147483648';
  groups[1].children[1].value = '0';
  const pending = f.ui.run();
  assert.equal(groups[0].children[1].disabled, true);
  assert.deepEqual(f.controller.evaluationCalls, [{ logical: 'add', args: [-2147483648, 0] }]);
  f.evaluation.resolve({ revision: f.controller.revision, logical: 'add', value: -2147483648,
    componentSha256: 'b'.repeat(64) });
  await pending;
  assert.match(f.elements.observation.textContent, /= -2147483648 \(signed i32\)/);
  assert.match(f.elements.identity.textContent, /observed component SHA-256 b{64}/);
  f.elements.export.value = 'zero';
  f.elements.export.dispatch('change');
  assert.match(f.elements.arguments.textContent, /no arguments/);
  assert.equal(f.elements.observation.textContent, 'No observation.');
});

test('invalid numeric input focuses the first error and never calls evaluation', async () => {
  const f = fixture(); await compileFixture(f);
  const input = f.elements.arguments.children[0].children[1];
  input.value = '-0';
  await f.ui.run();
  assert.equal(input.focused, true);
  assert.equal(input.attributes['aria-invalid'], 'true');
  assert.equal(f.controller.evaluationCalls.length, 0);
  assert.match(f.elements.observation.textContent, /highlighted argument/);
});

test('editing or cancelling an evaluation suppresses late observations', async () => {
  for (const change of ['edit', 'cancel']) {
    const f = fixture(); await compileFixture(f);
    const revision = f.controller.revision;
    const pending = f.ui.run();
    if (change === 'edit') f.edit('different source');
    else f.elements.cancel.dispatch('click');
    f.evaluation.resolve({ revision, logical: 'add', value: 42, componentSha256: 'b'.repeat(64) });
    await pending;
    assert.equal(f.elements.observation.textContent, 'No observation.');
    assert.equal(f.elements.compile.disabled, false);
    assert.equal(f.elements.run.disabled, change === 'edit');
  }
});

test('unavailable compiler errors remain inert availability text without source diagnostics', async () => {
  const f = fixture(); const pending = f.ui.compile();
  f.compilation.resolve(f.result({ status: 'unavailable', identity: null, exports: [],
    failure: { code: 'ZRYNA-F1001', message: '<img src=x onerror="throw 1">' } }));
  await pending;
  assert.match(f.elements.status.textContent, /<img src=x onerror="throw 1">/);
  assert.equal(f.elements.diagnostics.children.length, 0);
  assert.equal(f.elements['diagnostic-empty'].textContent, 'No source diagnostics were produced.');
  assert.equal(f.elements.run.disabled, true);
});

test('rejected compilation presents compiler diagnostics and leaves source text unchanged', async () => {
  const f = fixture();
  const source = '<script>throw 1</script>';
  f.edit(source);
  const pending = f.ui.compile();
  f.compilation.resolve(f.result({ status: 'rejected', identity: null, exports: [],
    report: { schema_version: 1, diagnostics: [{ code: 'TS1005', severity: 'error', path: null,
      byte_start: null, byte_end: null, line_start: null, column_start: null,
      line_end: null, column_end: null, message: '<b>invalid source</b>', guidance: 'Edit source.' }] } }));
  await pending;
  assert.equal(f.elements.source.value, source);
  assert.ok(f.elements.diagnostics.textContent.includes('<b>invalid source</b>'));
  assert.equal(f.elements.run.disabled, true);
  assert.match(f.elements.status.textContent, /Compilation rejected/);
});

test('keyboard compilation and example selection follow the same revision lifecycle', async () => {
  const f = fixture();
  let prevented = false;
  f.elements.source.dispatch('keydown', { ctrlKey: true, key: 'Enter',
    preventDefault() { prevented = true; } });
  assert.equal(prevented, true);
  assert.equal(f.controller.compileCalls, 1);
  const old = f.result();
  f.elements.preset.value = 'edited-literal';
  f.elements.preset.dispatch('change');
  assert.match(f.elements.source.value, /return a \+ 3/);
  assert.equal(f.elements.source.focused, true);
  assert.equal(f.elements.compile.disabled, true);
  f.compilation.resolve(old);
  await new Promise(resolve => setImmediate(resolve));
  assert.equal(f.elements.run.disabled, true);
  assert.equal(f.elements.compile.disabled, false);
  assert.equal(f.elements.identity.textContent, '');
});

test('mismatched revision or component observations cannot publish signed results', async () => {
  for (const overrides of [{ revision: 99 }, { componentSha256: 'c'.repeat(64) },
    { logical: 'zero' }, { value: -0 }, { value: 2147483648 }]) {
    const f = fixture(); await compileFixture(f);
    const pending = f.ui.run();
    f.evaluation.resolve({ revision: f.controller.revision, logical: 'add', value: 42,
      componentSha256: 'b'.repeat(64), ...overrides });
    await pending;
    assert.match(f.elements.observation.textContent, /Evaluation failed/);
    assert.doesNotMatch(f.elements.observation.textContent, /signed i32/);
  }
});
