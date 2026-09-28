'use strict';
const { test } = require('node:test');
const assert = require('node:assert/strict');
const { EventEmitter } = require('node:events');
const { PassThrough } = require('node:stream');
const { resolve } = require('node:path');
const { Connection } = require('../src/connection.cjs');

function fixture() {
  let invocation;
  const child = new EventEmitter();
  child.stdin = new PassThrough();
  child.stdout = new PassThrough();
  child.stderr = new PassThrough();
  child.kill = () => { child.killed = true; };
  const errors = [];
  const notifications = [];
  const config = { serverPath: resolve('trusted server'), compilerRoot: resolve('trusted compiler'), nodePath: resolve('trusted node') };
  const connection = new Connection(config, (...args) => notifications.push(args), error => errors.push(error), (...args) => {
    invocation = args;
    return child;
  });
  return { connection, child, config, errors, notifications, invocation };
}

function frame(value) {
  const body = Buffer.from(JSON.stringify({ jsonrpc: '2.0', ...value }));
  return Buffer.concat([Buffer.from(`Content-Length: ${body.length}\r\n\r\n`), body]);
}

test('launch uses explicit executable and arguments without shell or workspace configuration', async () => {
  const f = fixture();
  assert.equal(f.invocation[0], f.config.serverPath);
  assert.deepEqual(f.invocation[1], ['--compiler-root', f.config.compilerRoot, '--node', f.config.nodePath]);
  assert.equal(f.invocation[2].shell, false);
  assert.equal(f.invocation[2].cwd, f.config.compilerRoot);
  f.connection.fail(new Error('test cleanup'));
  assert.equal(f.child.killed, true);
  assert.throws(() => new Connection({ serverPath: 'relative', compilerRoot: '/', nodePath: '/' }), /absolute/);
});

test('M3 launch binds the trusted workspace root without a shell', () => {
  const f = fixture();
  f.connection.fail(new Error('test cleanup'));
  const workspaceRoot = resolve('trusted workspace');
  let invocation;
  const child = new EventEmitter();
  child.stdin = new PassThrough();
  child.stdout = new PassThrough();
  child.stderr = new PassThrough();
  child.kill = () => {};
  const connection = new Connection({ ...f.config, workspaceRoot }, () => {}, () => {}, (...args) => {
    invocation = args;
    return child;
  });
  assert.deepEqual(invocation[1], ['--compiler-root', f.config.compilerRoot,
    '--node', f.config.nodePath, '--workspace-root', workspaceRoot]);
  assert.equal(invocation[2].shell, false);
  connection.fail(new Error('test cleanup'));
  assert.throws(() => new Connection({ ...f.config, workspaceRoot: 'relative' }), /trusted workspace root/);
});

test('fragmented Unicode responses correlate exactly; notifications remain inert', async () => {
  const f = fixture();
  const pending = f.connection.request('test', {});
  const bytes = frame({ id: 1, result: '😀' });
  for (const byte of bytes) f.child.stdout.write(Buffer.from([byte]));
  assert.equal(await pending, '😀');
  f.child.stdout.write(frame({ method: 'window/logMessage', params: { message: '$(anything)' } }));
  assert.equal(f.notifications[0][1].message, '$(anything)');
  f.connection.fail(new Error('test cleanup'));
});

test('cancellation rejects once and ignores late response', async () => {
  const f = fixture();
  let cancel;
  let disposed = false;
  const pending = f.connection.request('test', {}, {
    onCancellationRequested(callback) { cancel = callback; return { dispose() { disposed = true; } }; },
  });
  cancel();
  await assert.rejects(pending, /cancelled/);
  f.child.stdout.write(frame({ id: 1, result: [] }));
  assert.equal(f.connection.pending.size, 0);
  assert.equal(disposed, true);
  f.connection.fail(new Error('test cleanup'));
});

test('malformed and oversized frames stop the process and reject all pending work', async () => {
  for (const malformed of ['Content-Length: 16777217\r\n\r\n', 'Content-Length: 1\r\nContent-Length: 1\r\n\r\n{', 'x'.repeat(1025)]) {
    const f = fixture();
    const pending = f.connection.request('test', {});
    f.child.stdout.write(Buffer.from(malformed));
    await assert.rejects(pending, /Invalid language-server/);
    assert.equal(f.connection.closed, true);
    assert.equal(f.child.killed, true);
  }
});

test('graceful shutdown sends exit and leaves no pending requests', async () => {
  const f = fixture();
  const stopping = f.connection.stop();
  f.child.stdout.write(frame({ id: 1, result: null }));
  await stopping;
  assert.equal(f.connection.closed, true);
  assert.equal(f.connection.pending.size, 0);
  assert.equal(f.child.killed, true);
});

test('cancellation during registration never sends the cancelled request', async () => {
  const f = fixture();
  let written = '';
  f.child.stdin.on('data', bytes => { written += bytes.toString(); });
  const pending = f.connection.request('cancelled-method', {}, {
    onCancellationRequested(callback) { callback(); return { dispose() {} }; },
  });
  await assert.rejects(pending, /cancelled/);
  assert.equal(written.includes('cancelled-method'), false);
  assert.equal(f.connection.pending.size, 0);
  f.connection.fail(new Error('test cleanup'));
});

test('request deadline rejects and releases its bounded slot', async t => {
  t.mock.timers.enable({ apis: ['setTimeout'] });
  const f = fixture();
  const pending = f.connection.request('test', {});
  t.mock.timers.tick(35001);
  await assert.rejects(pending, /timed out/);
  assert.equal(f.connection.pending.size, 0);
  f.connection.fail(new Error('test cleanup'));
});
