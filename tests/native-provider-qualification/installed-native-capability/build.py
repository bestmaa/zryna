"""Build original source-bound private probe images; this is not a release producer."""
import datetime
import hashlib
import json
import os
import pathlib
import stat
import subprocess
import tomllib

from independent_cases import COMPILE_BINDINGS, MARKER_PREFIX, TARGETS, canonical_bytes, descriptor

if not __debug__:
    raise RuntimeError('private proof requires Python assertions; optimization is unsupported')


def sha(raw):
    return hashlib.sha256(raw).hexdigest()


def write(path, raw, mode=0o600):
    """Set only the initial creation mode; never chmod or alter the process umask."""
    path.parent.mkdir(parents=True, exist_ok=True, mode=0o700)
    fd = os.open(path, os.O_WRONLY | os.O_CREAT | os.O_EXCL, mode)
    with os.fdopen(fd, 'wb') as stream:
        stream.write(raw)
    if os.name != 'nt':
        assert stat.S_IMODE(path.lstat().st_mode) == mode, str(path)


def git(root, *args):
    return subprocess.check_output(['git', '-C', str(root), *args]).decode().strip()


def snapshot(root):
    assert git(root, 'status', '--porcelain') == '', 'a clean committed source is required'
    files = {}
    rows = subprocess.check_output(['git', '-C', str(root), 'ls-tree', '-rz', 'HEAD']).split(b'\0')
    for row in rows:
        if not row:
            continue
        meta, name = row.split(b'\t', 1)
        mode, kind, blob = meta.decode().split()
        path = root / name.decode()
        info = path.lstat()
        assert kind == 'blob' and stat.S_ISREG(info.st_mode)
        raw = path.read_bytes()
        assert hashlib.sha1(b'blob ' + str(len(raw)).encode() + b'\0' + raw).hexdigest() == blob
        assert mode in ('100644', '100755')
        if os.name != 'nt':
            assert bool(info.st_mode & 0o111) == (mode == '100755')
        files[name.decode()] = {'bytes': len(raw), 'sha256': sha(raw), 'git_mode': mode, 'blob': blob}
    return {'head': git(root, 'rev-parse', 'HEAD'), 'tree': git(root, 'rev-parse', 'HEAD^{tree}'), 'files': files}


def command(output, label, argv, cwd, env, source, timeout=1800):
    before = snapshot(source)
    start = datetime.datetime.now(datetime.timezone.utc).isoformat()
    with (output / (label + '.stdout')).open('xb') as stdout, (output / (label + '.stderr')).open('xb') as stderr:
        child = subprocess.Popen(argv, cwd=cwd, env=env, stdin=subprocess.DEVNULL,
                                 stdout=stdout, stderr=stderr)
        try:
            result = child.wait(timeout=timeout)
        except subprocess.TimeoutExpired:
            child.kill()
            child.wait()
            raise
    after = snapshot(source)
    record = {'argv': argv, 'cwd': str(cwd), 'selected_environment': env,
              'started_at': start, 'completed_at': datetime.datetime.now(datetime.timezone.utc).isoformat(),
              'exit': result, 'direct_child_reaped': True, 'source_before': before,
              'source_after': after, 'source_unchanged': before == after,
              'stdout': {'bytes': (output / (label + '.stdout')).stat().st_size,
                         'sha256': sha((output / (label + '.stdout')).read_bytes())},
              'stderr': {'bytes': (output / (label + '.stderr')).stat().st_size,
                         'sha256': sha((output / (label + '.stderr')).read_bytes())}}
    write(output / (label + '-execution.json'), json.dumps(record, indent=2).encode() + b'\n')
    assert type(result) is int and result == 0 and before == after, label
    return record


