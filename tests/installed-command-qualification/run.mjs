// Actual fresh-install H1 invocation from an unrelated directory, with no checkout/tool lookup.
import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { existsSync, mkdirSync, readFileSync, renameSync, writeFileSync, unlinkSync, realpathSync } from 'node:fs';
import { dirname, isAbsolute, join, relative, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { prepare } from './prepare.mjs';
import { sha256 } from '../../scripts/distribution/canonical.mjs';

const source = resolve(dirname(fileURLToPath(import.meta.url)), '../..');
const output = resolve(process.argv[2] || '');
const outside = relative(source, output);
if (process.argv.length !== 3 || !outside
    || (!isAbsolute(outside) && outside.split(/[\\/]/)[0] !== '..')) throw new Error('External output required');
mkdirSync(output, { recursive: false });
const { installation, paths, proof, assertCurrent } = await prepare(source, output);
const relocated = join(output, 'relocated-install');
renameSync(installation, relocated);
const project = join(output, 'ordinary-user-project');
const cwd = join(output, 'unrelated-cwd');
const emptyPath = join(output, 'empty-path');
for (const path of [project, cwd, emptyPath]) mkdirSync(path);
for (const name of ['pure', 'error', 'lookup']) {
  writeFileSync(join(project, `${name}.zry`), readFileSync(join(source, `examples/wasi-command/${name}.zry`)));
}
writeFileSync(join(project, 'dependency.zry'), 'import { main } from "./pure.zry"; export function other(): bool { return main(); }');
const executable = join(relocated, paths.cli);
const rows = [];
const environment = { PATH: emptyPath, MODE: 'ambient-value-never-admitted' };
for (const key of ['SystemRoot', 'WINDIR', 'TEMP', 'TMP', 'TMPDIR']) {
  if (process.env[key]) environment[key] = process.env[key];
}
const common = ['--target', 'wasi-command', '--profile', 'command-h1-v1', '--export', 'main', '--json'];
const manifestName = 'zryna-wasi-command-manifest-v1.json';
function invoke(name, entry, extras = [], expected = undefined, projectRoot = project, verb = 'run', directory = cwd) {
  assertCurrent();
  const args = [verb, entry, ...common, '--name', name,
    ...(projectRoot ? ['--project-root', projectRoot] : []), ...extras];
  const result = spawnSync(executable, args, { cwd: directory, env: environment, shell: false,
    windowsHide: true, encoding: 'utf8', timeout: 90_000, maxBuffer: 1024 * 1024 });
  assert.ifError(result.error);
  const bundle = join(projectRoot || directory, '.zryna/out', `${name}.wasi-command-run`);
  const row = { name, arguments: args, exitCode: result.status, bundleExists: existsSync(bundle),
    stdout: result.stdout, stderr: result.stderr };
  rows.push(row);
  writeFileSync(join(output, 'invocations.json'), `${JSON.stringify(rows, null, 2)}\n`);
  if (expected) {
    assert.equal(result.status, expected.runReturn === 'ok' ? 0 : 5, result.stdout + result.stderr);
    const response = JSON.parse(result.stdout);
    assert.equal(response.profile, 'command-h1-v1');
    assert.equal(response.execution.outcome.runReturn, expected.runReturn);
    assert.equal(response.execution.teardown, 'confirmed');
    const data = readFileSync(join(bundle, manifestName));
    const manifest = JSON.parse(data);
    assert.equal(manifest.schema, 'zryna.wasi-command-manifest.v1');
    assert.equal(manifest.execution.runReturn, expected.runReturn);
    assert.equal(manifest.input.kind, expected.input);
    assert.equal(manifest.teardown, 'confirmed');
    assert.equal(manifest.grants.effective.length, expected.input === 'none' ? 0 : 1);
    const component = readFileSync(join(bundle, manifest.component.path));
    assert.equal(sha256(component), manifest.component.sha256);
    for (const privateText of ['test-only-secret-value', 'ambient-value-never-admitted']) {
      assert(!data.includes(Buffer.from(privateText)), 'private/ambient value must not enter manifest');
      assert(!result.stdout.includes(privateText), 'private/ambient value must not enter response');
    }
    row.manifestSha256 = sha256(data);
    row.componentSha256 = sha256(component);
  } else {
    assert.notEqual(result.status, 0, 'negative must reject');
    assert(!existsSync(bundle), 'negative must publish no bundle');
  }
  return row;
}
function privateFile(name, document) {
  const path = join(output, `${name}.json`);
  const text = typeof document === 'string' ? document : JSON.stringify(document);
  if (process.platform !== 'win32') writeFileSync(path, text, { flag: 'wx', mode: 0o600 });
  else {
    const script = `$ErrorActionPreference = 'Stop'
$path = $env:ZRYNA_PRIVATE_PATH
$bytes = [Text.Encoding]::UTF8.GetBytes($env:ZRYNA_PRIVATE_TEXT)
$user = [System.Security.Principal.WindowsIdentity]::GetCurrent().User
$acl = [System.Security.AccessControl.FileSecurity]::new()
$acl.SetOwner($user)
$acl.SetAccessRuleProtection($true, $false)
foreach ($sid in @($user, [System.Security.Principal.SecurityIdentifier]::new('S-1-5-18'))) {
 $acl.AddAccessRule([System.Security.AccessControl.FileSystemAccessRule]::new($sid,'FullControl','Allow'))
}
$file = [IO.FileStream]::new($path, [IO.FileMode]::CreateNew, [Security.AccessControl.FileSystemRights]::Write, [IO.FileShare]::None, 4096, [IO.FileOptions]::None, $acl)
try { $file.Write($bytes, 0, $bytes.Length) } finally { $file.Dispose() }`;
    const result = spawnSync('powershell.exe', ['-NoLogo', '-NoProfile', '-NonInteractive', '-Command', script],
      { env: { ...process.env, ZRYNA_PRIVATE_PATH: path, ZRYNA_PRIVATE_TEXT: text }, encoding: 'utf8' });
    assert.equal(result.status, 0, result.stderr);
  }
  return ['--grant-file', path];
}
function request(value, key = 'MODE', capability = 'environment') {
  return { schema: 'zryna.wasi-command-request.v1', world: 'zryna:capability-profiles/command@0.1.0',
    grant: { capability, key }, input: value === undefined ? { present: false } : { present: true, value } };
}
invoke('pure', 'pure.zry', [], { runReturn: 'ok', input: 'none' });
invoke('declared-error', 'error.zry', [], { runReturn: 'err', input: 'none' });
invoke('found', 'lookup.zry', privateFile('found-input', request('test-only-secret-value')), { runReturn: 'ok', input: 'present' });
invoke('missing', 'lookup.zry', privateFile('missing-input', request(undefined)), { runReturn: 'err', input: 'missing' });
invoke('empty', 'lookup.zry', privateFile('empty-input', request('')), { runReturn: 'ok', input: 'present' });
invoke('utf8', 'lookup.zry', privateFile('utf8-input', request('हिन्दी🙂')), { runReturn: 'ok', input: 'present' });
invoke('omitted', 'lookup.zry');
invoke('wrong-key', 'lookup.zry', privateFile('wrong-key-input', request('x', 'OTHER')));
invoke('unsupported', 'lookup.zry', privateFile('unsupported-input', request('x', 'MODE', 'network')));
invoke('malformed', 'lookup.zry', privateFile('malformed-input', '{'));
invoke('duplicate', 'lookup.zry', privateFile('duplicate-input', JSON.stringify(request('x')).replace('"present":true', '"present":true,"present":false')));
invoke('conflicting', 'lookup.zry', privateFile('conflicting-input', { ...request('x'), input: { present: false, value: 'x' } }));
invoke('extra-grant', 'pure.zry', privateFile('extra-input', request('x')));
invoke('relative-grant', 'lookup.zry', ['--grant-file', 'relative.json']);
invoke('node-override', 'pure.zry', ['--node', realpathSync(process.execPath)]);
invoke('root-override', 'pure.zry', ['--root', source]);
invoke('scalar-argument', 'pure.zry', ['--arg=i32:1']);
invoke('other-export', 'pure.zry', ['--export', 'other']);
invoke('dependency', 'dependency.zry');
invoke('build-only', 'pure.zry', [], undefined, project, 'build');
invoke('inside-installation', 'pure.zry', [], undefined, relocated);
invoke('contains-installation', 'pure.zry', [], undefined, output);
invoke('default-directory', 'pure.zry', [], { runReturn: 'ok', input: 'none' }, null, 'run', project);
const before = sha256(readFileSync(join(project, '.zryna/out/pure.wasi-command-run', manifestName)));
const repeated = spawnSync(executable, ['run', 'pure.zry', ...common, '--project-root', project, '--name', 'pure'],
  { cwd, env: environment, encoding: 'utf8', shell: false, timeout: 90_000 });
assert.ifError(repeated.error);
assert.notEqual(repeated.status, 0);
assert.equal(sha256(readFileSync(join(project, '.zryna/out/pure.wasi-command-run', manifestName))), before);
rows.push({ name: 'existing-output-preserved', exitCode: repeated.status, preservedManifestSha256: before });
for (const [name, path] of [['provider-tamper', 'lib/zryna/bootstrap/worker-v4.mjs'], ['runtime-tamper', paths.node], ['metadata-tamper', 'metadata/distribution.json']]) {
  const full = join(relocated, path);
  const original = readFileSync(full);
  try {
    writeFileSync(full, Buffer.concat([original, Buffer.from('\n')]));
    const row = invoke(name, 'pure.zry');
    assert(row.stdout.includes('ZRYNA-C4220'), 'installed admission must reject tampering');
  } finally { writeFileSync(full, original); }
}
const extra = join(relocated, 'unexpected.txt');
try { writeFileSync(extra, 'extra'); invoke('extra-installation-file', 'pure.zry'); }
finally { unlinkSync(extra); }
invoke('restored-installation', 'pure.zry', [], { runReturn: 'ok', input: 'none' });
writeFileSync(join(output, 'invocations.json'), `${JSON.stringify(rows, null, 2)}\n`);
assertCurrent();
const result = { ...proof, format: 'zryna.installed-command-qualification.v1',
  platform: process.platform, node: process.version, outsideCheckout: true,
  installation: relocated, project, cwd, invocationPath: executable,
  inheritedToolOverrides: false, pathContainsBuildTools: false,
  rows: rows.map(row => ({ name: row.name, exitCode: row.exitCode, manifestSha256: row.manifestSha256,
    componentSha256: row.componentSha256 })), passed: rows.length, failed: 0,
  linuxForeignOwnerProof: 'not-provided', localWindowsProof: 'not-provided' };
writeFileSync(join(output, 'qualification.json'), `${JSON.stringify(result, null, 2)}\n`, { flag: 'wx' });
console.log(JSON.stringify(result, null, 2));
