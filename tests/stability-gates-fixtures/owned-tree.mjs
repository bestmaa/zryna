import { spawn } from 'node:child_process';
import { writeFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';

const fixture = fileURLToPath(import.meta.url);
const [role, ready, output, implementation, detached = 'false'] = process.argv.slice(2);
if (role === 'grandchild') {
  process.on('SIGINT', () => {});
  process.on('SIGTERM', () => {});
  process.send({ pid: process.pid });
  setInterval(() => {}, 1000);
} else if (role === 'child') {
  const child = spawn(process.execPath, [fixture, 'grandchild'], {
    detached: detached === 'true' && process.platform !== 'win32',
    stdio: ['ignore', 'ignore', 'ignore', 'ipc'], windowsHide: true,
  });
  process.on('SIGINT', () => {});
  process.on('SIGTERM', () => {});
  child.on('message', message => {
    writeFileSync(ready, JSON.stringify({ child: process.pid, grandchild: message.pid }));
    console.log('owned tree ready');
  });
  setInterval(() => {}, 1000);
} else if (role === 'supervisor') {
  const { execute } = await import(implementation);
  const before = ['SIGINT', 'SIGTERM'].map(signal => process.listenerCount(signal));
  process.on('message', message => {
    for (const signal of message.signals ?? [message.signal]) {
      if (['SIGINT', 'SIGTERM'].includes(signal)) process.emit(signal);
    }
  });
  const result = await execute(process.execPath, [fixture, 'child', ready, output, implementation, detached],
    { root: process.cwd(), timeoutMs: 5000 });
  writeFileSync(output, JSON.stringify({ ...result,
    stdout: result.stdout.toString(), stderr: result.stderr.toString(), before,
    after: ['SIGINT', 'SIGTERM'].map(signal => process.listenerCount(signal)) }));
  if (process.connected) process.disconnect();
  process.exitCode = result.signal === 'SIGINT' ? 130 : result.signal === 'SIGTERM' ? 143 : 1;
} else if (role === 'collector') {
  const { collect } = await import(implementation);
  process.on('message', message => {
    if (['SIGINT', 'SIGTERM'].includes(message.signal)) process.emit(message.signal);
  });
  const result = await collect('compatibility', output);
  if (process.connected) process.disconnect();
  process.exitCode = result.signal === 'SIGINT' ? 130 : 143;
} else {
  throw new Error('owned test fixture role required');
}
