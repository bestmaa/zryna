"""Independent source, installation build, image and observed tool provenance."""
import datetime as dt
import hashlib
import json
import ntpath
import os
from pathlib import Path, PureWindowsPath as Win
import posixpath
import re
import stat
import subprocess
import sys
import tomllib
from independent_cases import COMPILE_BINDINGS, MARKER_PREFIX, TARGETS

if not __debug__:
    raise RuntimeError('admission requires unoptimized Python')
MAX = 128 * 1024 * 1024
BUILD_LABELS = ('pinned-cargo-version', 'pinned-rustc-version', 'actual-rustc-path',
                'actual-cargo-path', 'generate-proof-lock', 'build-prepared-private-image',
                'build-unprepared-negative-image')

def require(ok, message):
    if not ok:
        raise ValueError(message)


def digest(raw):
    return hashlib.sha256(raw).hexdigest()


def exact(value, expected):
    if type(value) is not type(expected):
        return False
    if type(expected) is dict:
        return value.keys() == expected.keys() and all(exact(value[k], expected[k]) for k in expected)
    if type(expected) is list:
        return len(value) == len(expected) and all(exact(v, e) for v, e in zip(value, expected))
    return value == expected


def stable(info):
    return tuple(getattr(info, name, None) for name in ('st_dev', 'st_ino', 'st_nlink', 'st_size', 'st_mtime_ns',
                                                      'st_ctime_ns', 'st_mode', 'st_file_attributes', 'st_reparse_tag'))


def package_update_only(before, after):
    """Compare complete pre-process/post-process journals across the probe's Update.

    Only same-byte lock replacement and direct parent timestamps are observed.
    This permits no mutation credit and is never used for pre-mutation setup.
    """
    if exact(before, after):
        return True
    if type(before) is not dict or type(after) is not dict:
        return False
    left = {k.replace('\\', '/'): v for k, v in before.items()}
    right = {k.replace('\\', '/'): v for k, v in after.items()}
    if len(left) != len(before) or len(right) != len(after) or left.keys() != right.keys():
        return False
    lock = 'sources/packages/app/zryna.lock.json'; parent = 'sources/packages/app'
    allowed = {lock: {'inode', 'mtime_ns', 'ctime_ns'}, parent: {'mtime_ns', 'ctime_ns'}}
    if left.get(lock, {}).get('kind') != 'file' or right.get(lock, {}).get('kind') != 'file':
        return False
    if left.get(parent, {}).get('kind') != 'directory' or right.get(parent, {}).get('kind') != 'directory':
        return False
    for path, row in left.items():
        other = right[path]
        if exact(row, other):
            continue
        if path not in allowed or type(row) is not dict or type(other) is not dict or row.keys() != other.keys():
            return False
        for key in row:
            if exact(row[key], other[key]):
                continue
            if key not in allowed[path] or not all(type(v) is int and abs(v) < 2**128 for v in (row[key], other[key])):
                return False
    return True


def read(path, limit=8 * 1024 * 1024):
    path = Path(path).absolute()
    for item in (path, *path.parents):
        info = item.lstat()
        require(not stat.S_ISLNK(info.st_mode) and not getattr(info, 'st_file_attributes', 0) & 0x400, 'linked evidence')
    before = path.lstat()
    require(stat.S_ISREG(before.st_mode) and before.st_nlink == 1 and before.st_size <= limit, 'file bound/type')
    with path.open('rb') as stream:
        opened = os.fstat(stream.fileno())
        require((opened.st_dev, opened.st_ino, opened.st_size) == (before.st_dev, before.st_ino, before.st_size), 'open identity')
        raw = stream.read(limit + 1)
        after_handle = os.fstat(stream.fileno())
    require(len(raw) <= limit and stable(opened) == stable(after_handle) and stable(before) == stable(path.lstat()), 'unstable evidence')
    return raw


def decode(raw):
    def pairs(rows):
        result = {}
        for key, value in rows:
            require(key not in result, 'duplicate JSON key')
            result[key] = value
        return result
    return json.loads(raw, object_pairs_hook=pairs, parse_constant=lambda _: require(False, 'nonfinite JSON'))


def document(path):
    return decode(read(path))


