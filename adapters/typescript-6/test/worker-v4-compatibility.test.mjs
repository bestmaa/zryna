import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import test from 'node:test';
import { fileURLToPath } from 'node:url';

import { compatibilityCases } from './fixtures/v4-compatibility-cases.mjs';

const adapterRoot = fileURLToPath(new URL('../', import.meta.url));
for (const sample of compatibilityCases) {
  test(`v4 retains original main wire bytes: ${sample.name}`, () => {
    const env = { ...process.env };
    delete env.NODE_ENV;
    for (const key of Object.keys(env)) {
      if (key.startsWith('ZRYNA_TEST_')) delete env[key];
    }
    const result = spawnSync(process.execPath, ['src/worker-v4.mjs'], {
      cwd: adapterRoot, input: sample.input, env: { ...env, ...sample.env },
      timeout: 30_000, maxBuffer: 16 * 1024 * 1024, windowsHide: true,
    });
    assert.ifError(result.error);
    assert.equal(result.status, 0, result.stderr.toString('utf8'));
    assert.equal(result.stderr.length, 0);
    assert.equal(createHash('sha256').update(result.stdout).digest('hex'), sample.sha256);
  });
}
