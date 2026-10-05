// Failure-only probe: retain Node stderr without changing transport expectations.
const { spawnSync } = require('node:child_process');
const [directory, entrypoint] = process.argv.slice(1);
const ordinary = path => path.startsWith('\\\\?\\UNC\\')
  ? '\\\\' + path.slice(8)
  : path.startsWith('\\\\?\\') ? path.slice(4) : path;
for (const compatible of [false, true]) {
  const root = compatible ? ordinary(directory) : directory;
  const script = compatible ? ordinary(entrypoint) : entrypoint;
  const result = spawnSync(process.execPath, [script], {
    cwd: root,
    env: { SystemRoot: process.env.SystemRoot, WINDIR: process.env.WINDIR },
    input: '{"id":1,"method":"handshake"}\n',
    encoding: 'utf8', timeout: 1500, maxBuffer: 16384,
  });
  console.log(JSON.stringify({ compatible, root, script, status: result.status,
    error: result.error?.message, stdout: result.stdout, stderr: result.stderr }));
}