def file_binding(raw):
    return {'bytes': len(raw), 'sha256': digest(raw)}


def canonical(value):
    return (json.dumps(value, sort_keys=True, separators=(',', ':'), ensure_ascii=False, allow_nan=False) + '\n').encode()


def source_snapshot(root):
    def git(*args):
        return subprocess.check_output(['git', '-C', str(root), *args])
    require(not git('status', '--porcelain').strip(), 'source not clean')
    files = {}
    for row in git('ls-tree', '-rz', 'HEAD').split(b'\0'):
        if not row:
            continue
        meta, name = row.split(b'\t', 1)
        mode, kind, blob = meta.decode().split()
        raw = read(root / name.decode())
        require(kind == 'blob' and mode in ('100644', '100755'), 'source Git type')
        require(hashlib.sha1(b'blob ' + str(len(raw)).encode() + b'\0' + raw).hexdigest() == blob, 'source Git bytes')
        if os.name != 'nt':
            require(bool((root / name.decode()).lstat().st_mode & 0o111) == (mode == '100755'), 'source mode')
        files[name.decode()] = file_binding(raw) | {'git_mode': mode, 'blob': blob}
    return {'head': git('rev-parse', 'HEAD').decode().strip(), 'tree': git('rev-parse', 'HEAD^{tree}').decode().strip(), 'files': files}


def image_header(raw, platform):
    require(64 <= len(raw) <= MAX, 'image bound')
    if platform == 'linux':
        require(raw[:7] == b'\x7fELF\x02\x01\x01' and int.from_bytes(raw[16:18], 'little') in (2, 3)
                and int.from_bytes(raw[18:20], 'little') == 62, 'ELF identity')
    else:
        pe = int.from_bytes(raw[60:64], 'little')
        require(raw[:2] == b'MZ' and 64 <= pe <= len(raw) - 26 and raw[pe:pe+4] == b'PE\0\0'
                and raw[pe+4:pe+6] == b'\x64\x86' and raw[pe+24:pe+26] == b'\x0b\x02', 'PE identity')
    return file_binding(raw)


def image(raw, binding, platform):
    image_header(raw, platform)
    marker = MARKER_PREFIX + binding['descriptor'].encode() + b'\0' + binding['head'].encode() + b'\0' + binding['tree'].encode() + b'\0'
    require(raw.count(marker) == 1, 'compiled private marker')
    return file_binding(raw)


def captured(folder, label, source, previous=None, *, linker_help=False, dumpbin_help=False):
    require(type(linker_help) is bool and (not linker_help or label == 'observed-linker-version'), 'linker help scope')
    require(type(dumpbin_help) is bool and (not dumpbin_help or label == 'observed-dumpbin-version')
            and not (linker_help and dumpbin_help), 'dumpbin help scope')
    row = document(folder / (label + '-execution.json'))
    require(exact(row['source_before'], source) and exact(row['source_after'], source) and row['source_unchanged'] is True
            and type(row['exit']) is int and row['exit'] == (1100 if linker_help or dumpbin_help else 0)
            and row['direct_child_reaped'] is True, 'build execution')
    start = dt.datetime.fromisoformat(row['started_at']); end = dt.datetime.fromisoformat(row['completed_at'])
    require(start.tzinfo and end.tzinfo and start <= end and (previous is None or previous <= start), 'build order')
    for stream in ('stdout', 'stderr'):
        require(exact(file_binding(read(folder / (label + '.' + stream), 16 * 1024 * 1024)), row[stream]), 'build raw stream')
    return row, end


