import { spawn } from 'node:child_process';
import { createHash, randomUUID } from 'node:crypto';
import { lstat, mkdtemp, mkdir, open, readFile, realpath, rm, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { dirname, join, resolve, sep } from 'node:path';
import { fileURLToPath } from 'node:url';
import { downloadAndUnzipVSCode } from '@vscode/test-electron';

const root = resolve(fileURLToPath(new URL('..', import.meta.url)));
const extension = join(root, 'editors', 'vscode-zryna');
const runner = join(extension, 'test', 'host-runner');
const args = process.argv.slice(2);
// pnpm forwards the documented argument separator to the script.
if (args[0] === '--') args.shift();
const [setupArg, digest, ...rest] = args;
const prepareTrust = rest.includes('--prepare-trust');
const executables = rest.filter(item => item !== '--prepare-trust');
if (!setupArg || !/^[a-f0-9]{64}$/.test(digest ?? '') || executables.length > 1) {
  console.error('Usage: node scripts/run-editor-host-acceptance.mjs <verified-setup-directory> <setup.json-sha256> [VS-Code-executable] [--prepare-trust]');
  process.exit(2);
}

const setup = resolve(setupArg);
const state = join(tmpdir(), `zryna-vscode-host-state-${createHash('sha256').update(root).digest('hex').slice(0, 12)}`);
const temporary = await mkdtemp(join(tmpdir(), 'zryna-editor-host-'));
const workspace = join(state, 'workspace');
const userData = join(state, 'user-data');
const extensions = join(state, 'extensions');
const resultPath = join(temporary, 'result.json');
const configPath = join(temporary, 'config.json');
const lockPath = join(state, 'run.lock');
const lockOwner = JSON.stringify({ pid: process.pid, nonce: randomUUID() });
const started = Date.now();
let timer;
let child;
let childClose;
let childClosed = true;
let lockOwned = false;
async function removeWithin(parent, target, options) {
  if (!resolve(target).startsWith(`${resolve(parent)}${sep}`)) throw new Error('Cleanup path escaped its isolated directory.');
  const actualRoot = await realpath(parent);
  const actualParent = await realpath(dirname(target)).catch(error => {
    if (error.code === 'ENOENT') return null;
    throw error;
  });
  if (!actualParent) return;
  const canonical = path => process.platform === 'win32' ? path.toLowerCase() : path;
  if (canonical(actualParent) !== canonical(actualRoot)
    && !canonical(actualParent).startsWith(`${canonical(actualRoot)}${sep}`)) {
    throw new Error('Cleanup ancestor escaped its isolated directory.');
  }
  const stat = await lstat(target).catch(error => {
    if (error.code === 'ENOENT') return null;
    throw error;
  });
  if (stat?.isSymbolicLink() && options.recursive) throw new Error('Refusing recursive cleanup through a link.');
  if (stat) await rm(target, options);
}
async function terminate(childProcess) {
  if (process.platform === 'win32' && childProcess.pid && process.env.SystemRoot) {
    const killer = spawn(join(process.env.SystemRoot, 'System32', 'taskkill.exe'),
      ['/PID', String(childProcess.pid), '/T', '/F'], { stdio: 'ignore', windowsHide: true });
    let killerTimer;
    const stopped = await Promise.race([
      new Promise(fulfill => {
        killer.once('close', () => fulfill(true));
        killer.once('error', () => fulfill(false));
      }),
      new Promise(fulfill => { killerTimer = setTimeout(() => fulfill(false), 5000); }),
    ]);
    clearTimeout(killerTimer);
    if (!stopped) { killer.kill(); childProcess.kill(); }
  } else childProcess.kill();
}
try {
  await mkdir(state, { recursive: true });
  if ((await lstat(state)).isSymbolicLink()) throw new Error('Isolated state directory is a link.');
  try {
    const lock = await open(lockPath, 'wx');
    lockOwned = true;
    try { await lock.writeFile(lockOwner); } finally { await lock.close(); }
  } catch (error) {
    if (error.code === 'EEXIST') throw new Error(`Acceptance state is already in use: ${lockPath}. Wait for the active run; if this lock is stale, verify its process has stopped before removing it.`);
    throw error;
  }
  await mkdir(workspace, { recursive: true });
  await removeWithin(state, join(workspace, 'main.zry'), { force: true });
  const actual = createHash('sha256').update(await readFile(join(setup, 'setup.json'))).digest('hex');
  if (actual !== digest) throw new Error('Setup digest differs from the supplied reviewed digest.');
  const code = executables.length ? resolve(executables[0]) : await downloadAndUnzipVSCode({
    version: '1.138.0', cachePath: join(tmpdir(), 'zryna-vscode-test-cache'),
  });
  const downloadMs = Date.now() - started;
  await writeFile(configPath, JSON.stringify({ setup, digest, resultPath, prepareTrust }));
  console.log(`Isolated test workspace: ${workspace}`);
  const args = [
    '--new-window', '--skip-welcome', '--skip-release-notes', '--disable-updates',
    `--user-data-dir=${userData}`,
    `--extensions-dir=${extensions}`,
    `--extensionDevelopmentPath=${extension}`,
    `--extensionDevelopmentPath=${runner}`,
    workspace,
  ];
  child = spawn(code, args, {
    env: { ...process.env, ZRYNA_EDITOR_HOST_CONFIG: configPath },
    stdio: ['ignore', 'pipe', 'pipe'], windowsHide: false,
  });
  childClosed = false;
  childClose = new Promise((fulfill, reject) => {
    child.once('error', reject);
    child.once('close', (status, signal) => { childClosed = true; fulfill({ status, signal }); });
  });
  let output = '';
  for (const stream of [child.stdout, child.stderr]) stream.on('data', bytes => {
    output += bytes.toString();
    if (output.length > 100000) output = output.slice(-100000);
  });
  const timeout = new Promise((_, reject) => {
    timer = setTimeout(() => reject(new Error('VS Code extension-host acceptance timed out after 6 minutes.')), 360000);
  });
  const ended = await Promise.race([childClose, timeout]);
  const result = await readFile(resultPath, 'utf8').then(JSON.parse, () => null);
  console.log(JSON.stringify({ durationMs: Date.now() - started, setupAndDownloadMs: downloadMs,
    editorExit: ended, result, ...(ended.status !== 0 || !result?.passed ? { output: output.slice(-12000) } : {}) }, null, 2));
  if (ended.status !== 0 || !result?.passed) process.exitCode = 1;
} catch (error) {
  console.error(error);
  process.exitCode = 1;
} finally {
  clearTimeout(timer);
  if (!childClosed && child) {
    try { await terminate(child); } catch (error) { console.error(`Could not terminate VS Code: ${error}`); }
    let closeTimer;
    await Promise.race([childClose.catch(() => {}),
      new Promise(fulfill => { closeTimer = setTimeout(fulfill, 15000); })]);
    clearTimeout(closeTimer);
  }
  if (!childClosed) {
    console.error(`VS Code exit was not confirmed. Retained isolated state, result files, and lock: ${state}; ${temporary}`);
    process.exitCode = 1;
  } else {
    await removeWithin(tmpdir(), temporary, { recursive: true, force: true });
    if (lockOwned) {
      await removeWithin(state, join(workspace, 'main.zry'), { force: true });
      await removeWithin(state, join(userData, 'User', 'globalStorage', 'zryna.zryna', 'runs'),
        { recursive: true, force: true });
      if (await readFile(lockPath, 'utf8') !== lockOwner) throw new Error('Acceptance lock owner changed; lock retained.');
      await removeWithin(state, lockPath, { force: true });
    }
  }
}
