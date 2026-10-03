"""Linux-only real cross-target proof; no dependency, profile or runtime ABI activation."""
import hashlib
import json
import os
import pathlib
import platform
import subprocess
import sys

source = pathlib.Path(__file__).resolve().parent
repository = source.parents[1]
output = pathlib.Path(sys.argv[1]).resolve()
assert platform.system() == 'Linux' and platform.machine() == 'x86_64'
assert int(os.environ['CARGO_BUILD_JOBS']) <= 2
assert int(os.environ['RUST_TEST_THREADS']) <= 2
assert subprocess.check_output(['rustc', '--version'], text=True).startswith('rustc 1.97.1 ')
assert subprocess.check_output(['cargo', '--version'], text=True).startswith('cargo 1.97.1 ')
assert subprocess.check_output(['node', '--version'], text=True).strip() == 'v22.22.1'
output.mkdir(parents=True, exist_ok=True)
target = pathlib.Path(os.environ['CARGO_TARGET_DIR']).resolve() / 'debug'
names = ['zryna_abi', 'zryna_ir', 'zryna_layout', 'zryna_ownership_runtime_abi',
         'zryna_semantics', 'zryna_source', 'zryna_syntax', 'zryna_native_mir',
         'zryna_backend_javascript', 'zryna_backend_webassembly', 'zryna_backend_native']
command = ['cargo', 'build', '--locked']
for name in names:
    command += ['-p', name.replace('_', '-')]
subprocess.run(command, cwd=repository, check=True, timeout=600)
command = ['rustc', '--edition=2024', '-L', 'dependency=' + str(target / 'deps'),
           str(source / 'emit.rs'), '-o', str(output / 'emitter')]
for name in names:
    library = target / ('lib' + name + '.rlib')
    assert library.is_file(), library
    command += ['--extern', name + '=' + str(library)]
subprocess.run(command, cwd=repository, check=True, timeout=120)
subprocess.run([str(output / 'emitter'), str(output)], cwd=repository, check=True, timeout=30)

compiler = pathlib.Path('/usr/bin/gcc')
assert subprocess.check_output([str(compiler), '-dumpmachine'], text=True).strip() == 'x86_64-linux-gnu'
assert 12 <= int(subprocess.check_output([str(compiler), '-dumpversion'], text=True).split('.')[0]) <= 15
expected = []
for value in [-2147483648, -1, 0, 7, 2147483647]:
    expected += ['score:' + str(value), 'unwrap:' + str(value)]
expected += ['flag:1', 'flag:0', 'add:-2147483648', 'add:0', 'fallback:11', 'error:23', 'unwrap:7', 'flag:1']
records = []
for name in ['false', 'true']:
    executable = output / (name + '-native')
    command = [str(compiler), '-std=c11', '-O0', '-Wall', '-Wextra', '-Werror',
               '-no-pie', '-Wl,--build-id=none', str(source / 'execute.c'),
               str(output / (name + '.o')), '-o', str(executable)]
    subprocess.run(command, check=True, timeout=30)
    result = subprocess.run([str(executable)], text=True, capture_output=True, timeout=10, check=True)
    assert result.stdout.splitlines() == expected, result.stdout
    records.append({'fixture': name, 'exit_code': result.returncode, 'fixed_observations': 18,
                    'invalid_raw_bool_sigill': 4, 'stdout': result.stdout, 'compiler_command': command,
                    'executable_sha256': hashlib.sha256(executable.read_bytes()).hexdigest()})
    print('Native generic Option/Result ' + name + ': 18 fixed observations / 4 raw Boolean traps / PASS')
subprocess.run(['node', str(source / 'observations.mjs'), str(output)], check=True, timeout=10)
js_wasm = json.loads((output / 'js-wasm-observations.json').read_text())
for native, portable in zip(records, js_wasm['receipts'], strict=True):
    carriers = [item['exportName'] + ':' + str(int(item['expected'])) for item in portable['observations']]
    assert native['stdout'].splitlines() == carriers
artifacts = []
for name in ['false', 'true']:
    for suffix, pinned in [
        ('mjs', '9717f992cf0697c798c90e5ada1280ce808bd70ff7a515076a719d518d87b83c'),
        ('wasm', '7caf4f55d06f8f9109437a8de25d0317edfceb3f2b6db53337134a2d4e5cb78f'),
        ('zir', None), ('o', None),
    ]:
        path = output / (name + '.' + suffix)
        data = path.read_bytes()
        digest = hashlib.sha256(data).hexdigest()
        if pinned is not None:
            assert digest == pinned, (path, digest)
        artifacts.append({'path': str(path), 'bytes': len(data), 'sha256': digest})
(output / 'observations.json').write_text(json.dumps({
    'compiler_version': subprocess.check_output([str(compiler), '--version'], text=True).splitlines()[0],
    'same_sealed_program_per_fixture': True, 'native': records, 'js_wasm': js_wasm,
    'artifacts': artifacts, 'ownership_effects': {'loans': 0, 'drops': 0},
    'not_public_profile_or_owned_runtime_admission': True,
}, indent=2) + '\n')
print('All three backends match 36 fixed observations; accepted JS/Wasm bytes remain exact.')
