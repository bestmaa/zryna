import assert from 'node:assert/strict';
import test from 'node:test';
import { requireExecutedProof, requireExactCandidate } from '../scripts/scalar-host/run-conformance.mjs';

const name = 'fixture::real_browser';
const success = `test ${name} ... ok\ntest result: ok. 1 passed; 0 failed; 0 ignored; 9 filtered out;\n`;
const result = stdout => ({ status: 0, signal: null, stdout });

test('accepts an actual exact test receipt including Windows line endings', () => {
  assert.deepEqual(requireExecutedProof(result(success.replaceAll('\n', '\r\n')), name),
    { test: name, passed: 1, failed: 0, ignored: 0 });
});

test('rejects a successful process that listed, filtered out or ignored the fixture', () => {
  for (const text of ['', `test ${name} ... ignored\ntest result: ok. 0 passed; 0 failed; 1 ignored;`,
    'test result: ok. 0 passed; 0 failed; 0 ignored;', success.replace(name, 'unrelated')]) {
    assert.throws(() => requireExecutedProof(result(text), name));
  }
});

test('a printed pass cannot hide another summary, a failing exit, timeout or signal', () => {
  assert.throws(() => requireExecutedProof(result(`${success}test result: ok. 0 passed; 0 failed; 0 ignored;`), name));
  for (const failure of [{ status: 101 }, { error: new Error('timeout') }, { signal: 'SIGTERM' }]) {
    assert.throws(() => requireExecutedProof({ ...result(success), ...failure }, name));
  }
});

test('rejects changed commits, tracked changes and untracked files in exact-head evidence', () => {
  const commit = 'a'.repeat(40);
  assert.equal(requireExactCandidate(result(`${commit}\r\n`), result(''), commit), commit);
  for (const [identity, state] of [[result('b'.repeat(40)), result('')],
    [result(`${commit}\n`), result(' M fixture.mjs\n')],
    [result(`${commit}\n`), result('?? extra.mjs\n')], [result('not-a-commit'), result('')]]) {
    assert.throws(() => requireExactCandidate(identity, state, commit));
  }
});

test('candidate inspection cannot succeed through a failing process, timeout or signal', () => {
  const identity = result('a'.repeat(40));
  for (const failure of [{ status: 1 }, { error: new Error('timeout') }, { signal: 'SIGTERM' }]) {
    assert.throws(() => requireExactCandidate({ ...identity, ...failure }, result('')));
    assert.throws(() => requireExactCandidate(identity, { ...result(''), ...failure }));
  }
});
