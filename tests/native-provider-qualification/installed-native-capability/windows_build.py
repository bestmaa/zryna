"""Per-run Windows tool pins; observations do not establish upstream supplier trust."""
import hashlib
import json
import os
from pathlib import Path, PureWindowsPath
import re
import stat
import sys

if not __debug__:
    raise RuntimeError('Windows installation proof requires assertions enabled')

PYTHON = '3.12.10'
RUST = '1.97.1'
TARGET = 'x86_64-pc-windows-msvc'
TOOLS = ('cargo', 'rustc', 'rustup', 'linker', 'dumpbin')
SDK_INPUTS = ('kernel32.lib', 'ucrt.lib', 'vcruntime.lib', 'libcmt.lib')
MAX_TOOL = 128 * 1024 * 1024


def identity(path):
    path = Path(path)
    assert path.is_absolute()
    info = path.lstat()
    assert stat.S_ISREG(info.st_mode) and not path.is_symlink()
    assert not getattr(info, 'st_file_attributes', 0) & 0x400
    assert 0 < info.st_size <= MAX_TOOL
    raw = path.read_bytes()
    after = path.lstat()
    assert (info.st_size, info.st_mtime_ns, info.st_ino) == (after.st_size, after.st_mtime_ns, after.st_ino)
    return {'path': str(path), 'bytes': len(raw), 'sha256': hashlib.sha256(raw).hexdigest()}


def validate(profile):
    assert set(profile) == {'schema_version', 'platform', 'target', 'python', 'tools', 'rust_version',
                            'cargo_version', 'msvc', 'sdk_inputs', 'host_environment', 'observation_scope',
                            'cargo_home', 'rustup_home'}
    assert type(profile['schema_version']) is int and profile['schema_version'] == 1
    assert profile['platform'] == 'win32' and profile['target'] == TARGET
    assert profile['python']['version'] == PYTHON
    assert profile['rust_version'] == profile['cargo_version'] == RUST
    assert set(profile['tools']) == set(TOOLS) and set(profile['sdk_inputs']) == set(SDK_INPUTS)
    for role, row in [('python', profile['python']), *profile['tools'].items(), *profile['sdk_inputs'].items()]:
        assert set(row) == ({'path', 'bytes', 'sha256', 'raw_relative_path', 'version'} if role == 'python'
                            else {'path', 'bytes', 'sha256', 'raw_relative_path'})
        assert PureWindowsPath(row['path']).is_absolute() and '..' not in PureWindowsPath(row['path']).parts
        assert type(row['bytes']) is int and 0 < row['bytes'] <= MAX_TOOL
        assert re.fullmatch('[a-f0-9]{64}', row['sha256'])
        assert row['raw_relative_path'] == 'tool-originals/' + role.replace('.', '-') + ('.exe' if role in (*TOOLS, 'python') else '.bin')
    msvc = profile['msvc']
    assert set(msvc) == {'version', 'root', 'sdk_version', 'sdk_root'}
    assert re.fullmatch(r'14\.[0-9]+\.[0-9]+', msvc['version'])
    assert re.fullmatch(r'10\.0\.[0-9]+\.0', msvc['sdk_version'])
    vc = PureWindowsPath(msvc['root'])
    assert re.fullmatch(r'[A-Za-z]:\\Program Files\\Microsoft Visual Studio\\2022\\Enterprise\\VC\\Tools\\MSVC\\' + re.escape(msvc['version']), str(vc), re.I)
    native = vc / 'bin/Hostx64/x64'
    assert PureWindowsPath(profile['tools']['linker']['path']) == native / 'link.exe'
    assert PureWindowsPath(profile['tools']['dumpbin']['path']) == native / 'dumpbin.exe'
    assert PureWindowsPath(msvc['sdk_root']) == PureWindowsPath(r'C:\Program Files (x86)\Windows Kits\10')
    expected_libraries = {'kernel32.lib': PureWindowsPath(msvc['sdk_root']) / 'Lib' / msvc['sdk_version'] / 'um/x64/kernel32.lib',
                          'ucrt.lib': PureWindowsPath(msvc['sdk_root']) / 'Lib' / msvc['sdk_version'] / 'ucrt/x64/ucrt.lib',
                          'vcruntime.lib': vc / 'lib/x64/vcruntime.lib', 'libcmt.lib': vc / 'lib/x64/libcmt.lib'}
    assert all(PureWindowsPath(profile['sdk_inputs'][role]['path']) == path for role, path in expected_libraries.items())
    assert profile['observation_scope'] == 'per-run observed exact tools and selected core SDK inputs; no upstream supplier byte authority'
    host = profile['host_environment']
    assert set(host) == {'SystemRoot', 'PATH', 'LIB', 'INCLUDE', 'LIBPATH'}
    assert PureWindowsPath(host['SystemRoot']) == PureWindowsPath(r'C:\Windows')
    for key in ('PATH', 'LIB', 'INCLUDE', 'LIBPATH'):
        assert type(host[key]) is str and 0 < len(host[key]) <= 8192
        entries = host[key].split(';')
        assert all(PureWindowsPath(p).is_absolute() and '..' not in PureWindowsPath(p).parts for p in entries)
        assert len(set(p.casefold() for p in entries)) == len(entries)
    expected_path = [str(PureWindowsPath(profile['tools']['cargo']['path']).parent), str(native), r'C:\Windows\System32', r'C:\Windows']
    assert [PureWindowsPath(p) for p in host['PATH'].split(';')] == [PureWindowsPath(p) for p in expected_path]
    expected_lib = [vc / 'lib/x64', PureWindowsPath(msvc['sdk_root']) / 'Lib' / msvc['sdk_version'] / 'um/x64', PureWindowsPath(msvc['sdk_root']) / 'Lib' / msvc['sdk_version'] / 'ucrt/x64']
    assert [PureWindowsPath(p) for p in host['LIB'].split(';')] == expected_lib
    assert host['LIBPATH'] == host['LIB']
    expected_include = [vc / 'include', *[PureWindowsPath(msvc['sdk_root']) / 'Include' / msvc['sdk_version'] / part for part in ('ucrt', 'um', 'shared')]]
    assert [PureWindowsPath(p) for p in host['INCLUDE'].split(';')] == expected_include
    assert all(PureWindowsPath(profile[key]).is_absolute() for key in ('cargo_home', 'rustup_home'))


