import { readFile } from 'node:fs/promises';

const handshake = { id: 4294967295, method: 'handshake' };
const source = 'export function value(input: i32): i32 { return input + 1; }';
const analyze = (id, text, path = 'src/main.zry') => ({
  id, method: 'analyze', params: { schema_version: 4, files: [{ path, text }] },
});
const lines = (requests, newline = true) => Buffer.from(
  requests.map((request) => typeof request === 'string' ? request : JSON.stringify(request))
    .join('\n') + (newline ? '\n' : ''),
);

const fixtures = [
  'syntax-v4-shorthand.zry', 'exclusive-root-borrow.zry',
  'pair-score-v4.zry', 'conformance/enum-body.zry',
  'conformance/owned-vec-body.zry', 'finite-recursive-composition.zry',
];
const fixtureRequests = await Promise.all(fixtures.map(async (name, index) => analyze(
  index, await readFile(new URL(`../../../../tests/m3-fixtures/${name}`, import.meta.url), 'utf8'),
  `src/${name}`,
)));

// Wire digests are captured from original main 0635c19f, before extraction.
// Include the final newline, serialized property order, and exact error messages.
export const compatibilityCases = [
  {
    name: 'source snapshots and UTF-8 CRLF spans',
    input: lines([
      handshake, ...fixtureRequests, analyze(6, `// 😀\r\n${source}`),
      { id: 7, method: 'analyze', params: { schema_version: 4, files: [
        { path: 'z.zry', text: source }, { path: 'a.zry', text: source },
      ] } },
    ]),
    sha256: '9aa56fc3d83810607a1164910ff0fe57872d2873ec466dde23555e35727c0212',
  },
  {
    name: 'strict request and source rejection with recovery',
    input: lines([
      ' ', '{', '{"id":0,"id":1,"method":"handshake"}',
      null, [], { id: -1, method: 'handshake' },
      { id: 0, method: 'handshake', extra: true },
      { id: 1, method: 'analyze', params: { schema_version: 3, files: [] } },
      analyze(2, source, 'CON.zry'), analyze(3, source, '../escape.zry'),
      analyze(4, '\ud800'), analyze(5, '#!unsupported\n'),
      analyze(6, 'export function broken('), analyze(7, 'class Unsupported {}'),
      analyze(8, 'export function f(): i32 { return true && false; }'), handshake,
    ], false),
    sha256: '7f07f7ce5d55cb61721a6d071a73be1781bbdecb98094b5959e9e0e3577f533a',
  },
  {
    name: 'invalid UTF-8 and unterminated final request',
    input: Buffer.concat([Buffer.from([0xff, 0x0a]), lines([handshake], false)]),
    sha256: '24ee032d487c3df63d1d9dc6414b363367fc909220c3262b5cc06ed64544b2a7',
  },
  {
    name: 'request overflow discarding and recovery',
    env: { NODE_ENV: 'test', ZRYNA_TEST_REQUEST_BYTES: '64' },
    input: lines(['x'.repeat(65), handshake]),
    sha256: 'f1476f1914661dbbed42da9e036f8cac0eab5fc806cc6c6441ce3f507f777f0f',
  },
  {
    name: 'response overflow and recovery',
    env: { NODE_ENV: 'test', ZRYNA_TEST_RESPONSE_BYTES: '300' },
    input: lines([analyze(0, source), handshake]),
    sha256: 'ae9619c5954599c444a7825f1534b4617f0c7d15f41e7b57b36555c5d95f71c6',
  },
  {
    name: 'source and syntax limits recover on the same process',
    env: { NODE_ENV: 'test', ZRYNA_TEST_FUNCTIONS_PER_FILE: '1' },
    input: lines([analyze(0, source), analyze(1, `${source}\n${source}`), handshake]),
    sha256: '785f973983a81376a7ec922e3a74fc37cb2f59d80715f93b203b12ed62168422',
  },
  {
    name: 'production ignores lowered test limits',
    env: { NODE_ENV: 'production', ZRYNA_TEST_RESPONSE_BYTES: '1' },
    input: lines([analyze(0, source), handshake]),
    sha256: 'eca9dd6dc23946faedd1e125667148a38488297b8a2ae63a019fd2edb783d234',
  },
];
