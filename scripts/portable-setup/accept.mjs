import assert from 'node:assert/strict';
import { spawnSync, spawn } from 'node:child_process';
import { readFileSync, writeFileSync, mkdirSync, renameSync, copyFileSync, existsSync } from 'node:fs';
import { join, resolve } from 'node:path';
import { pathToFileURL } from 'node:url';
import { createRequire } from 'node:module';
const require = createRequire(import.meta.url);
const { verifyInstallation, hash } = require('../../editors/vscode-zryna/src/installation.cjs');
const { Connection } = require('../../editors/vscode-zryna/src/connection.cjs');
const { runProject } = require('../../editors/vscode-zryna/src/run-project.cjs');

const args = process.argv.slice(2);
if (args.length !== 6 || args[0] !== '--installation' || args[2] !== '--digest' || args[4] !== '--work') {
  throw new Error('Expected --installation <extracted candidate> --digest <reviewed manifest SHA256> --work <new directory>.');
}
const original = resolve(args[1]);
const digest = args[3];
const work = resolve(args[5]);
mkdirSync(work, { recursive: false });
verifyInstallation(original, digest);
const relocated = join(work, 'relocated setup');
renameSync(original, relocated);
const config = verifyInstallation(relocated, digest);
mkdirSync(join(work, 'empty-path'));
mkdirSync(join(work, 'temporary'));
const env = { PATH: join(work, 'empty-path'), HOME: work, USERPROFILE: work,
  TMPDIR: join(work, 'temporary'), TEMP: join(work, 'temporary'), TMP: join(work, 'temporary'),
  LANG: 'C', LC_ALL: 'C', ...(process.platform === 'win32' ? { SystemRoot: process.env.SystemRoot,
    WINDIR: process.env.SystemRoot, PATHEXT: '.EXE', COMSPEC: join(process.env.SystemRoot, 'System32/cmd.exe') } : {}) };
const command = (exe, argv, cwd = work) => {
  const result = spawnSync(exe, argv, { cwd, env, encoding: 'utf8', shell: false, windowsHide: true,
    timeout: 120000, maxBuffer: 1024 * 1024 });
  assert.equal(result.status, 0, result.stderr || result.stdout || String(result.error));
  return result.stdout.trim();
};
for (const tool of ['node', 'cargo', 'rustc', 'pnpm']) {
  const result = spawnSync(tool, ['--version'], { cwd: work, env, shell: false, windowsHide: true });
  assert.ok(result.error || result.status !== 0, `${tool} must not be found in acceptance PATH`);
}
assert.equal(command(config.compilerPath, ['--version']), 'zryna 0.2.3');
assert.equal(command(config.serverPath, ['--version']),
  `zryna-language-server 0.5.0 portable-setup-v1 ${config.manifest.sourceCommit}`);
command(config.compilerPath, ['new', 'hello']);
for (const target of ['javascript', 'webassembly']) {
  assert.equal(command(config.compilerPath, ['run', 'src/main.zry', '--project-root', 'hello',
    '--target', target, '--export', 'main', '--name', target]), `${target}: i32 42`);
}
const project = join(work, 'practice');
mkdirSync(project);
copyFileSync(join(relocated, 'examples/main.zry'), join(project, 'main.zry'));
copyFileSync(join(relocated, 'examples/control-flow.zry'), join(project, 'control-flow.zry'));
const m3Project = join(project, 'm3');
mkdirSync(m3Project);
copyFileSync(join(relocated, 'examples/m3/main.zry'), join(m3Project, 'main.zry'));
copyFileSync(join(relocated, 'examples/m3/math.zry'), join(m3Project, 'math.zry'));
const bytes = readFileSync(join(project, 'main.zry'));
const sourceHash = hash(bytes);
const m2Bytes = readFileSync(join(project, 'control-flow.zry'));
const m2SourceHash = hash(m2Bytes);
const m3Bytes = readFileSync(join(m3Project, 'main.zry'));
const m3SourceHash = hash(m3Bytes);
const m3ImportBytes = readFileSync(join(m3Project, 'math.zry'));
const m3ImportHash = hash(m3ImportBytes);
const reports = [];
const invocation = async (executable, argv, cwd) => ({ stdout: command(executable, argv, cwd), stderr: '' });
for (const [target, name, values, expected] of [['javascript', 'add', ['13', '-4'], 9],
  ['webassembly', 'double', ['13'], 26]]) {
  const result = await runProject({ compiler: config.compilerPath, storage: join(work, 'run-storage'),
    bytes, selection: { profile: 'i32-v1', target, name, args: values.map(value => ({ type: 'i32', value })),
      resultType: 'i32' }, report: line => reports.push(line) }, invocation);
  assert.ok(existsSync(result.file));
  assert.ok(reports.some(line => line.includes(`${target}: i32 ${expected}`)));
}
for (const [target, enabled, count, expected] of [['javascript', 'true', '5', 10],
  ['webassembly', 'false', '3', -6]]) {
  const result = await runProject({ compiler: config.compilerPath, storage: join(work, 'run-storage'),
    bytes: m2Bytes, selection: { profile: 'control-flow-v1', target, name: 'accumulate',
      args: [{ type: 'bool', value: enabled }, { type: 'i32', value: count }], resultType: 'i32' },
    report: line => reports.push(line) }, invocation);
  assert.ok(existsSync(result.file));
  assert.ok(reports.some(line => line.includes(`${target}: i32 ${expected}`)));
}
const notices = [];
const connection = new Connection(config, (method, params) => notices.push({ method, params }), () => {},
  (executable, argv, options) => spawn(executable, argv, { ...options, env }));
