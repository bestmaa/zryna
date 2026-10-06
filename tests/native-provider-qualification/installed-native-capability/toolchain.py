"""Observe original installed Windows tools before compilation; never issue syntax."""
import os
import ntpath
from pathlib import Path, PureWindowsPath
import re
import sys

from build import command, sha, write
from windows_build import PYTHON, SDK_INPUTS, TARGET, TOOLS, identity, validate, verify_profile


def retain_original(output, role, path):
    assert role in (*TOOLS, 'python', *SDK_INPUTS)
    path = Path(path)
    row = identity(path)
    relative = 'tool-originals/' + role.replace('.', '-') + ('.exe' if role in (*TOOLS, 'python') else '.bin')
    raw = path.read_bytes()
    assert sha(raw) == row['sha256']
    write(output / relative, raw)
    return {**row, 'raw_relative_path': relative}


def verify_linker_help(profile, row, stdout, stderr):
    """Accept the observed help exit only with original, bound LINK help streams."""
    assert type(row['exit']) is int and row['exit'] == 1100 and row['direct_child_reaped'] is True
    assert row['source_unchanged'] is True and row['source_before'] == row['source_after']
    assert row['argv'] == [profile['tools']['linker']['path'], '/?']
    assert row['selected_environment'] == profile['host_environment'] and ntpath.basename(row['cwd']) == 'toolchain'
    assert row['stdout'] == {'bytes': len(stdout), 'sha256': sha(stdout)}
    assert row['stderr'] == {'bytes': len(stderr), 'sha256': sha(stderr)} and stderr == b''
    assert 0 < len(stdout) <= 64 * 1024
    text = stdout.replace(b'\r\n', b'\n')
    banner = re.match(rb'\AMicrosoft \(R\) Incremental Linker Version (14\.\d+\.\d+\.\d+)\n'
                      rb'Copyright \(C\) Microsoft Corporation\.  All rights reserved\.\n\n'
                      rb' usage: LINK \[options\] \[files\] \[@commandfile\]\n\n   options:\n\n', text)
    assert banner and banner.group(1).decode().split('.')[:2] == profile['msvc']['version'].split('.')[:2]
    assert all(option in text for option in (b'      /ALIGN:', b'      /OUT:', b'      /VERSION:'))
    assert b'\nLINK :' not in text


def capture(source, output, cargo, rustc, rustup):
    assert sys.platform == 'win32' and sys.version.split()[0] == PYTHON
    output.mkdir(mode=0o700)
    vc_version = os.environ['VCToolsVersion'].strip('\\/')
    vc = Path(os.environ['VCToolsInstallDir'].rstrip('\\/'))
    sdk_version = os.environ['WindowsSDKVersion'].strip('\\/')
    sdk = Path(os.environ['WindowsSdkDir'].rstrip('\\/'))
    native = vc / 'bin/Hostx64/x64'
    paths = {'cargo': Path(cargo), 'rustc': Path(rustc), 'rustup': Path(rustup),
             'linker': native / 'link.exe', 'dumpbin': native / 'dumpbin.exe'}
    libraries = {'kernel32.lib': sdk / 'Lib' / sdk_version / 'um/x64/kernel32.lib',
                 'ucrt.lib': sdk / 'Lib' / sdk_version / 'ucrt/x64/ucrt.lib',
                 'vcruntime.lib': vc / 'lib/x64/vcruntime.lib', 'libcmt.lib': vc / 'lib/x64/libcmt.lib'}

    def original(role, path):
        return retain_original(output, role, path)

    lib = [str(vc / 'lib/x64'), str(sdk / 'Lib' / sdk_version / 'um/x64'), str(sdk / 'Lib' / sdk_version / 'ucrt/x64')]
    host = {'SystemRoot': os.environ['SystemRoot'],
            'PATH': ';'.join([str(Path(cargo).parent), str(native), r'C:\Windows\System32', r'C:\Windows']),
            'LIB': ';'.join(lib), 'LIBPATH': ';'.join(lib),
            'INCLUDE': ';'.join([str(vc / 'include'), *[str(sdk / 'Include' / sdk_version / part) for part in ('ucrt', 'um', 'shared')]])}
    profile = {'schema_version': 1, 'platform': 'win32', 'target': TARGET,
               'python': {**original('python', sys.executable), 'version': PYTHON},
               'tools': {role: original(role, path) for role, path in paths.items()},
               'rust_version': '1.97.1', 'cargo_version': '1.97.1',
               'msvc': {'version': vc_version, 'root': str(vc), 'sdk_version': sdk_version, 'sdk_root': str(sdk)},
               'sdk_inputs': {role: original(role, libraries[role]) for role in SDK_INPUTS},
               'host_environment': host,
               'observation_scope': 'per-run observed exact tools and selected core SDK inputs; no upstream supplier byte authority',
               'cargo_home': os.environ.get('CARGO_HOME', str(Path.home() / '.cargo')),
               'rustup_home': os.environ.get('RUSTUP_HOME', str(Path.home() / '.rustup'))}
    validate(profile)
    verify_profile(profile)
    for role, argv in [('python', [sys.executable, '--version']),
                       ('cargo', [str(cargo), '-V']), ('rustc', [str(rustc), '-vV']),
                       ('linker', [str(paths['linker']), '/?']), ('dumpbin', [str(paths['dumpbin']), '/?'])]:
        row = command(output, 'observed-' + role + '-version', argv, output, host, source,
                      timeout=60, linker_help=role == 'linker')
        if role == 'linker':
            verify_linker_help(profile, row, (output / 'observed-linker-version.stdout').read_bytes(),
                               (output / 'observed-linker-version.stderr').read_bytes())
    assert (output / 'observed-cargo-version.stdout').read_text().startswith('cargo 1.97.1 ')
    rust_text = (output / 'observed-rustc-version.stdout').read_text()
    assert '\nrelease: 1.97.1\n' in rust_text and '\nhost: ' + TARGET + '\n' in rust_text
    assert '\ncommit-hash: 8bab26f4f68e0e26f0bb7960be334d5b520ea452\n' in rust_text
    write(output / 'before.json', __import__('json').dumps(profile, indent=2).encode() + b'\n')
    verify_profile(profile)
    return profile