def observed_linker_help(p, row, stdout, stderr):
    """Independently recognize LINK help; producer exit declarations are unused."""
    require(type(row['exit']) is int and row['exit'] == 1100 and row['direct_child_reaped'] is True
            and row['source_unchanged'] is True and exact(row['source_before'], row['source_after']), 'linker help execution')
    require(row['argv'] == [p['tools']['linker']['path'], '/?']
            and exact(row['selected_environment'], p['host_environment'])
            and ntpath.basename(row['cwd']) == 'toolchain', 'linker help query')
    require(exact(file_binding(stdout), row['stdout']) and exact(file_binding(stderr), row['stderr'])
            and not stderr and 0 < len(stdout) <= 65536, 'linker help streams')
    lines = stdout.replace(b'\r\n', b'\n').split(b'\n')
    require(len(lines) > 7, 'linker help lines')
    match = re.fullmatch(rb'Microsoft \(R\) Incremental Linker Version (14\.\d+\.\d+\.\d+)', lines[0])
    require(match is not None and match.group(1).decode().split('.')[:2] == p['msvc']['version'].split('.')[:2], 'linker help version')
    require(lines[1:7] == [b'Copyright (C) Microsoft Corporation.  All rights reserved.', b'',
            b' usage: LINK [options] [files] [@commandfile]', b'', b'   options:', b''], 'linker help header')
    require(all(any(line.startswith(prefix) for line in lines[7:]) for prefix in
            (b'      /ALIGN:', b'      /OUT:', b'      /VERSION:')) and not any(line.startswith(b'LINK :') for line in lines), 'linker help options')


def observed_dumpbin_help(p, row, stdout, stderr):
    """Independently admit observed DUMPBIN help, without trusting exit declarations."""
    require(type(row['exit']) is int and row['exit'] == 1100 and row['direct_child_reaped'] is True
            and row['source_unchanged'] is True and exact(row['source_before'], row['source_after']), 'dumpbin help execution')
    require(row['argv'] == [p['tools']['dumpbin']['path'], '/?']
            and exact(row['selected_environment'], p['host_environment'])
            and ntpath.basename(row['cwd']) == 'toolchain', 'dumpbin help query')
    require(exact(file_binding(stdout), row['stdout']) and exact(file_binding(stderr), row['stderr'])
            and not stderr and 0 < len(stdout) <= 65536, 'dumpbin help streams')
    lines = stdout.replace(b'\r\n', b'\n').split(b'\n')
    require(len(lines) > 7, 'dumpbin help lines')
    match = re.fullmatch(rb'Microsoft \(R\) COFF/PE Dumper Version (14\.\d+\.\d+\.\d+)', lines[0])
    require(match is not None and match.group(1).decode().split('.')[:2] == p['msvc']['version'].split('.')[:2], 'dumpbin help version')
    require(lines[1:7] == [b'Copyright (C) Microsoft Corporation.  All rights reserved.', b'',
            b'usage: DUMPBIN [options] [files]', b'', b'   options:', b''], 'dumpbin help header')
    require(all(option in lines[7:] for option in (b'      /DEPENDENTS', b'      /HEADERS', b'      /SYMBOLS'))
            and not any(line.startswith((b'DUMPBIN :', b'LINK :')) for line in lines), 'dumpbin help options')


def windows_observations(proof, p, src, live):
    profile_contract(p)
    folder = proof / 'toolchain'
    require(document(folder / 'before.json') == document(folder / 'after.json') == p, 'profile before/after')
    for role, row in [('python', p['python']), *p['tools'].items(), *p['sdk_inputs'].items()]:
        raw = read(folder / row['raw_relative_path'], MAX)
        require(file_binding(raw) == {k: row[k] for k in ('bytes', 'sha256')}, 'retained tool bytes')
        if role not in p['sdk_inputs']:
            image_header(raw, 'win32')
        if live:
            require(sys.platform == 'win32' and file_binding(read(row['path'], MAX)) == file_binding(raw), 'live tool bytes')
    last = None
    for role in ('python', 'cargo', 'rustc', 'linker', 'dumpbin'):
        row, last = captured(folder, 'observed-' + role + '-version', src, last,
                             linker_help=role == 'linker', dumpbin_help=role == 'dumpbin')
        tool = p['python'] if role == 'python' else p['tools'][role]
        require(row['argv'] == [tool['path'], {'python': '--version', 'cargo': '-V', 'rustc': '-vV', 'linker': '/?', 'dumpbin': '/?'}[role]]
                and row['selected_environment'] == p['host_environment'] and ntpath.basename(row['cwd']) == 'toolchain', 'observed tool query')
        if role == 'linker':
            observed_linker_help(p, row, read(folder / 'observed-linker-version.stdout'),
                                 read(folder / 'observed-linker-version.stderr'))
        if role == 'dumpbin':
            observed_dumpbin_help(p, row, read(folder / 'observed-dumpbin-version.stdout'),
                                  read(folder / 'observed-dumpbin-version.stderr'))
    require(read(folder / 'observed-python-version.stdout').decode().strip() == 'Python 3.12.10', 'actual Python version')
    require(read(folder / 'observed-cargo-version.stdout').decode().startswith('cargo 1.97.1 '), 'actual Cargo version')
    rust = read(folder / 'observed-rustc-version.stdout').decode().replace('\r\n', '\n')
    require('\nrelease: 1.97.1\n' in rust and '\nhost: x86_64-pc-windows-msvc\n' in rust
            and '\ncommit-hash: 8bab26f4f68e0e26f0bb7960be334d5b520ea452\n' in rust, 'actual Rust version')
    return last


