import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { readFile } from 'node:fs/promises';
import { createHash } from 'node:crypto';
import test from 'node:test';
import Ajv2020 from 'ajv/dist/2020.js';

const adapter = new URL('../', import.meta.url);
const fixtures = new URL('../../../tests/m7-syntax-fixtures/', import.meta.url);
const root = new URL('../../../', import.meta.url);
const corpus = JSON.parse(await readFile(new URL('tests/provider-conformance-v5/corpus.json', root)));
const json = async name => JSON.parse(await readFile(new URL(name, fixtures), 'utf8'));
const schema = JSON.parse(await readFile(new URL('../../../schemas/zryna-syntax-v5.schema.json', import.meta.url)));
const validate = new Ajv2020({ strict: true, allErrors: true }).compile(schema);
const handshake = { id: 1, method: 'handshake' };
const analyze = (files, id = 2) => ({ id, method: 'analyze', params: { schema_version: 5, files } });
const single = text => [{ path: 'main.zry', text }];

function exchange(requests, env = {}) {
  const input = Array.isArray(requests) ? requests.map(JSON.stringify).join('\n') + '\n' : requests;
  const run = spawnSync(process.execPath, ['src/worker-v5.mjs'], {
    cwd: adapter, input, encoding: 'utf8', timeout: 30_000, maxBuffer: 8 * 1024 * 1024,
    env: { ...process.env, ...env }, windowsHide: true,
  });
  assert.equal(run.error, undefined);
  assert.equal(run.status, 0, run.stderr);
  assert.equal(run.stderr, '');
  return { bytes: run.stdout, responses: run.stdout.trim().split('\n').map(JSON.parse) };
}

test('v5 exact handshake is isolated and syntax-only', () => {
  assert.deepEqual(exchange([handshake]).responses[0], { id: 1, result: {
    provider: 'typescript-6', provider_version: '6.0.3', protocol_version: 5,
    capabilities: { module_resolution: false, semantic_diagnostics: false,
      control_flow_v1: true, data_ownership_syntax_v1: true, bounded_generics_syntax_v1: true },
  } });
});

for (const entry of corpus.cases) {
  test(`v5 agrees exactly with frozen independently authored ${entry.id} DTO in fresh processes`, async () => {
    const expected = JSON.parse(await readFile(new URL(entry.reference, root)));
    const files = await Promise.all(entry.files.map(async unit => ({ path: unit.path,
      text: (await Promise.all(unit.fragments.map(fragment => fragment.text ??
        readFile(new URL(fragment.source, root), 'utf8')))).join('') })));
    const first = exchange([handshake, analyze(files)]);
    const second = exchange([handshake, analyze([...files].reverse())]);
    assert.equal(first.bytes, second.bytes, 'fresh-process and inventory-order determinism');
    const actual = first.responses[1];
    assert.equal(actual.error, undefined, JSON.stringify(actual.error));
    assert.deepEqual(actual.result, expected);
    assert.equal(validate(actual.result), true, JSON.stringify(validate.errors));
  });
}

test('frozen corpus digests include syntax and previously qualified executable reference bytes', async () => {
  assert.equal(corpus.native_v5, 'reserved-later; no native v5 implementation or parity claim');
  for (const artifact of corpus.artifacts) {
    const bytes = await readFile(new URL(artifact.path, root));
    assert.equal(bytes.length, artifact.bytes, artifact.path);
    assert.equal(createHash('sha256').update(bytes).digest('hex'), artifact.sha256, artifact.path);
  }
});

test('v5 generic punctuation owns nested and trailing commas with original UTF-8 spans', () => {
  const text = '// 😀\r\nfunction identity<T extends ZrynaValue,>(x: T): T { return x; }\n'
    + 'function probe(): i32 { return identity<FixedArray<i32, 2>,>(7); }';
  const result = exchange([analyze(single(text))]).responses[0];
  assert.equal(result.error, undefined, JSON.stringify(result.error));
  assert.equal(validate(result.result), true, JSON.stringify(validate.errors));
  const unit = result.result.files[0];
  assert.equal(unit.functions[0].span.start, Buffer.byteLength('// 😀\r\n'));
  assert.equal(unit.functions[0].type_parameters.comma_spans.length, 1);
  const args = unit.functions[1].body.expressions.at(-1).kind.type_arguments;
  assert.equal(args.comma_spans.length, 1, 'nested fixed-array comma is not owned by outer list');
  const source = Buffer.from(text);
  assert.equal(source.subarray(args.span.start, args.span.end).toString(), '<FixedArray<i32, 2>,>');
});

