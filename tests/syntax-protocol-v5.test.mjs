import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { readFile } from 'node:fs/promises';
import { createRequire } from 'node:module';
import test from 'node:test';

const Ajv2020 = createRequire(import.meta.url)('ajv/dist/2020.js').default;
const root = new URL('../', import.meta.url);
const json = async path => JSON.parse(await readFile(new URL(path, root), 'utf8'));
const schema = await json('schemas/zryna-syntax-v5.schema.json');
const fixture = await json('tests/m7-syntax-fixtures/reference.json');
const validate = new Ajv2020({ strict: true, allErrors: true }).compile(schema);
const validator = name => new Ajv2020({ strict: true }).compile({
  $ref: `#/$defs/${name}`, $defs: schema.$defs,
});
const names = name => schema.$defs[name].oneOf.flatMap(entry => {
  const definition = entry.$ref ? schema.$defs[entry.$ref.slice(8)] : entry;
  return definition.properties.kind.enum ?? [definition.properties.kind.const];
});
const span = { file: 0, start: 0, end: 1 };
const identifier = { text: 'T', span };

test('v5 independently freezes the complete raw tag inventory', () => {
  assert.equal(validate(fixture), true, JSON.stringify(validate.errors));
  assert.equal(schema.properties.schema_version.const, 5);
  assert.deepEqual(names('typeKind'), ['missing', 'named', 'string', 'vec', 'shared',
    'weak', 'borrow', 'borrow-mut', 'fixed-array', 'application']);
  assert.deepEqual(names('dataDeclarationKind'), ['struct', 'enum']);
  assert.deepEqual(names('expressionKind'), ['reference', 'bool-literal', 'i32-literal',
    'string-literal', 'negation', 'addition', 'subtraction', 'multiplication', 'equal',
    'not-equal', 'less-than', 'less-equal', 'greater-than', 'greater-equal', 'call',
    'struct-construction', 'enum-construction', 'fixed-array-construction',
    'vec-construction', 'field-access', 'index', 'clone', 'shared', 'downgrade',
    'borrow', 'borrow-mut', 'vec-push', 'match']);
  assert.deepEqual(names('statementKind'), ['local-declaration', 'assignment', 'return',
    'block', 'if', 'while', 'expression-statement', 'weak-upgrade']);
});

test('every nested object closes unknown fields and requires nullable fields explicitly', () => {
  const objects = [];
  const walk = (value, path = []) => {
    if (Array.isArray(value)) value.forEach((item, index) => walk(item, [...path, index]));
    else if (value && typeof value === 'object') {
      objects.push(path);
      for (const [key, child] of Object.entries(value)) walk(child, [...path, key]);
    }
  };
  walk(fixture);
  const at = (value, path) => path.reduce((item, key) => item[key], value);
  for (const path of objects) {
    let changed = structuredClone(fixture);
    at(changed, path).unknown_claim = true;
    assert.equal(validate(changed), false, `unknown ${path.join('.')}`);
    for (const key of Object.keys(at(fixture, path))) {
      changed = structuredClone(fixture);
      delete at(changed, path)[key];
      assert.equal(validate(changed), false, `omitted ${[...path, key].join('.')}`);
    }
  }
});

test('one/two parameters and explicit arguments have independent exact/first-extra schema gates', () => {
  const parameter = { span, name: identifier, extends_span: span,
    bound: { text: 'ZrynaValue', span } };
  const parameters = validator('typeParameterList');
  const arguments_ = validator('typeArgumentList');
  for (const count of [0, 1, 2, 3]) {
    const common = { span, less_than_span: span, comma_spans: [], greater_than_span: span };
    assert.equal(parameters({ ...common, parameters: Array(count).fill(parameter) }),
      count === 1 || count === 2);
    assert.equal(arguments_({ ...common, arguments: Array(count).fill(0) }),
      count === 1 || count === 2);
  }
  const altered = structuredClone(fixture);
  altered.files[0].functions[0].body.expressions.find(expression =>
    expression.kind.kind === 'enum-construction').kind.type_arguments = null;
  assert.equal(validate(altered), true, 'omitted application is syntax for later M7001');
  const unknownBound = { ...parameter, bound: { text: 'OtherValue', span } };
  assert.equal(validator('typeParameter')(unknownBound), true,
    'source-authenticated unknown bound belongs to D7001');
});