def profile_contract(p):
    require(set(p) == {'schema_version', 'platform', 'target', 'python', 'tools', 'rust_version', 'cargo_version',
            'msvc', 'sdk_inputs', 'host_environment', 'observation_scope', 'cargo_home', 'rustup_home'}, 'profile fields')
    require(type(p['schema_version']) is int and p['schema_version'] == 1 and p['platform'] == 'win32'
            and p['target'] == TARGETS['win32'][0] and p['python']['version'] == '3.12.10'
            and p['rust_version'] == p['cargo_version'] == '1.97.1', 'profile pins')
    require(set(p['tools']) == {'cargo', 'rustc', 'rustup', 'linker', 'dumpbin'}
            and set(p['sdk_inputs']) == {'kernel32.lib', 'ucrt.lib', 'vcruntime.lib', 'libcmt.lib'}, 'profile roles')
    for role, row in [('python', p['python']), *p['tools'].items(), *p['sdk_inputs'].items()]:
        require(set(row) == {'path', 'bytes', 'sha256', 'raw_relative_path'} | ({'version'} if role == 'python' else set()), 'tool fields')
        require(Win(row['path']).is_absolute() and '..' not in Win(row['path']).parts and type(row['bytes']) is int
                and 0 < row['bytes'] <= MAX and re.fullmatch('[a-f0-9]{64}', row['sha256']), 'tool identity')
        require(row['raw_relative_path'] == 'tool-originals/' + role.replace('.', '-')
                + ('.bin' if role in p['sdk_inputs'] else '.exe'), 'tool original path')
    m = p['msvc']; vc = Win(m['root']); sdk = Win(m['sdk_root']); native = vc / 'bin/Hostx64/x64'
    require(set(m) == {'root', 'version', 'sdk_root', 'sdk_version'} and re.fullmatch(r'14\.\d+\.\d+', m['version'])
            and re.fullmatch(r'10\.0\.\d+\.0', m['sdk_version']), 'SDK version')
    require(re.fullmatch(r'[A-Za-z]:\\Program Files\\Microsoft Visual Studio\\2022\\Enterprise\\VC\\Tools\\MSVC\\'
            + re.escape(m['version']), str(vc), re.I) and sdk == Win(r'C:\Program Files (x86)\Windows Kits\10'), 'SDK roots')
    require(Win(p['tools']['linker']['path']) == native / 'link.exe'
            and Win(p['tools']['dumpbin']['path']) == native / 'dumpbin.exe', 'MSVC tools')
    libs = {'kernel32.lib': sdk / 'Lib' / m['sdk_version'] / 'um/x64/kernel32.lib',
            'ucrt.lib': sdk / 'Lib' / m['sdk_version'] / 'ucrt/x64/ucrt.lib',
            'vcruntime.lib': vc / 'lib/x64/vcruntime.lib', 'libcmt.lib': vc / 'lib/x64/libcmt.lib'}
    require(all(Win(p['sdk_inputs'][k]['path']) == v for k, v in libs.items()), 'selected SDK paths')
    env = p['host_environment']; lib = [vc / 'lib/x64', sdk / 'Lib' / m['sdk_version'] / 'um/x64', sdk / 'Lib' / m['sdk_version'] / 'ucrt/x64']
    paths = [Win(p['tools']['cargo']['path']).parent, native, Win(r'C:\Windows\System32'), Win(r'C:\Windows')]
    inc = [vc / 'include', *[sdk / 'Include' / m['sdk_version'] / k for k in ('ucrt', 'um', 'shared')]]
    require(set(env) == {'SystemRoot', 'PATH', 'LIB', 'INCLUDE', 'LIBPATH'} and Win(env['SystemRoot']) == Win(r'C:\Windows'), 'host fields')
    for key, expected in [('PATH', paths), ('LIB', lib), ('LIBPATH', lib), ('INCLUDE', inc)]:
        require(type(env[key]) is str and len(env[key]) <= 8192 and [Win(x) for x in env[key].split(';')] == expected, 'host search path')
    require(all(Win(p[k]).is_absolute() and '..' not in Win(p[k]).parts for k in ('cargo_home', 'rustup_home')), 'tool homes')
    require(p['observation_scope'] == 'per-run observed exact tools and selected core SDK inputs; no upstream supplier byte authority', 'supplier scope')


