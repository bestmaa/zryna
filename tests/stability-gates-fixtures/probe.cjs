// Read-only lifetime diagnostics for the synthetic collector fixture. No probe
// command/version is substituted, and this is never candidate evidence.
const cp = require('node:child_process');
const fs = require('node:fs');
const { syncBuiltinESMExports } = require('node:module');
const path = process.env.ZRYNA_STABILITY_FIXTURE_TRACE;
function record(value) {
  if (!path || fs.existsSync(path) && fs.statSync(path).size > 64 * 1024) return;
  fs.appendFileSync(path, JSON.stringify({ pid: process.pid, at: Date.now(), ...value }) + '\n');
}
const sync = cp.spawnSync;
cp.spawnSync = function(command, args, options) {
  record({ event: 'spawnSync-start', command, args, cwd: options?.cwd });
  const result = sync.apply(this, arguments);
  record({ event: 'spawnSync-end', command, child: result.pid, status: result.status,
    error: result.error?.code, stderr: String(result.stderr ?? '').slice(0, 1024) });
  return result;
};
const async = cp.spawn;
cp.spawn = function(command, args, options) {
  const child = async.apply(this, arguments);
  record({ event: 'spawn', command, args, cwd: options?.cwd, child: child.pid });
  child.once('close', (code, signal) => record({ event: 'close', child: child.pid, code, signal }));
  return child;
};
record({ event: 'collector-start', cwd: process.cwd() });
syncBuiltinESMExports();
