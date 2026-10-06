#!/usr/bin/env python3
"""Host-independent closed-profile controls; fixtures are never tool authority."""
import copy
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

from toolchain import retain_original
from windows_build import environment, validate


def fixture():
    vc = r'C:\Program Files\Microsoft Visual Studio\2022\Enterprise\VC\Tools\MSVC\14.44.35207'
    sdk = r'C:\Program Files (x86)\Windows Kits\10'
    rust = r'C:\Users\runneradmin\.rustup\toolchains\1.97.1-x86_64-pc-windows-msvc\bin'
    paths = {'cargo': rust + r'\cargo.exe', 'rustc': rust + r'\rustc.exe',
             'rustup': r'C:\Users\runneradmin\.cargo\bin\rustup.exe',
             'linker': vc + r'\bin\Hostx64\x64\link.exe', 'dumpbin': vc + r'\bin\Hostx64\x64\dumpbin.exe'}
    def row(role, path):
        return {'path': path, 'bytes': 1, 'sha256': '0' * 64,
                'raw_relative_path': 'tool-originals/' + role.replace('.', '-') + ('.exe' if role in (*paths, 'python') else '.bin')}
    lib = ';'.join([vc + r'\lib\x64', sdk + r'\Lib\10.0.26100.0\um\x64', sdk + r'\Lib\10.0.26100.0\ucrt\x64'])
    return {'schema_version': 1, 'platform': 'win32', 'target': 'x86_64-pc-windows-msvc',
            'python': {**row('python', r'C:\hostedtoolcache\windows\Python\3.12.10\x64\python.exe'), 'version': '3.12.10'},
            'tools': {name: row(name, path) for name, path in paths.items()},
            'rust_version': '1.97.1', 'cargo_version': '1.97.1',
            'msvc': {'version': '14.44.35207', 'root': vc, 'sdk_version': '10.0.26100.0', 'sdk_root': sdk},
            'sdk_inputs': {role: row(role, path) for role, path in {
                'kernel32.lib': sdk + r'\Lib\10.0.26100.0\um\x64\kernel32.lib',
                'ucrt.lib': sdk + r'\Lib\10.0.26100.0\ucrt\x64\ucrt.lib',
                'vcruntime.lib': vc + r'\lib\x64\vcruntime.lib', 'libcmt.lib': vc + r'\lib\x64\libcmt.lib'}.items()},
            'host_environment': {'SystemRoot': r'C:\Windows',
                'PATH': ';'.join([rust, vc + r'\bin\Hostx64\x64', r'C:\Windows\System32', r'C:\Windows']),
                'LIB': lib, 'LIBPATH': lib,
                'INCLUDE': ';'.join([vc + r'\include', *[sdk + '\\Include\\10.0.26100.0\\' + part for part in ('ucrt', 'um', 'shared')]])},
            'observation_scope': 'per-run observed exact tools and selected core SDK inputs; no upstream supplier byte authority',
            'cargo_home': r'C:\Users\runneradmin\.cargo', 'rustup_home': r'C:\Users\runneradmin\.rustup'}


