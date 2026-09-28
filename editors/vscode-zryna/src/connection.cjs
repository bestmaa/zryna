'use strict';

const { spawn } = require('node:child_process');
const { isAbsolute } = require('node:path');
const { TextDecoder } = require('node:util');

const MAX_FRAME = 16 * 1024 * 1024;

class Connection {
  constructor(config, onNotification, onFailure, launch = spawn) {
    if (!(config.installed ? [config.serverPath, config.compilerRoot]
      : [config.serverPath, config.compilerRoot, config.nodePath]).every(value =>
      typeof value === 'string' && isAbsolute(value) && !value.includes('\0'))) {
      throw new Error('Configure absolute Zryna server, compiler and Node paths in user settings.');
    }
    if (config.workspaceRoot !== undefined && (typeof config.workspaceRoot !== 'string'
      || !isAbsolute(config.workspaceRoot) || config.workspaceRoot.includes('\0'))) {
      throw new Error('Zryna requires an absolute trusted workspace root.');
    }
    this.pending = new Map();
    this.sequence = 0;
    this.buffer = Buffer.alloc(0);
    this.onNotification = onNotification;
    this.onFailure = onFailure;
    this.closed = false;
    const args = config.installed ? ['--installed-root', config.compilerRoot]
      : ['--compiler-root', config.compilerRoot, '--node', config.nodePath];
    if (config.workspaceRoot !== undefined) args.push('--workspace-root', config.workspaceRoot);
    this.child = launch(config.serverPath, args, {
      cwd: config.compilerRoot, shell: false, windowsHide: true, stdio: ['pipe', 'pipe', 'pipe'],
    });
    this.child.stdout.on('data', bytes => {
      try { this.receive(bytes); } catch { this.fail(new Error('Invalid language-server response.')); }
    });
    // Compiler stderr is drained without exposing source contents or command-like diagnostic text.
    this.child.stderr.on('data', () => {});
    this.child.on('error', () => this.fail(new Error('Could not start the configured Zryna server.')));
    this.child.on('exit', () => this.fail(new Error('The Zryna server stopped.')));
    this.child.stdin.on('error', () => this.fail(new Error('The Zryna server input closed.')));
  }

  send(value) {
    if (this.closed) throw new Error('The Zryna connection is closed.');
    const body = Buffer.from(JSON.stringify({ jsonrpc: '2.0', ...value }));
    if (body.length > MAX_FRAME) throw new Error('Zryna document exceeds the transport limit.');
    this.child.stdin.write(Buffer.concat([Buffer.from(`Content-Length: ${body.length}\r\n\r\n`), body]));
  }

  notify(method, params) { this.send({ method, params }); }

  request(method, params, token) {
    if (this.pending.size >= 32) return Promise.reject(new Error('Too many Zryna requests.'));
    if (token?.isCancellationRequested) return Promise.reject(new Error('Request cancelled.'));
    const id = ++this.sequence;
    return new Promise((resolve, reject) => {
      const finish = (error, value) => {
        const entry = this.pending.get(id);
        if (!entry) return;
        clearTimeout(entry.timer);
        entry.cancellation?.dispose();
        this.pending.delete(id);
        if (error) reject(error); else resolve(value);
      };
      const timer = setTimeout(() => {
        if (!this.closed) this.notify('$/cancelRequest', { id });
        finish(new Error('Zryna request timed out.'));
      }, 35000);
      this.pending.set(id, { timer, finish });
      const cancellation = token?.onCancellationRequested(() => {
        if (!this.closed) this.notify('$/cancelRequest', { id });
        finish(new Error('Request cancelled.'));
      });
      const entry = this.pending.get(id);
      if (entry) entry.cancellation = cancellation; else cancellation?.dispose();
      try { if (this.pending.has(id)) this.send({ id, method, params }); } catch (error) { finish(error); }
    });
  }

  receive(bytes) {
    if (this.closed) return;
    if (this.buffer.length + bytes.length > MAX_FRAME + 1024) throw new Error('Frame limit.');
    this.buffer = Buffer.concat([this.buffer, bytes]);
    for (;;) {
      const separator = this.buffer.indexOf('\r\n\r\n');
      if (separator < 0) {
        if (this.buffer.length > 1024) throw new Error('Header limit.');
        return;
      }
      if (separator > 1024) throw new Error('Header limit.');
      const match = /^Content-Length: ([0-9]+)$/i.exec(this.buffer.subarray(0, separator).toString('ascii'));
      const length = match && Number(match[1]);
      if (!Number.isSafeInteger(length) || length < 1 || length > MAX_FRAME) throw new Error('Length.');
      const end = separator + 4 + length;
      if (this.buffer.length < end) return;
      const message = JSON.parse(new TextDecoder('utf-8', { fatal: true }).decode(this.buffer.subarray(separator + 4, end)));
      this.buffer = this.buffer.subarray(end);
      if (message.jsonrpc !== '2.0') throw new Error('Protocol.');
      if (message.method) {
        if (message.id !== undefined) throw new Error('Server requests are unsupported.');
        this.onNotification(message.method, message.params);
      } else {
        const entry = this.pending.get(message.id);
        if (!entry) continue; // A cancelled request may already have reached the server.
        const error = message.error ? new Error(
          typeof message.error.data?.code === 'string' ? message.error.data.code : 'Zryna request rejected.') : null;
        entry.finish(error, message.result);
      }
    }
  }

  fail(error) {
    if (this.closed) return;
    this.closed = true;
    for (const entry of [...this.pending.values()]) entry.finish(error);
    this.child.kill();
    this.onFailure(error);
  }

  async stop() {
    if (this.closed) return;
    try {
      await this.request('shutdown', null);
      this.notify('exit', null);
    } finally {
      this.closed = true;
      for (const entry of [...this.pending.values()]) entry.finish(new Error('Connection closed.'));
      this.child.stdin.end();
      this.child.kill();
    }
  }
}

module.exports = { Connection };
