import assert from 'node:assert/strict';
import { mkdirSync, mkdtempSync, readFileSync, readdirSync, renameSync, rmSync, symlinkSync, writeFileSync } from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import test from 'node:test';
import { retainProtectedDocsOutput } from '../scripts/docs/protected-output.mjs';

const linux = process.platform === 'linux';
const outputs = [['documents/reference/support.md', Buffer.from('support fixture\n')],
  ['documents/reference/conformance.md', Buffer.from('conformance fixture\n')],
  ['manifest.json', Buffer.from('manifest fixture\n')], ['manifest.sha256', Buffer.from('checksum fixture\n')]];

function fixture(context) {
  const root = mkdtempSync(path.join(os.tmpdir(), 'zryna-docs-output-'));
  context.after(() => {
    assert.equal(path.dirname(root), path.resolve(os.tmpdir()));
    assert(path.basename(root).startsWith('zryna-docs-output-'));
    rmSync(root, { recursive: true, force: true });
  });
  const source = path.join(root, 'source'), parent = path.join(root, 'output-parent'), output = path.join(parent, 'bundle');
  mkdirSync(source); mkdirSync(parent);
  writeFileSync(path.join(source, 'source.txt'), 'source fixture\n');
  return { root, source, parent, output };
}

test('anchored protected writer retains exact nested bytes and closes every owned descriptor', { skip: !linux }, t => {
  const f = fixture(t), before = readdirSync('/proc/self/fd').length;
  retainProtectedDocsOutput(f.output, f.source, outputs);
  for (const [name, data] of outputs) assert.deepEqual(readFileSync(path.join(f.output, name)), data);
  assert.equal(readdirSync('/proc/self/fd').length, before);
});

test('a preexisting output is never replaced or cleaned', { skip: !linux }, t => {
  const f = fixture(t);
  mkdirSync(f.output); writeFileSync(path.join(f.output, 'marker'), 'existing\n');
  assert.throws(() => retainProtectedDocsOutput(f.output, f.source, outputs), /EEXIST/);
  assert.equal(readFileSync(path.join(f.output, 'marker'), 'utf8'), 'existing\n');
});

test('source aliases and output ancestor aliases reject before any document write', { skip: !linux }, t => {
  const f = fixture(t), alias = path.join(f.root, 'alias');
  symlinkSync(f.source, alias, 'dir');
  assert.throws(() => retainProtectedDocsOutput(path.join(alias, 'bundle'), f.source, outputs));
  assert.deepEqual(readdirSync(f.source), ['source.txt']);
  assert.throws(() => retainProtectedDocsOutput(f.output, alias, outputs));
  assert.deepEqual(readdirSync(f.parent), []);
});

test('renamed output ancestry preserves its partial and never follows a replacement', { skip: !linux }, t => {
  const f = fixture(t), displaced = path.join(f.root, 'old-parent'), before = readdirSync('/proc/self/fd').length;
  assert.throws(() => retainProtectedDocsOutput(f.output, f.source, outputs, index => {
    if (index === 0) { renameSync(f.parent, displaced); mkdirSync(f.parent); }
  }));
  assert.deepEqual(readdirSync(path.join(displaced, 'bundle')), []);
  assert.deepEqual(readdirSync(f.parent), []);
  assert.equal(readdirSync('/proc/self/fd').length, before);
});

test('renamed nested output directory rejects before manifest completion', { skip: !linux }, t => {
  const f = fixture(t), before = readdirSync('/proc/self/fd').length;
  assert.throws(() => retainProtectedDocsOutput(f.output, f.source, outputs, index => {
    if (index === 1) {
      renameSync(path.join(f.output, 'documents'), path.join(f.output, 'old-documents'));
      mkdirSync(path.join(f.output, 'documents'));
    }
  }));
  assert.equal(readFileSync(path.join(f.output, 'old-documents/reference/support.md'), 'utf8'), 'support fixture\n');
  assert.deepEqual(readdirSync(path.join(f.output, 'documents')), []);
  assert(!readdirSync(f.output).includes('manifest.json'));
  assert.equal(readdirSync('/proc/self/fd').length, before);
});