class ClosedProfileControls(unittest.TestCase):
    def test_declared_profile_fixture_is_syntax_only(self):
        validate(fixture())

    def test_closed_build_environment_uses_selected_linker_and_no_runtime_hook(self):
        env = environment(fixture(), r'C:\owned\proof', r'C:\owned\target')
        self.assertEqual(env['CARGO_BUILD_JOBS'], '2')
        self.assertEqual(env['CARGO_PROFILE_DEV_OPT_LEVEL'], '1')
        self.assertEqual(env['CARGO_TARGET_X86_64_PC_WINDOWS_MSVC_LINKER'], fixture()['tools']['linker']['path'])
        for key in ('NODE_OPTIONS', 'NODE_PATH', 'RUSTFLAGS', 'RUSTC_WRAPPER', 'RUSTC_WORKSPACE_WRAPPER'):
            self.assertNotIn(key, env)

    def test_hostile_profile_fields_and_paths_reject(self):
        mutations = [
            lambda p: p.update(extra='unreviewed'),
            lambda p: p.update(schema_version=True),
            lambda p: p.update(platform='linux'),
            lambda p: p.update(target='x86_64-unknown-linux-gnu'),
            lambda p: p['python'].update(version='3.13.5'),
            lambda p: p.update(rust_version='stable'),
            lambda p: p['tools'].pop('rustup'),
            lambda p: p['tools']['cargo'].update(bytes=True),
            lambda p: p['tools']['cargo'].update(bytes=128 * 1024 * 1024 + 1),
            lambda p: p['tools']['cargo'].update(sha256='A' * 64),
            lambda p: p['tools']['cargo'].update(raw_relative_path='../outside'),
            lambda p: p['tools']['cargo'].update(path='relative.exe'),
            lambda p: p['tools']['linker'].update(path=r'C:\unapproved\link.exe'),
            lambda p: p['msvc'].update(root=r'C:\unapproved\MSVC'),
            lambda p: p['msvc'].update(version='latest'),
            lambda p: p['msvc'].update(sdk_version='latest'),
            lambda p: p['sdk_inputs'].pop('kernel32.lib'),
            lambda p: p['sdk_inputs']['ucrt.lib'].update(path=r'C:\foreign\ucrt.lib'),
            lambda p: p['host_environment'].update(PATH=p['host_environment']['PATH'] + r';C:\unapproved'),
            lambda p: p['host_environment'].update(LIB=p['host_environment']['LIB'] + r';C:\unapproved'),
            lambda p: p['host_environment'].update(INCLUDE=p['host_environment']['INCLUDE'] + r';C:\unapproved'),
            lambda p: p['host_environment'].update(NODE_OPTIONS='--require hook'),
            lambda p: p.update(observation_scope='trusted supplier'),
        ]
        for mutate in mutations:
            profile = copy.deepcopy(fixture())
            mutate(profile)
            with self.subTest(mutate=mutate), self.assertRaises((AssertionError, KeyError)):
                validate(profile)


class OriginalToolRetentionControls(unittest.TestCase):
    def test_real_sys_executable_string_and_path_retain_exact_bytes(self):
        interpreter = Path(sys.executable).resolve()
        expected = interpreter.read_bytes()
        with tempfile.TemporaryDirectory() as temporary:
            output = Path(temporary)
            direct = retain_original(output / 'path-input', 'python', interpreter)
            program = ('import json, sys\nfrom pathlib import Path\n'
                       'from toolchain import retain_original\n'
                       'assert type(sys.executable) is str\n'
                       'print(json.dumps(retain_original(Path(sys.argv[1]), "python", sys.executable)))\n')
            process = subprocess.run([str(interpreter), '-B', '-c', program, str(output / 'actual-sys-executable')],
                                     cwd=Path(__file__).resolve().parent, stdin=subprocess.DEVNULL,
                                     capture_output=True, text=True, timeout=30, check=True)
            self.assertEqual(process.stderr, '')
            actual = json.loads(process.stdout)
            self.assertEqual(actual, direct)
            self.assertEqual(actual['path'], str(interpreter))
            for label in ('path-input', 'actual-sys-executable'):
                self.assertEqual((output / label / actual['raw_relative_path']).read_bytes(), expected)
            with self.assertRaises(FileExistsError):
                retain_original(output / 'path-input', 'python', str(interpreter))

    def test_unknown_retention_role_rejects_before_output(self):
        with tempfile.TemporaryDirectory() as temporary:
            output = Path(temporary) / 'not-created'
            with self.assertRaises(AssertionError):
                retain_original(output, '../outside', str(Path(sys.executable).resolve()))
            self.assertFalse(output.exists())


if __name__ == '__main__':
    unittest.main(verbosity=2)