def windows_environment(profile, output, target):
    retained, build_target = Win(output), Win(target)
    require(retained.is_absolute() and build_target.is_absolute()
            and '..' not in retained.parts and '..' not in build_target.parts
            and not retained.is_relative_to(build_target) and not build_target.is_relative_to(retained),
            'disjoint absolute Windows proof and target roots')
    state = build_target / 'private-process-state'
    return {**profile['host_environment'], 'CARGO_HOME': profile['cargo_home'], 'RUSTUP_HOME': profile['rustup_home'],
            'RUSTUP_TOOLCHAIN': '1.97.1', 'RUSTC': profile['tools']['rustc']['path'],
            'CARGO_TARGET_X86_64_PC_WINDOWS_MSVC_LINKER': profile['tools']['linker']['path'],
            'CARGO_TARGET_DIR': target, 'CARGO_PROFILE_DEV_DEBUG': '0', 'CARGO_PROFILE_DEV_CODEGEN_UNITS': '16',
            'CARGO_PROFILE_DEV_OPT_LEVEL': '1', 'CARGO_INCREMENTAL': '0', 'CARGO_BUILD_JOBS': '2',
            'TEMP': str(state / 'Temp'), 'TMP': str(state / 'Temp'),
            'USERPROFILE': str(state), 'HOME': str(state),
            'APPDATA': str(state / 'AppData/Roaming'), 'LOCALAPPDATA': str(state / 'AppData/Local'),
            'LANG': 'C', 'LC_ALL': 'C'}


