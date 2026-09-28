import { spawnSync } from 'node:child_process';
import { resolve, join } from 'node:path';
import { mkdirSync } from 'node:fs';

const args = process.argv.slice(2);
if (args.length !== 4 || args[0] !== '--source' || args[2] !== '--output') {
  throw new Error('Expected --source <checkout> --output <new build directory>.');
}
const source = resolve(args[1]);
const output = resolve(args[3]);
const git = spawnSync('git', ['rev-parse', 'HEAD'], { cwd: source, encoding: 'utf8', shell: false });
if (git.status !== 0 || !/^[a-f0-9]{40}\s*$/.test(git.stdout)) throw new Error('Exact Git source identity required.');
if (!['win32', 'linux'].includes(process.platform) || process.arch !== 'x64') throw new Error('Unsupported build host.');
mkdirSync(output, { recursive: false });
const target = process.platform === 'win32' ? 'x86_64-pc-windows-msvc' : 'x86_64-unknown-linux-gnu';
const flags = [`--remap-path-prefix=${source}=/zryna/source`, `--remap-path-prefix=${output}=/zryna/build`,
  '-Cdebuginfo=0', '-Cstrip=symbols', '-Ccodegen-units=1',
  process.platform === 'win32' ? '-Clink-arg=/Brepro' : '-Clink-arg=-Wl,--build-id=none'];
const env = { ...process.env, ZRYNA_TOOLING_SOURCE_COMMIT: git.stdout.trim(),
  CARGO_ENCODED_RUSTFLAGS: flags.join('\x1f'), CARGO_TARGET_DIR: join(output, 'target'),
  CARGO_INCREMENTAL: '0', CARGO_BUILD_JOBS: '2' };
delete env.RUSTFLAGS;
const result = spawnSync('cargo', ['build', '--locked', '--release', '--target', target,
  '-p', 'zryna-language-server'], { cwd: source, env, shell: false, windowsHide: true, stdio: 'inherit' });
if (result.error || result.status !== 0) throw new Error('Candidate server build failed.');
console.log(join(output, 'target', target, 'release', `zryna-language-server${process.platform === 'win32' ? '.exe' : ''}`));
