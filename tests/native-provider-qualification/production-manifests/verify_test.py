"""Synthetic hostile archives; these controls execute no compiler or syntax provider."""
import copy
import hashlib
import importlib.util
import json
from pathlib import Path, PurePosixPath, PureWindowsPath
import shutil
import subprocess
import sys
import tempfile
from types import SimpleNamespace
import unittest
from unittest.mock import patch

sys.dont_write_bytecode = True
_spec = importlib.util.spec_from_file_location('seven_control_reader', Path(__file__).with_name('verify.py'))
v = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(v)
ROOT = Path(__file__).resolve().parents[3]
ERRORS = (AssertionError, ValueError, KeyError, TypeError, OSError)


class ManifestControls(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.base = tempfile.TemporaryDirectory()
        cls.source = Path(cls.base.name)/'source'
        cls.source.mkdir()
        for name in (v.m.REGISTRY, *(f'tests/m3-fixtures/conformance/{name}.zry' for name in v.m.FROZEN)):
            path = cls.source/name
            path.parent.mkdir(parents=True, exist_ok=True)
            shutil.copy2(ROOT/name, path)
        for command in (['init', '-q'], ['add', '.'], ['-c', 'user.name=synthetic-control', '-c',
                        'user.email=synthetic@example.invalid', 'commit', '-qm', 'synthetic source fixture']):
            subprocess.run(['git', '-C', str(cls.source), *command], check=True, capture_output=True)
        cls.head = v.git(cls.source, 'rev-parse', 'HEAD')
        cls.tree = v.git(cls.source, 'rev-parse', 'HEAD^{tree}')
        cls.inputs = v.committed_inputs(cls.source, cls.head)

    @classmethod
    def tearDownClass(cls):
        cls.base.cleanup()

    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.output = Path(self.temp.name)/'proof'
        self.make('linux')

    def write(self, path, value):
        path.write_bytes(v.m.canonical(value))

    def inventory(self, path):
        return {p.relative_to(path).as_posix(): {'bytes': p.stat().st_size, 'sha256': v.digest(p)}
                for p in path.rglob('*') if p.is_file()}

    def make(self, platform):
        if self.output.exists():
            shutil.rmtree(self.output)
        self.output.mkdir()
        windows = platform == 'win32'
        path = PureWindowsPath if windows else PurePosixPath
        source = str(path('C:/source' if windows else str(self.source)))
        output = str(path('C:/evidence/proof' if windows else str(self.output)))
        prior = str(path('C:/evidence/cli' if windows else str(Path(self.temp.name)/'cli')))
        target = str(path('C:/target' if windows else str(Path(self.temp.name)/'target')))
        tool_base = path('C:/tools' if windows else '/synthetic-tools')
        suffix = '.exe' if windows else ''
        python = str(tool_base/('python'+suffix))
        tools = {name: {'path': str(tool_base/(name+suffix)), 'sha256': 'a'*64, 'version': version}
                 for name, version in (('cargo', 'cargo 1.97.1 synthetic'),
                                        ('rustc', 'rustc 1.97.1 synthetic'), ('node', 'v22.22.1'))}
        binaries = {name: {'path': str(path(prior)/(name+'-cli'+suffix)), 'sha256': 'b'*64}
                    for name in ('default', 'feature')}
        build = self.output/'cli-build'
        (build/'smoke').mkdir(parents=True)
        commands = []
        for index, label in enumerate(v.BUILD_LABELS):
            stdout = b''
            if index < 3:
                name = label.removesuffix('-version')
                argv = [tools[name]['path'], '--version']
                stdout = (tools[name]['version']+'\n').encode()
            elif index < 5:
                argv = [tools['cargo']['path'], 'build', '--locked', '--offline', '-p', 'zryna']
                if index == 4:
                    argv += ['--features', 'native-provider-internal']
            else:
                argv = [python, '-B', str(path(source)/'scripts/run-native-cli-smoke.py'), '--root', source,
                        '--default-cli', binaries['default']['path'], '--feature-cli', binaries['feature']['path'],
                        '--node', tools['node']['path'], '--cargo', tools['cargo']['path'],
                        '--rustc', tools['rustc']['path'], '--output', str(path(prior)/'smoke')]
            commands.append({'label': label, 'argv': argv, 'exit': 0, 'cwd': source, 'target': target})
            (build/(label+'.stdout')).write_bytes(stdout)
            (build/(label+'.stderr')).write_bytes(b'')
        old_ids = ('m1', 'm2', 'm3-pair', 'm3-array', 'm3-borrow', 'm3-vec', 'm3-string',
                   'm3-owned-aggregate', 'm3-owned-vec', 'm1-negative', 'm1-bool', 'm3-moved',
                   'm2-bare-import', 'm2-cycle-main', 'm2-cycle-dep', 'default-feature-disabled',
                   'ordinary-feature-build-needs-node', 'private-project-denied', 'private-component-denied',
                   'source-checkout-still-needs-cargo', 'source-and-binary-identity')
        smoke = {'version': 1, 'head': self.head, 'tree': self.tree, 'inputs': self.inputs,
                 'generated_inputs': {}, 'binaries': {'default_cli': binaries['default'], 'feature_cli': binaries['feature'],
                    **{name: {k: tools[name][k] for k in ('path', 'sha256')} for name in tools}},
                 'path': str(path(prior)/'smoke/empty-path'), 'records': [
                    {'id': name, 'status': 'passed', **({} if name == 'source-and-binary-identity' else {'detail': {}})}
                    for name in old_ids], 'counts': {'passed': 21, 'failed': 0, 'ignored': 0},
                 'blocked_acceptance': ['ordinary installed CLI without Node/pnpm/Cargo', 'public activation',
                    'native run selection', 'cross-platform installed distribution proof'], 'public_activation': False}
        self.write(build/'smoke/receipt.json', smoke)
        self.write(build/'cli-smoke.stdout', {key: smoke[key] for key in
                   ('head', 'counts', 'blocked_acceptance', 'public_activation')})
        cli = {'format': 'zryna.private-cli-ci.v1', 'head': self.head, 'tree': self.tree, 'platform': platform,
               'root': source, 'target': target, 'status': 'passed', 'inputs': self.inputs, 'tools': tools,
               'binaries': binaries, 'commands': commands, 'smoke_receipt': 'smoke/receipt.json',
               'public_activation': False, 'installed_distribution_acceptance': False, 'python': python}
        self.write(build/'ci-receipt.json', cli)
        (self.output/'empty-runtime-path').mkdir()
        contracts, generated = v.m.contracts(self.source, path(output), self.inputs)
        self.contracts = contracts
        self.receipt = {'format': 'zryna.production-manifest-parity.v1', 'head': self.head, 'tree': self.tree,
                        'platform': platform, 'root': source, 'output': output, 'cli_proof': prior,
                        'cli_receipt_sha256': v.digest(build/'ci-receipt.json'), 'inputs': self.inputs,
                        'generated_inputs': generated, 'tools': tools, 'binaries': binaries,
                        'cli_build_files': self.inventory(build), 'python': python,
                        'runtime_path': str(path(output)/'empty-runtime-path'), 'commands': [], 'records': [],
                        'status': 'passed', 'run_id': '123', 'run_attempt': '1', 'public_activation': False,
                        'installed_distribution_acceptance': False, 'scope': copy.deepcopy(v.SCOPE),
                        'counts': {'passed': 7, 'failed': 0, 'ignored': 0}}
        for label in v.IDS:
            contract = contracts[label]
            stem = 'production-manifest-'+label
            success = {'version': 1, 'command': 'build', 'diagnostics': [], 'results': [], 'ok': True,
                       'manifest': f'.zryna/out/{stem}.build/zryna-manifest-v3.json'}
            for role in ('native', 'bootstrap'):
                bundle = self.output/(label+'-'+role+'-bundle')
                for target_name, extension in zip(v.m.TARGETS, v.m.EXTENSIONS):
                    directory = bundle/target_name
                    directory.mkdir(parents=True)
                    data = b'export function score() { return 1; }\n'
                    if target_name == 'webassembly':
                        data = b'\0asm\x01\0\0\0'
                    elif target_name == 'native':
                        elf = bytearray(64)
                        elf[:7] = b'\x7fELF\x02\x01\x01'
                        elf[16:24] = b'\x01\0\x3e\0\x01\0\0\0'
                        elf[52:54] = b'\x40\0'
                        data = bytes(elf)
                    (directory/(stem+'.'+extension)).write_bytes(data)
                expected = v.m.expected_manifest(label, contract, self.inventory(bundle),
                    {key: 'c'*64 for key in ('type_universe_sha256', 'linear32_sha256', 'linux_x86_64_sha256')},
                    {'identifier': 'zryna-ownership-runtime-v1', 'version': 1, 'sha256': 'd'*64})
                self.write(bundle/'zryna-manifest-v3.json', expected)
            inventory = self.inventory(self.output/(label+'-native-bundle'))
            self.receipt['records'].append({'id': label, 'status': 'passed', 'source': contract['source'],
                'profile': 'data-ownership-v1', 'files': inventory, 'initial_output_absent': True,
                'final_output_absent': True, 'success_json_exact': True, 'manifest_bytes_exact': True,
                'create_only': True, 'node_on_path': False, 'pnpm_on_path': False})
            for role in v.ROLES:
                name = label+'-'+role
                argv = [binaries['default' if role == 'bootstrap' else 'feature']['path'], 'build',
                        contract['source'], '--root', source, '--name', stem, '--target', 'all', '--json',
                        '--profile', 'data-ownership-v1']
                argv += ['--node', tools['node']['path']] if role == 'bootstrap' else ['--native-frontend']
                value = success if role != 'create-only' else {'version': 1, 'command': 'build', 'manifest': None,
                    'ok': False, 'results': [], 'diagnostics': [{'code': 'ZRYNA-C1009', 'guidance':
                    'use a writable real output filesystem and a fresh artifact stem', 'message':
                    'create-only output bundle already exists', 'primary': {'kind': 'global'}, 'severity': 'error'}]}
                self.write(self.output/(name+'.stdout'), value)
                (self.output/(name+'.stderr')).write_bytes(b'')
                logs = {suffix: {'bytes': (self.output/(name+'.'+suffix)).stat().st_size,
                                  'sha256': v.digest(self.output/(name+'.'+suffix))} for suffix in ('stdout', 'stderr')}
                self.receipt['commands'].append({'label': name, 'argv': argv, 'cwd': source,
                    'exit': 4 if role == 'create-only' else 0, 'timed_out': False, **logs})
        self.save()

    def save(self):
        self.write(self.output/'receipt.json', self.receipt)

    def admit(self):
        self.save()
        return v.verify(self.source, self.output, self.head, self.receipt['platform'], run_id='123', run_attempt='1')

    def reject(self):
        with self.assertRaises(ERRORS):
            self.admit()

    def change_manifests(self, change, label='m3-enum'):
        for role in ('native', 'bootstrap'):
            bundle = self.output/(label+'-'+role+'-bundle')
            manifest = v.read(bundle/'zryna-manifest-v3.json')
            change(manifest)
            manifest['graph_sha256'] = v.m.graph_digest(3, manifest['sources'], manifest['edges'])
            self.write(bundle/'zryna-manifest-v3.json', manifest)
        next(row for row in self.receipt['records'] if row['id'] == label)['files'] = self.inventory(
            self.output/(label+'-native-bundle'))

    def log_change(self, label, change):
        path = self.output/(label+'.stdout')
        data = v.strict(path.read_bytes())
        change(data)
        self.write(path, data)
        next(row for row in self.receipt['commands'] if row['label'] == label)['stdout'] = {
            'bytes': path.stat().st_size, 'sha256': v.digest(path)}

    def test_exact_seven_and_real_frozen_registry_contracts(self):
        self.assertEqual(len(self.contracts), 7)
        self.assertEqual(len(self.receipt['generated_inputs']), 10)
        self.assertEqual(self.admit()['counts']['passed'], 7)

    def test_windows_archive_path_contract_without_windows_execution(self):
        self.make('win32')
        self.assertEqual(self.admit()['platform'], 'win32')

    def test_explicit_edge_target_is_bound_even_when_graph_digest_unchanged(self):
        self.change_manifests(lambda value: value['edges'][0].update(target=value['entrypoint']))
        self.reject()

    def test_enum_alias_and_imported_name_cannot_be_coherently_substituted(self):
        self.change_manifests(lambda value: value['edges'][0].update(local='score'))
        self.reject()

    def test_same_wrapper_cannot_swap_weak_source_bodies(self):
        self.change_manifests(lambda value: value['sources'][1].update(sha256=v.m.FROZEN['weak-live-body']),
                              'm3-weak-expired')
        self.reject()

    def test_coherent_non_build_manifest_and_invalid_types_reject(self):
        for field, value in [('version', True), ('protocol_version', 3), ('profile', 'zryna-control-flow-v1'),
                             ('command', 'run'), ('invocation', {'export': 'score', 'arguments': []}),
                             ('results', [{}]), ('diagnostics', [{}])]:
            with self.subTest(field=field):
                self.make('linux')
                self.change_manifests(lambda data: data.update({field: value}))
                self.reject()

    def test_layout_and_runtime_shape_do_not_accept_boolean_version(self):
        self.change_manifests(lambda data: data['runtime_abi'].update(version=True))
        self.reject()
        self.make('linux')
        self.change_manifests(lambda data: data['layouts'].update(linear32_sha256='not-a-hash'))
        self.reject()

    def test_rehashed_both_metadata_cannot_change_native_format(self):
        self.change_manifests(lambda data: data['artifacts'][2]['metadata'].update(format='elf-executable'))
        self.reject()

    def test_rehashed_both_manifest_canonical_order_and_newline_reject(self):
        for transform in (lambda b: b.replace(b'\n', b'\r\n'), lambda b: b+b'\n',
                          lambda b: json.dumps(v.strict(b), sort_keys=True, indent=2).encode()+b'\n'):
            self.make('linux')
            for role in ('native', 'bootstrap'):
                path = self.output/f'm3-enum-{role}-bundle/zryna-manifest-v3.json'
                path.write_bytes(transform(path.read_bytes()))
            self.receipt['records'][2]['files'] = self.inventory(self.output/'m3-enum-native-bundle')
            self.reject()

    def test_strict_json_duplicate_float_and_nonfinite_reject(self):
        for data in (b'{"x":1,"x":1}', b'{"x":1.0}', b'{"x":NaN}'):
            with self.assertRaises((AssertionError, ValueError)):
                v.strict(data)

    def test_both_success_envelopes_cannot_change_manifest_path(self):
        for role in ('native', 'bootstrap'):
            self.log_change('m3-abi-'+role, lambda value: value.update(manifest='outside.json'))
        self.reject()

    def test_create_only_full_diagnostic_and_exact_exit_required(self):
        self.log_change('m3-abi-create-only', lambda value: value['diagnostics'][0].update(guidance='retry'))
        self.reject()
        self.make('linux')
        self.receipt['commands'][1]['exit'] = True
        self.reject()

    def test_bundle_bytes_must_match_even_when_inventory_manifest_rehashed(self):
        bundle = self.output/'m3-enum-bootstrap-bundle'
        path = bundle/'javascript/production-manifest-m3-enum.mjs'
        path.write_bytes(path.read_bytes()+b'// divergent\n')
        data = v.read(bundle/'zryna-manifest-v3.json')
        data['artifacts'][0].update(bytes=path.stat().st_size, sha256=v.digest(path))
        self.write(bundle/'zryna-manifest-v3.json', data)
        self.reject()

    def test_extra_and_missing_inventory_reject(self):
        (self.output/'unexpected').write_bytes(b'extra')
        self.reject()
        self.make('linux')
        path = self.output/'m3-abi-native-bundle/native/production-manifest-m3-abi.o'
        path.unlink()
        self.reject()

    def test_reparse_or_linked_bundle_root_rejects_without_host_link_privilege(self):
        selected = self.output/'m3-abi-native-bundle'
        original = Path.lstat
        for attributes in ({'st_file_attributes': 0x400}, {'st_mode': 0o120777}):
            def hostile(path):
                value = original(path)
                if path == selected:
                    fields = {'st_mode': value.st_mode, 'st_size': value.st_size}
                    fields.update(attributes)
                    return SimpleNamespace(**fields)
                return value
            with patch.object(Path, 'lstat', hostile):
                self.reject()

    def test_partial_failed_and_boolean_census_never_qualify(self):
        for change in (lambda r: r['records'].pop(), lambda r: r.update(status='failed'),
                       lambda r: r['counts'].update(passed=True),
                       lambda r: r['records'][0].update(status='failed')):
            self.make('linux')
            change(self.receipt)
            self.reject()

    def test_wrong_source_binary_node_command_and_run_binding_reject(self):
        for change in (lambda r: r['commands'][0]['argv'].append('--node'),
                       lambda r: r['commands'][0]['argv'].__setitem__(0, '/different-binary'),
                       lambda r: r['commands'][0]['argv'].__setitem__(2, 'different-source.zry'),
                       lambda r: r['commands'][0].update(cwd='/another-root'),
                       lambda r: r['commands'][2]['argv'].__setitem__(-1, '/wrong-node'),
                       lambda r: r.update(run_attempt='2')):
            self.make('linux')
            change(self.receipt)
            self.reject()

    def test_preceding_build_cannot_coherently_disable_feature(self):
        path = self.output/'cli-build/ci-receipt.json'
        prior = v.read(path)
        prior['commands'][4]['argv'] = prior['commands'][3]['argv']
        self.write(path, prior)
        self.receipt['cli_receipt_sha256'] = v.digest(path)
        self.receipt['cli_build_files'] = self.inventory(self.output/'cli-build')
        self.reject()

    def test_hidden_committed_source_corruption_is_rejected(self):
        path = self.source/v.m.REGISTRY
        original = path.read_bytes()
        subprocess.run(['git', '-C', str(self.source), 'update-index', '--assume-unchanged', v.m.REGISTRY], check=True)
        try:
            path.write_bytes(original+b' ')
            self.assertEqual(v.git(self.source, 'status', '--porcelain'), '')
            self.reject()
        finally:
            path.write_bytes(original)
            subprocess.run(['git', '-C', str(self.source), 'update-index', '--no-assume-unchanged', v.m.REGISTRY], check=True)


if __name__ == '__main__':
    unittest.main()
