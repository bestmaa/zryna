'use strict';

const { test } = require('node:test');
const assert = require('node:assert/strict');
const { mkdtempSync, writeFileSync, rmSync } = require('node:fs');
const { tmpdir } = require('node:os');
const { join, resolve } = require('node:path');
const { spawnSync } = require('node:child_process');

const launcher = resolve(__dirname, '../../../scripts/run-editor-host-acceptance.mjs');

for (const separator of [[], ['--']]) {
  test(`host acceptance ${separator.length ? 'pnpm separator' : 'direct'} invocation validates setup before launching Code`, () => {
    const directory = mkdtempSync(join(tmpdir(), 'zryna-host-cli-test-'));
    try {
      writeFileSync(join(directory, 'setup.json'), '{}\n');
      const result = spawnSync(process.execPath, [launcher, ...separator, directory, 'a'.repeat(64), '--prepare-trust'], {
        env: { ...process.env, TMPDIR: directory, TMP: directory, TEMP: directory },
        encoding: 'utf8', timeout: 10000, maxBuffer: 16384, windowsHide: true,
      });
      assert.ifError(result.error);
      assert.equal(result.status, 1, result.stderr);
      assert.match(result.stderr, /Setup digest differs from the supplied reviewed digest/);
      assert.doesNotMatch(result.stderr, /Usage:/);
    } finally {
      rmSync(directory, { recursive: true, force: true });
    }
  });
}