test('replacement of a completed file cannot be hidden by equal byte count', { skip: !linux }, t => {
  const f = fixture(t);
  assert.throws(() => retainProtectedDocsOutput(f.output, f.source, outputs, index => {
    if (index === 1) {
      const file = path.join(f.output, outputs[0][0]);
      renameSync(file, file + '.preserved');
      writeFileSync(file, Buffer.alloc(outputs[0][1].length, 0x78));
    }
  }));
  assert.deepEqual(readFileSync(path.join(f.output, outputs[0][0] + '.preserved')), outputs[0][1]);
  assert.deepEqual(readFileSync(path.join(f.output, outputs[0][0])), Buffer.alloc(outputs[0][1].length, 0x78));
  assert(!readdirSync(f.output).includes('manifest.json'));
});

for (const [name, mutate] of [
  ['truncate', file => writeFileSync(file, Buffer.alloc(0))],
  ['same-size overwrite', file => writeFileSync(file, Buffer.alloc(outputs[0][1].length, 0x78))],
  ['unlink', file => rmSync(file)],
]) {
  test(`completed document ${name} rejects before the manifest is written`, { skip: !linux }, t => {
    const f = fixture(t), before = readdirSync('/proc/self/fd').length;
    assert.throws(() => retainProtectedDocsOutput(f.output, f.source, outputs, index => {
      if (index === 2) mutate(path.join(f.output, outputs[0][0]));
    }));
    assert.deepEqual(readFileSync(path.join(f.output, outputs[1][0])), outputs[1][1]);
    assert(!readdirSync(f.output).includes('manifest.json'));
    assert(!readdirSync(f.output).includes('manifest.sha256'));
    assert.equal(readdirSync('/proc/self/fd').length, before);
  });
}

test('source ancestry replacement rejects even though committed inputs were already captured', { skip: !linux }, t => {
  const f = fixture(t), displaced = path.join(f.root, 'old-source');
  assert.throws(() => retainProtectedDocsOutput(f.output, f.source, outputs, index => {
    if (index === 0) { renameSync(f.source, displaced); mkdirSync(f.source); }
  }));
  assert.equal(readFileSync(path.join(displaced, 'source.txt'), 'utf8'), 'source fixture\n');
  assert.deepEqual(readdirSync(f.output), []);
});

test('a checksum arriving before completion is never overwritten', { skip: !linux }, t => {
  const f = fixture(t);
  assert.throws(() => retainProtectedDocsOutput(f.output, f.source, outputs, index => {
    if (index === 3) writeFileSync(path.join(f.output, 'manifest.sha256'), 'foreign arrival\n');
  }), /EEXIST/);
  assert.equal(readFileSync(path.join(f.output, 'manifest.sha256'), 'utf8'), 'foreign arrival\n');
});

test('callback failure preserves independently closed partials and does not leak FDs', { skip: !linux }, t => {
  const f = fixture(t), before = readdirSync('/proc/self/fd').length;
  assert.throws(() => retainProtectedDocsOutput(f.output, f.source, outputs, index => {
    if (index === 1) throw new Error('interrupted captured output');
  }), /interrupted captured output/);
  assert.deepEqual(readFileSync(path.join(f.output, outputs[0][0])), outputs[0][1]);
  assert(!readdirSync(f.output).includes('manifest.json'));
  assert.equal(readdirSync('/proc/self/fd').length, before);
});

test('duplicate, parent-file collision and oversized inventories reject before creation', { skip: !linux }, t => {
  const f = fixture(t);
  for (const invalid of [[['same', Buffer.alloc(0)], ['same', Buffer.alloc(0)]],
    [['documents', Buffer.alloc(0)], ['documents/file.md', Buffer.alloc(0)]],
    [['oversized.md', Buffer.alloc(2097153)]], Array.from({ length: 515 }, (_, index) => [`${index}.md`, Buffer.alloc(0)])]) {
    assert.throws(() => retainProtectedDocsOutput(f.output, f.source, invalid), /inventory rejected/);
    assert.deepEqual(readdirSync(f.parent), []);
  }
});

test('unsupported output platform cannot create a directory', { skip: linux }, t => {
  const f = fixture(t);
  assert.throws(() => retainProtectedDocsOutput(f.output, f.source, outputs), /requires Linux/);
  assert.deepEqual(readdirSync(f.parent), []);
});