try {
  const initialize = await connection.request('initialize', {
    rootUri: pathToFileURL(project).href, capabilities: { general: { positionEncodings: ['utf-16'] } },
  });
  assert.equal(initialize.serverInfo.version, '0.5.0');
  assert.equal(initialize.capabilities.definitionProvider, true);
  assert.equal(initialize.capabilities.experimental.zrynaSourceCommit, config.manifest.sourceCommit);
  const uri = pathToFileURL(join(project, 'main.zry')).href;
  connection.notify('initialized', {});
  connection.notify('textDocument/didOpen', { textDocument: { uri, languageId: 'zryna', version: 1, text: bytes.toString() } });
  const edits = await connection.request('textDocument/formatting', {
    textDocument: { uri }, options: { tabSize: 2, insertSpaces: true },
  });
  assert.ok(edits.length > 0);
  const definition = await connection.request('textDocument/definition', {
    textDocument: { uri }, position: { line: 0, character: bytes.toString().indexOf('x+y') },
  });
  assert.equal(definition.uri, uri);
  connection.notify('textDocument/didChange', { textDocument: { uri, version: 2 },
    contentChanges: [{ text: 'export function bad(x:i32):i32{return missing;}\n' }] });
  try {
    const invalid = await connection.request('textDocument/formatting', {
      textDocument: { uri }, options: { tabSize: 2, insertSpaces: true },
    });
    assert.equal(invalid.length, 0);
  } catch (error) { assert.match(error.message, /ZRYNA|format|revision|rejected/i); }
  assert.ok(notices.some(value => value.method === 'textDocument/publishDiagnostics'
    && value.params.version === 2 && value.params.diagnostics.length > 0));
} finally { await connection.stop(); }
const m2Notices = [];
const m2Connection = new Connection(config, (method, params) => m2Notices.push({ method, params }), () => {},
  (executable, argv, options) => spawn(executable, argv, { ...options, env }));
try {
  const initialize = await m2Connection.request('initialize', {
    rootUri: pathToFileURL(project).href, capabilities: { general: { positionEncodings: ['utf-16'] } },
    initializationOptions: { zrynaProfile: 'control-flow-v1' },
  });
  assert.equal(initialize.serverInfo.version, '0.5.0');
  assert.equal(initialize.capabilities.definitionProvider, false);
  assert.equal(initialize.capabilities.experimental.zrynaAnalysisProfile, 'control-flow-v1');
  assert.equal(initialize.capabilities.experimental.zrynaFormattingProfile, 'control-flow-format-v1');
  assert.equal(initialize.capabilities.experimental.zrynaSourceCommit, config.manifest.sourceCommit);
  const uri = pathToFileURL(join(project, 'control-flow.zry')).href;
  m2Connection.notify('initialized', {});
  m2Connection.notify('textDocument/didOpen', { textDocument: { uri, languageId: 'zryna', version: 1,
    text: m2Bytes.toString() } });
  const edits = await m2Connection.request('textDocument/formatting', {
    textDocument: { uri }, options: { tabSize: 2, insertSpaces: true },
  });
  assert.ok(edits.length > 0);
  m2Connection.notify('textDocument/didChange', { textDocument: { uri, version: 2 },
    contentChanges: [{ text: 'export function bad(x:i32):i32{let y:i32=missing;return y;}\n' }] });
  try {
    const invalid = await m2Connection.request('textDocument/formatting', {
      textDocument: { uri }, options: { tabSize: 2, insertSpaces: true },
    });
    assert.equal(invalid.length, 0);
  } catch (error) { assert.match(error.message, /ZRYNA|format|revision|rejected/i); }
  assert.ok(m2Notices.some(value => value.method === 'textDocument/publishDiagnostics'
    && value.params.version === 2 && value.params.diagnostics.length > 0));
} finally { await m2Connection.stop(); }
const m3Connection = new Connection({ ...config, workspaceRoot: m3Project }, () => {}, () => {},
  (executable, argv, options) => spawn(executable, argv, { ...options, env }));