def verify_build(root, proof, src, platform, live):
    build = proof / 'build'; receipt = document(build / 'build-receipt.json')
    require(exact(receipt['source'], src) and receipt['platform'] == platform, 'build source')
    version = tomllib.loads(read(root / 'Cargo.toml').decode())['workspace']['package']['version']
    license_raw = read(root / 'LICENSE', 65536)
    descriptor = {'format': 'zryna.native-installation-internal.v1', 'version': version,
                  'source': {'repository': 'https://github.com/zryna/zryna', 'commit': src['head'], 'tree': src['tree']},
                  'target': TARGETS[platform][0], 'cli': TARGETS[platform][1], 'protocols': [2, 3, 4], 'license_sha256': digest(license_raw)}
    payload = read(build / 'native-provider.json', 4096)
    require(payload == canonical(descriptor) and read(build / 'LICENSE', 65536) == license_raw, 'descriptor/license')
    binding = {'descriptor': digest(payload), 'head': src['head'], 'tree': src['tree']}
    require(receipt['binding'] == binding and receipt['descriptor_sha256'] == digest(payload)
            and receipt['descriptor_bytes'] == len(payload) and receipt['license_sha256'] == digest(license_raw), 'build binding')
    prepared = read(build / 'prepared-image', MAX); unprepared = read(build / 'unprepared-image', MAX)
    require(exact(image(prepared, binding, platform), {k: receipt['prepared_image'][k] for k in ('bytes', 'sha256')}), 'prepared bytes')
    require(exact(file_binding(unprepared), receipt['unprepared_image']) and prepared != unprepared
            and unprepared.count(MARKER_PREFIX + b'\0' * 147) == 1, 'unprepared image')
    for name in ('probe.rs', 'mutations.rs'):
        require(read(build / 'cargo-proof' / name) == read(root / 'tests/native-provider-qualification/installed-native-capability' / name), 'probe source')
    package = tomllib.loads(read(build / 'cargo-proof/Cargo.toml').decode())
    require(package['package'] == {'name': 'native-installation-proof', 'version': '0.0.0', 'edition': '2024'}
            and package['bin'] == [{'name': 'native-installation-proof', 'path': 'probe.rs'}], 'proof package')
    pm = ntpath if platform == 'win32' else posixpath
    deps = package['dependencies']; require(set(package) == {'workspace', 'package', 'bin', 'dependencies'}, 'package fields')
    require(set(deps) == {'serde_json', 'sha2', 'zryna-driver', 'zryna-source', 'zryna-diagnostics'}
            and deps['serde_json'] == '1.0.143' and deps['sha2'] == '=0.10.9', 'proof dependencies')
    origin = pm.dirname(pm.dirname(deps['zryna-driver']['path']))
    require(pm.isabs(origin), 'source origin path')
    for name in ('zryna-driver', 'zryna-source', 'zryna-diagnostics'):
        require(deps[name] == {'path': pm.join(origin, 'crates', name)}
                | ({'features': ['native-provider-internal']} if name == 'zryna-driver' else {}), 'source dependency path')
    root_keys = {tuple(p.get(k) for k in ('name', 'version', 'source', 'checksum')) for p in tomllib.loads(read(root / 'Cargo.lock').decode())['package']}
    require(all(tuple(p.get(k) for k in ('name', 'version', 'source', 'checksum')) in root_keys for p in
                tomllib.loads(read(build / 'cargo-proof/Cargo.lock').decode())['package'] if p['name'] != 'native-installation-proof'), 'dependency lock')
    profile = receipt.get('windows_toolchain')
    last = windows_observations(proof, profile, src, live) if platform == 'win32' else None
    records = {}
    for label in BUILD_LABELS:
        row, last = captured(build, label, src, last); records[label] = row
        env = row['selected_environment']; argv = row['argv']
        pm = ntpath if platform == 'win32' else posixpath
        require(pm.basename(row['cwd']) == 'cargo-proof' and pm.basename(pm.dirname(row['cwd'])) == 'build', 'build cwd')
        require(row['cwd'] == records[BUILD_LABELS[0]]['cwd'] and env['CARGO_TARGET_DIR'] == records[BUILD_LABELS[0]]['selected_environment']['CARGO_TARGET_DIR'], 'single build/target root')
        manifest_path = pm.join(row['cwd'], 'Cargo.toml')
        suffix = {'pinned-cargo-version': ['-V'], 'pinned-rustc-version': ['-vV'],
                  'actual-rustc-path': ['which', 'rustc'], 'actual-cargo-path': ['which', 'cargo'],
                  'generate-proof-lock': ['generate-lockfile', '--offline', '--manifest-path', manifest_path]}.get(label,
                  ['build', '--locked', '--offline', '--manifest-path', manifest_path])
        require(argv[1:] == suffix, 'exact build arguments')
        require(env.get('RUSTUP_TOOLCHAIN') == '1.97.1' and env.get('CARGO_PROFILE_DEV_OPT_LEVEL') == '1'
                and env.get('CARGO_PROFILE_DEV_DEBUG') == '0' and env.get('CARGO_PROFILE_DEV_CODEGEN_UNITS') == '16'
                and env.get('CARGO_INCREMENTAL') == '0' and env.get('CARGO_BUILD_JOBS') == '2'
                and not any(k.upper().startswith('NODE_') or k in ('RUSTC_WRAPPER', 'RUSTC_WORKSPACE_WRAPPER', 'RUSTFLAGS') for k in env), 'build environment')
        expected_binding = dict(zip(COMPILE_BINDINGS, binding.values())) if label == 'build-prepared-private-image' else {}
        require({k: env[k] for k in COMPILE_BINDINGS if k in env} == expected_binding, 'compile binding environment')
        if profile:
            require(env.get('RUSTC') == profile['tools']['rustc']['path'] and all(env.get(k) == v for k, v in profile['host_environment'].items()), 'observed build environment')
            expected_exe = profile['tools']['rustc' if label == 'pinned-rustc-version' else 'rustup' if label.startswith('actual-') else 'cargo']['path']
            require(argv[0] == expected_exe, 'build tool argv')
            fixed = windows_environment(profile, pm.dirname(row['cwd']), env['CARGO_TARGET_DIR'])
            require(env == fixed | expected_binding, 'closed build environment')
    require(read(build / 'pinned-cargo-version.stdout').decode().startswith('cargo 1.97.1 '), 'Cargo pin')
    text = read(build / 'pinned-rustc-version.stdout').decode().replace('\r\n', '\n')
    require('\nrelease: 1.97.1\n' in text and '\nhost: ' + TARGETS[platform][0] + '\n' in text, 'Rust pin')
    if profile:
        require(all(read(build / ('actual-' + k + '-path.stdout')).decode().strip() == profile['tools'][k]['path'] for k in ('cargo', 'rustc')), 'rustup selected actual paths')
    tools = document(build / 'pinned-tool-identities.json')
    actual = [read(build / ('actual-' + k + '-path.stdout')).decode().strip() for k in ('cargo', 'rustc')]
    expected_paths = {records['pinned-cargo-version']['argv'][0], records['pinned-rustc-version']['argv'][0],
                      records['actual-cargo-path']['argv'][0], *actual}
    require(set(tools) == expected_paths, 'exact selected tool identities')
    if profile:
        require(all(tools[t['path']] == {k: t[k] for k in ('bytes', 'sha256')} for t in
                    (profile['tools'][role] for role in ('cargo', 'rustc', 'rustup'))), 'original build tool parity')
    receipt = receipt | {'original_build_root': pm.dirname(records[BUILD_LABELS[0]]['cwd']),
                         'original_source_root': origin, 'original_target': records[BUILD_LABELS[0]]['selected_environment']['CARGO_TARGET_DIR']}
    return receipt, prepared


