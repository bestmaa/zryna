import assert from 'node:assert/strict';
import test from 'node:test';

import {
  REQUIRED_DIFFERENTIAL_TEST,
  REQUIRED_DIAGNOSTIC_TEST,
  REQUIRED_RESOURCE_TEST,
  REQUIRED_PROJECT_RESOURCE_TEST,
  runNativeParserProviderDifferential,
  verifyDifferentialOutput,
} from '../scripts/run-native-parser-provider-differential.mjs';

const success = `test ${REQUIRED_DIFFERENTIAL_TEST} ... ok
test ${REQUIRED_DIAGNOSTIC_TEST} ... ok
test ${REQUIRED_RESOURCE_TEST} ... ok
test ${REQUIRED_PROJECT_RESOURCE_TEST} ... ok
test result: ok. 1 passed; 0 failed; 0 ignored;`;

test('parser differential output requires the exact nonzero test', () => {
  assert.doesNotThrow(() => verifyDifferentialOutput(success));
  assert.throws(() => verifyDifferentialOutput('test result: ok. 0 passed;'), /did not pass/);
  for (const name of [REQUIRED_DIFFERENTIAL_TEST, REQUIRED_DIAGNOSTIC_TEST, REQUIRED_RESOURCE_TEST, REQUIRED_PROJECT_RESOURCE_TEST]) {
    assert.throws(() => verifyDifferentialOutput(success.replace(`test ${name} ... ok`, '')),
      /did not pass/);
  }
});

test('parser differential runner invokes the locked ignored test without a shell', () => {
  let observed;
  runNativeParserProviderDifferential((executable, args, options) => {
    observed = { executable, args, options };
    return { status: 0, stdout: success, stderr: '' };
  });
  assert.equal(observed.executable, 'cargo');
  assert.deepEqual(observed.args, [
    'test', '--locked', '-p', 'zryna-frontend', '--test', 'native_parser_v4_provider',
    '--test', 'native_parser_parity', '--', '--ignored',
  ]);
  assert.equal(observed.options.shell, false);
});