try {
  const initialize = await m3Connection.request('initialize', {
    rootUri: pathToFileURL(m3Project).href, capabilities: { general: { positionEncodings: ['utf-16'] } },
    initializationOptions: { zrynaProfile: 'data-ownership-v1' },
  });
  assert.equal(initialize.serverInfo.version, '0.5.0');
  assert.equal(initialize.capabilities.definitionProvider, false);
  assert.equal(initialize.capabilities.experimental.zrynaAnalysisProfile, 'data-ownership-v1');
  assert.equal(initialize.capabilities.experimental.zrynaFormattingProfile, 'data-ownership-format-v1');
  assert.equal(initialize.capabilities.experimental.zrynaSourceCommit, config.manifest.sourceCommit);
  const uri = pathToFileURL(join(m3Project, 'main.zry')).href;
  m3Connection.notify('initialized', {});
  m3Connection.notify('textDocument/didOpen', { textDocument: { uri, languageId: 'zryna', version: 1,
    text: m3Bytes.toString() } });
  const edits = await m3Connection.request('textDocument/formatting', {
    textDocument: { uri }, options: { tabSize: 2, insertSpaces: true },
  });
  assert.deepEqual(edits.map(edit => edit.newText).join(''),
    'import {\n  double\n}\nfrom "./math.zry";\nexport function score(value: i32): i32 {\n  return double(value);\n}\n');
  m3Connection.notify('textDocument/didChange', { textDocument: { uri, version: 2 },
    contentChanges: [{ text: edits.map(edit => edit.newText).join('') }] });
  assert.deepEqual(await m3Connection.request('textDocument/formatting', {
    textDocument: { uri }, options: { tabSize: 2, insertSpaces: true },
  }), []);
  writeFileSync(join(m3Project, 'math.zry'), 'export function double(value:i32):i32{return missing;}');
  try {
    const stale = await m3Connection.request('textDocument/formatting', {
      textDocument: { uri }, options: { tabSize: 2, insertSpaces: true },
    });
    assert.equal(stale.length, 0);
  } catch (error) { assert.match(error.message, /ZRYNA|format|revision|rejected/i); }
} finally {
  writeFileSync(join(m3Project, 'math.zry'), m3ImportBytes);
  await m3Connection.stop();
}
assert.equal(hash(readFileSync(join(project, 'main.zry'))), sourceHash);
assert.equal(hash(readFileSync(join(project, 'control-flow.zry'))), m2SourceHash);
assert.equal(hash(readFileSync(join(m3Project, 'main.zry'))), m3SourceHash);
assert.equal(hash(readFileSync(join(m3Project, 'math.zry'))), m3ImportHash);
assert.throws(() => verifyInstallation(relocated, '0'.repeat(64)));
for (const name of ['worker.mjs', 'worker-v3.mjs', 'limits-v3.mjs']) {
  const worker = join(relocated, 'compiler/lib/zryna/bootstrap', name);
  const savedWorker = readFileSync(worker);
  try {
    writeFileSync(worker, 'throw new Error("substituted worker");\n');
    assert.throws(() => verifyInstallation(relocated, digest));
    const rejected = spawnSync(config.serverPath, ['--installed-root', config.compilerRoot], {
      cwd: work, env, shell: false, windowsHide: true, encoding: 'utf8', timeout: 10000,
    });
    assert.notEqual(rejected.status, 0, name);
    assert.match(rejected.stderr, /ZRYNA-D3001/, name);
    assert.match(rejected.stderr, name === 'worker.mjs'
      ? /installed worker differs from this tooling build/
      : /protocol-v3 tooling worker differs from this tooling build/, name);
  } finally { writeFileSync(worker, savedWorker); }
}
for (const name of ['worker-v3.mjs', 'limits-v3.mjs']) {
  const worker = join(relocated, 'compiler/lib/zryna/bootstrap', name);
  const missing = join(work, name);
  renameSync(worker, missing);
  try {
    assert.throws(() => verifyInstallation(relocated, digest));
    const rejected = spawnSync(config.serverPath, ['--installed-root', config.compilerRoot], {
      cwd: work, env, shell: false, windowsHide: true, encoding: 'utf8', timeout: 10000,
    });
    assert.notEqual(rejected.status, 0, name);
    assert.match(rejected.stderr, /ZRYNA-D3001/, name);
  } finally { renameSync(missing, worker); }
}
verifyInstallation(relocated, digest);
const receipt = { format: 'zryna.portable-acceptance.v1', sourceCommit: config.manifest.sourceCommit,
  manifestSha256: digest, target: config.manifest.target, runtimePathIsolated: true,
  compilerStarter: { javascript: 42, webassembly: 42 },
  editorRun: { scalar: { javascript: 9, webassembly: 26 }, controlFlow: { javascript: 10, webassembly: -6 } },
  formatting: { scalar: true, controlFlow: true, dataOwnership: true },
  diagnostics: { scalar: true, controlFlow: true },
  scalarDefinition: true, sourcePreserved: { scalar: sourceHash, controlFlow: m2SourceHash,
    dataOwnership: m3SourceHash, savedImport: m3ImportHash }, relocated: true,
  mismatchRejected: true, substitutedWorkerRejected: true, missingWorkerRejected: true,
  environment: { platform: process.platform, hostRelease: require('node:os').release(),
    independentCleanMachine: false }, installation: relocated };
writeFileSync(join(work, 'acceptance.json'), `${JSON.stringify(receipt, null, 2)}\n`, { flag: 'wx' });
console.log(JSON.stringify(receipt, null, 2));