def image(raw, binding, platform):
    if platform == 'linux':
        assert raw[:7] == bytes([127, 69, 76, 70, 2, 1, 1])
        assert int.from_bytes(raw[16:18], 'little') in (2, 3)
        assert int.from_bytes(raw[18:20], 'little') == 62
    else:
        assert platform == 'win32' and raw[:2] == b'MZ'
        pe = int.from_bytes(raw[60:64], 'little')
        assert 64 <= pe and pe + 26 <= len(raw) and raw[pe:pe + 4] == b'PE\0\0'
        assert int.from_bytes(raw[pe + 4:pe + 6], 'little') == 0x8664
        assert int.from_bytes(raw[pe + 24:pe + 26], 'little') == 0x20b
    marker = MARKER_PREFIX + binding['descriptor'].encode() + b'\0' + binding['head'].encode() + b'\0' + binding['tree'].encode() + b'\0'
    assert raw.count(marker) == 1, 'private purpose/source marker missing or ambiguous'
    return {'bytes': len(raw), 'sha256': sha(raw), 'purpose_marker_count': 1}


def build(source, output, target, cargo, platform, windows_profile=None):
    assert platform in TARGETS
    if platform == 'win32':
        from windows_build import environment, verify_profile
        assert os.name == 'nt' and windows_profile is not None
        verify_profile(windows_profile)
    before = snapshot(source)
    output.mkdir(mode=0o700)
    license_bytes = (source / 'LICENSE').read_bytes()
    version = tomllib.loads((source / 'Cargo.toml').read_text())['workspace']['package']['version']
    payload = canonical_bytes(descriptor(version, before['head'], before['tree'], platform, sha(license_bytes)))
    write(output / 'native-provider.json', payload)
    write(output / 'LICENSE', license_bytes)
    package = output / 'cargo-proof'
    package.mkdir(mode=0o700)
    module = source / 'tests/native-provider-qualification/installed-native-capability'
    for name in ['probe.rs', 'mutations.rs']:
        write(package / name, (module / name).read_bytes())
    manifest = ['[workspace]', '[package]', 'name = "native-installation-proof"', 'version = "0.0.0"', 'edition = "2024"',
                '[[bin]]', 'name = "native-installation-proof"', 'path = "probe.rs"', '[dependencies]',
                'serde_json = "1.0.143"', 'sha2 = "=0.10.9"']
    for name in ['zryna-driver', 'zryna-source', 'zryna-diagnostics']:
        path = json.dumps(str(source / 'crates' / name))
        features = ', features = ["native-provider-internal"]' if name == 'zryna-driver' else ''
        manifest.append(name + ' = { path = ' + path + features + ' }')
    write(package / 'Cargo.toml', ('\n'.join(manifest) + '\n').encode())
    write(package / 'Cargo.lock', (source / 'Cargo.lock').read_bytes())
    tool_root = pathlib.Path(cargo).parents[2]
    env = {'PATH': str(pathlib.Path(cargo).parent) + os.pathsep + '/usr/bin:/bin',
           'CARGO_HOME': str(tool_root / 'cargo'), 'RUSTUP_HOME': str(tool_root / 'rustup'),
           'RUSTUP_TOOLCHAIN': '1.97.1', 'CARGO_TARGET_DIR': str(target),
           'CARGO_PROFILE_DEV_DEBUG': '0', 'CARGO_PROFILE_DEV_CODEGEN_UNITS': '16',
           'CARGO_PROFILE_DEV_OPT_LEVEL': '1',
           'CARGO_INCREMENTAL': '0', 'CARGO_BUILD_JOBS': '2',
           'LANG': 'C', 'LC_ALL': 'C', 'HOME': str(output), 'TMPDIR': str(output)}
    if platform == 'win32':
        env = environment(windows_profile, output, target)
    # The caller supplies a pinned toolchain root; Windows keeps only documented OS variables.
    for key in ['SYSTEMROOT', 'SystemRoot', 'WINDIR', 'COMSPEC', 'PATHEXT', 'TEMP', 'TMP', 'INCLUDE', 'LIB', 'LIBPATH']:
        if platform == 'linux' and key in os.environ:
            env[key] = os.environ[key]
    command(output, 'pinned-cargo-version', [cargo, '-V'], package, env, source)
    assert (output / 'pinned-cargo-version.stdout').read_text().startswith('cargo 1.97.1 ')
    rustc = pathlib.Path(windows_profile['tools']['rustc']['path']) if platform == 'win32' else pathlib.Path(cargo).with_name('rustc')
    rustup = pathlib.Path(windows_profile['tools']['rustup']['path']) if platform == 'win32' else pathlib.Path(cargo).with_name('rustup')
    command(output, 'pinned-rustc-version', [str(rustc), '-vV'], package, env, source)
    version_text = (output / 'pinned-rustc-version.stdout').read_text()
    assert '\nrelease: 1.97.1\n' in version_text and '\nhost: ' + TARGETS[platform][0] + '\n' in version_text
    command(output, 'actual-rustc-path', [str(rustup), 'which', 'rustc'], package, env, source)
    actual_rustc = pathlib.Path((output / 'actual-rustc-path.stdout').read_text().strip())
    command(output, 'actual-cargo-path', [str(rustup), 'which', 'cargo'], package, env, source)
    actual_cargo = pathlib.Path((output / 'actual-cargo-path.stdout').read_text().strip())
    tools = {str(path): {'bytes': path.stat().st_size, 'sha256': sha(path.read_bytes())}
             for path in [pathlib.Path(cargo), rustc, rustup, actual_rustc, actual_cargo]}
    if platform == 'win32':
        assert pathlib.Path(cargo) == actual_cargo and rustc == actual_rustc
    write(output / 'pinned-tool-identities.json', json.dumps(tools, indent=2).encode() + b'\n')
    command(output, 'generate-proof-lock', [cargo, 'generate-lockfile', '--offline', '--manifest-path', str(package / 'Cargo.toml')], package, env, source)
    root_packages = tomllib.loads((source / 'Cargo.lock').read_text())['package']
    actual_packages = tomllib.loads((package / 'Cargo.lock').read_text())['package']
    key = lambda row: tuple(row.get(name) for name in ['name', 'version', 'source', 'checksum'])
    root_keys = {key(row) for row in root_packages}
    assert all(key(row) in root_keys for row in actual_packages if row['name'] != 'native-installation-proof'), 'proof dependency differs from committed lock'
    binding = {'descriptor': sha(payload), 'head': before['head'], 'tree': before['tree']}
    env.update(dict(zip(COMPILE_BINDINGS, [binding['descriptor'], binding['head'], binding['tree']])))
    argv = [cargo, 'build', '--locked', '--offline', '--manifest-path', str(package / 'Cargo.toml')]
    command(output, 'build-prepared-private-image', argv, package, env, source)
    built = target / 'debug' / pathlib.Path(TARGETS[platform][1]).name
    raw = built.read_bytes()
    prepared = image(raw, binding, platform)
    write(output / 'prepared-image', raw, 0o700)
    for key in COMPILE_BINDINGS:
        env.pop(key)
    command(output, 'build-unprepared-negative-image', argv, package, env, source)
    raw = built.read_bytes()
    assert raw.count(MARKER_PREFIX + b'\0' * (64 + 1 + 40 + 1 + 40 + 1)) == 1
    write(output / 'unprepared-image', raw, 0o700)
    result = {'status': 'passed-builds-only', 'source': before, 'binding': binding,
              'prepared_image': prepared, 'unprepared_image': {'bytes': len(raw), 'sha256': sha(raw)},
              'descriptor_bytes': len(payload), 'descriptor_sha256': sha(payload),
              'license_sha256': sha(license_bytes), 'platform': platform,
              'dependencies_match_committed_lock': True, 'compiler_or_public_default_acceptance': False}
    if platform == 'win32':
        verify_profile(windows_profile)
        result['windows_toolchain'] = windows_profile
    write(output / 'build-receipt.json', json.dumps(result, indent=2).encode() + b'\n')
    assert snapshot(source) == before
    return result