test('v5 omitted arguments and unknown bounds remain syntax, without inferred semantic codes', () => {
  const source = 'function id<T extends OtherValue>(value: T): T { return value; }\n'
    + 'function probe(): i32 { const x: Option<i32> = Option.some(7); return id(7); }';
  const response = exchange([analyze(single(source))]).responses[0];
  assert.equal(response.error, undefined, JSON.stringify(response.error));
  assert.equal(validate(response.result), true, JSON.stringify(validate.errors));
  assert.equal(response.result.files[0].functions[0].type_parameters.parameters[0].bound.text, 'OtherValue');
  for (const node of response.result.files[0].functions[1].body.expressions) {
    if (['call', 'enum-construction'].includes(node.kind.kind)) assert.equal(node.kind.type_arguments, null);
  }
  assert.deepEqual(response.result.diagnostics, []);
});

test('unrepresentable exclusions reject through provider channel and recover deterministically', () => {
  const sources = [
    'function id<T extends ZrynaValue, E extends ZrynaValue, V extends ZrynaValue>(v: T): T { return v; }',
    'function id<T = i32>(v: T): T { return v; }',
    'function id<out T extends ZrynaValue>(v: T): T { return v; }',
    'function p(): i32 { return id<i32, i32, i32>(7); }',
    'function p(): i32 { return Option<i32>.some(7); }',
    'function p(): i32 { return match(7, { "Option.some": (x) => { return x; } }); }',
    'function p(): i32 { const x = 7; return x; }',
    'function p(): i32 { function inner<T extends ZrynaValue>(x: T): T { return x; } return 7; }',
    'type Alias<T> = T;',
  ];
  const requests = sources.flatMap((text, i) => [analyze(single(text), i + 10), handshake]);
  const result = exchange(requests).responses;
  for (let i = 0; i < sources.length; i++) {
    assert.equal(result[i * 2].error?.code, 'ZRYNA-F2002', sources[i]);
    assert.equal(result[i * 2 + 1].result.protocol_version, 5);
    assert.equal(result[i * 2].result, undefined);
  }
});

test('wire rejects duplicates, wrong protocol, unknown fields, invalid UTF-8, and path collisions', () => {
  for (const request of [
    '{"id":2,"id":2,"method":"handshake"}',
    '{"id":2,"method":"handshake","unknown":0}',
    JSON.stringify({ ...analyze([]), params: { schema_version: 4, files: [] } }),
    JSON.stringify(analyze([{ path: 'Main.zry', text: '' }, { path: 'main.zry', text: '' }])),
    Buffer.from([0xff, 0x0a]),
  ]) {
    const { responses } = exchange(request);
    assert.equal(responses[0].error.code, 'ZRYNA-F1001');
  }
});

test('lowered byte/file/nesting/type budgets fail at first extra and preserve handshake recovery', () => {
  const source = 'function p(): i32 { return 7; }';
  const cases = [
    ['SOURCE_BYTES', Buffer.byteLength(source), single(source), single(source + ' ')],
    ['FILES', 1, single(source), [...single(source), { path: 'other.zry', text: '' }]],
    ['FUNCTIONS_PER_PROJECT', 1, single(source), single(source + '\nfunction q(): i32 { return 8; }')],
    ['TYPE_SYNTAX_NODES_PER_PROJECT', 1, single(source), single('function p(x: i32): i32 { return x; }')],
    ['TYPE_SYNTAX_NESTING', 1, single('function p(x: T): i32 { return 7; }'),
      single('function p(x: Option<T>): i32 { return 7; }')],
  ];
  for (const [name, limit, exact, extra] of cases) {
    const responses = exchange([analyze(exact), analyze(extra, 3), handshake],
      { NODE_ENV: 'test', [`ZRYNA_TEST_${name}`]: String(limit) }).responses;
    assert.equal(responses[0].error, undefined, name);
    assert.equal(responses[1].error?.code, 'ZRYNA-F1002', name);
    assert.equal(responses[2].result.protocol_version, 5);
  }
  const requests = exchange([analyze(single(source)), handshake],
    { NODE_ENV: 'test', ZRYNA_TEST_REQUEST_BYTES: '32' }).responses;
  assert.equal(requests[0].error.code, 'ZRYNA-F1002');
  assert.equal(requests[1].result.protocol_version, 5);
});