def verify_profile(profile):
    validate(profile)
    assert sys.platform == 'win32' and sys.version.split()[0] == PYTHON
    for row in [profile['python'], *profile['tools'].values(), *profile['sdk_inputs'].values()]:
        assert identity(row['path']) == {key: row[key] for key in ('path', 'bytes', 'sha256')}


def environment(profile, output, target):
    validate(profile)
    retained, build_target = PureWindowsPath(str(output)), PureWindowsPath(str(target))
    assert retained.is_absolute() and build_target.is_absolute()
    assert '..' not in retained.parts and '..' not in build_target.parts
    assert not retained.is_relative_to(build_target) and not build_target.is_relative_to(retained)
    state = build_target / 'private-process-state'
    return {**profile['host_environment'], 'CARGO_HOME': profile['cargo_home'], 'RUSTUP_HOME': profile['rustup_home'],
            'RUSTUP_TOOLCHAIN': RUST, 'RUSTC': profile['tools']['rustc']['path'],
            'CARGO_TARGET_X86_64_PC_WINDOWS_MSVC_LINKER': profile['tools']['linker']['path'],
            'CARGO_TARGET_DIR': str(target), 'CARGO_PROFILE_DEV_DEBUG': '0', 'CARGO_PROFILE_DEV_CODEGEN_UNITS': '16',
            'CARGO_PROFILE_DEV_OPT_LEVEL': '1', 'CARGO_INCREMENTAL': '0', 'CARGO_BUILD_JOBS': '2',
            'TEMP': str(state / 'Temp'), 'TMP': str(state / 'Temp'),
            'USERPROFILE': str(state), 'HOME': str(state),
            'APPDATA': str(state / 'AppData/Roaming'), 'LOCALAPPDATA': str(state / 'AppData/Local'),
            'LANG': 'C', 'LC_ALL': 'C'}


def prepare_state(target):
    """Create fresh mutable child state outside the closed retained proof."""
    target = Path(target)
    target.mkdir(mode=0o700)
    state = target / 'private-process-state'
    state.mkdir(mode=0o700)
    (state / 'AppData').mkdir(mode=0o700)
    for path in (state / 'Temp', state / 'AppData/Roaming', state / 'AppData/Local'):
        path.mkdir(mode=0o700)