test('reference modules pin actual UTF-8 ranges independently of a provider', async () => {
  for (const unit of fixture.files) {
    const bytes = await readFile(new URL(`tests/m7-syntax-fixtures/${unit.path}`, root));
    const text = bytes.toString('utf8');
    assert.equal(Buffer.from(text).equals(bytes), true);
    const read = range => {
      assert.equal(range.file, unit.id);
      assert.ok(range.start <= range.end && range.end <= bytes.length);
      const fragment = bytes.subarray(range.start, range.end);
      assert.equal(Buffer.from(fragment.toString('utf8')).equals(fragment), true);
      return fragment.toString('utf8');
    };
    const walk = value => {
      if (Array.isArray(value)) return value.forEach(walk);
      if (!value || typeof value !== 'object') return;
      if (Object.keys(value).sort().join(',') === 'end,file,start') read(value);
      if ('text' in value && 'span' in value) assert.equal(read(value.span), value.text);
      for (const [key, child] of Object.entries(value)) {
        if (key === 'kind' && child && typeof child === 'object' && 'spelling' in child) {
          assert.equal(read(value.span), child.spelling);
        } else walk(child);
      }
    };
    walk(unit);
    for (let index = 1; index < unit.type_syntax.length; index++) {
      assert.ok(unit.type_syntax[index - 1].span.end <= unit.type_syntax[index].span.end,
        'fixed type occurrences preserve lexical postorder');
    }
    for (const declaration of [...unit.data_declarations, ...unit.functions]) {
      if (!declaration.type_parameters) continue;
      for (const parameter of declaration.type_parameters.parameters) {
        assert.equal(read(parameter.extends_span), 'extends');
        assert.equal(read(parameter.bound.span), 'ZrynaValue');
        assert.match(read(parameter.span), /^[TE] extends ZrynaValue$/);
      }
    }
  }
  const template = fixture.files[1].data_declarations[0];
  assert.equal(template.span.start, Buffer.byteLength('// π: exact UTF-8 spans\n'));
});

test('reference calls preserve member placement, all standard variants, and distinct annotations', () => {
  const unit = fixture.files[0];
  const expressions = unit.functions[0].body.expressions;
  const constructors = expressions.filter(expression => expression.kind.kind === 'enum-construction');
  assert.deepEqual(constructors.map(({ kind }) => `${kind.type_name.text}.${kind.variant.text}`),
    ['Option.none', 'Option.some', 'Result.ok', 'Result.err', 'Choice.none']);
  for (const { kind } of constructors) {
    assert.ok(kind.variant.span.end <= kind.type_arguments.span.start);
    assert.ok(kind.type_arguments.span.end <= kind.open_paren_span.start);
    assert.ok(kind.type_name.span.end <= kind.dot_span.start);
  }
  const annotations = unit.type_syntax.filter(type => type.kind.kind === 'application');
  assert.ok(annotations.some(type => type.kind.name.text === 'Box'));
  assert.equal(new Set(annotations.map(type => type.span.start)).size, annotations.length);
});

test('v2/v3/v4 schemas remain independent and reject the successor grammar', async () => {
  for (const version of [2, 3, 4]) {
    const old = await json(`schemas/zryna-syntax-v${version}.schema.json`);
    const check = new Ajv2020({ strict: true }).compile(old);
    assert.equal(check(fixture), false);
    const downgrade = structuredClone(fixture);
    downgrade.schema_version = version;
    assert.equal(check(downgrade), false);
    if (version === 4) {
      const type = new Ajv2020({ strict: true }).compile({ $ref: '#/$defs/typeSyntax', $defs: old.$defs });
      const application = fixture.files[0].type_syntax.find(node => node.kind.kind === 'application');
      assert.equal(type(application), false);
      const original = await json('tests/m3-fixtures/syntax-v4-valid.json');
      assert.equal(check(original), true);
    }
  }
});

test('source and wire fixture digests freeze independent reference bytes', async () => {
  const manifest = await json('tests/m7-syntax-fixtures/digests.json');
  for (const { path, sha256 } of manifest) {
    const bytes = await readFile(new URL(path, root));
    assert.equal(createHash('sha256').update(bytes).digest('hex'), sha256, path);
  }
});

test('fixed hostile source claims stay structurally decodable and target existing records', async () => {
  const cases = await json('tests/m7-syntax-fixtures/hostile-declarations.json');
  for (const entry of cases) {
    const changed = structuredClone(fixture);
    const keys = entry.pointer.slice(1).split('/');
    const key = keys.pop();
    const parent = keys.reduce((value, part) => value[part], changed);
    assert.ok(Object.hasOwn(parent, key), entry.id);
    parent[key] = entry.value;
    assert.equal(validate(changed), true, `${entry.id}: raw shape rejects before source authority`);
    assert.equal(entry.code, 'ZRYNA-Y5001');
  }
  const wire = await json('tests/m7-syntax-fixtures/hostile-wire.json');
  assert.deepEqual(wire.map(entry => entry.id), ['duplicate-version', 'duplicate-nested-code',
    'trailing-value', 'unknown-root-field', 'wrong-version']);
  assert.ok(wire.every(entry => entry.code === 'ZRYNA-Y5001'));
  // Duplicate-key and source checks belong to Rust; schema success is not decoder execution.
});