def unit_contract(unit, profile, origin, output, target):
    expected = [profile['tools']['cargo']['path'], 'test', '--locked', '--offline', '-p', 'zryna-driver', '--features',
                'native-provider-internal', '--lib', '--message-format=json',
                'distribution::native_installation::descriptor::tests', '--', '--nocapture']
    require(exact(unit['argv'], expected) and unit['cwd'] == origin, 'actual unit argv/cwd')
    env = windows_environment(profile, output, target)
    env.update(CARGO_PROFILE_TEST_DEBUG='0', CARGO_PROFILE_TEST_OPT_LEVEL='1', CARGO_PROFILE_TEST_CODEGEN_UNITS='16')
    require(exact(unit['selected_environment'], env), 'closed unit environment')


def verify_unit(root, proof, src, profile, origin, output, target):
    unit, _ = captured(proof, 'private-descriptor-units', src)
    unit_contract(unit, profile, origin, output, target)
    env = unit['selected_environment']
    raw = read(proof / 'private-descriptor-units.stdout', 16 * 1024 * 1024)
    text = raw.decode().replace('\r\n', '\n')
    require(len(re.findall(r'test result: ok\. 31 passed; 0 failed; 0 ignored;', text)) == 1, 'actual descriptor count')
    source = read(root / 'crates/zryna-driver/src/distribution/native_installation/descriptor_tests.rs').decode()
    names = set(re.findall(r'fn (independent_\w+)\(', source)) | set(re.findall(r'top_case!\(\s*(independent_\w+)', source))
    actual = re.findall(r'^test distribution::native_installation::descriptor::tests::(independent_\w+) \.\.\. ok$', text, re.M)
    require(len(names) == len(actual) == 31 and set(actual) == names, 'all source-selected descriptor units')
    artifacts = [decode(line) for line in raw.splitlines() if line.startswith(b'{')]
    paths = [a['executable'] for a in artifacts if a.get('reason') == 'compiler-artifact' and a.get('profile', {}).get('test') and a.get('executable')]
    binding = document(proof / 'private-descriptor-unit-image-binding.json')
    image_raw = read(proof / 'private-descriptor-unit-image.exe', MAX); image_header(image_raw, 'win32')
    require(paths == [binding['actual_cargo_test_path']] and binding['retained_path'] == 'private-descriptor-unit-image.exe'
            and ntpath.commonpath([paths[0], env['CARGO_TARGET_DIR']]) == env['CARGO_TARGET_DIR']
            and file_binding(image_raw) == {k: binding[k] for k in ('bytes', 'sha256')}, 'actual unit image')
