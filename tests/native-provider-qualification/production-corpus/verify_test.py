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
_spec = importlib.util.spec_from_file_location('corpus_control_reader', Path(__file__).with_name('verify.py'))
v = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(v)
ROOT = Path(__file__).resolve().parents[3]
ERRORS = (AssertionError, ValueError, KeyError, TypeError, OSError)


class CorpusControls(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.base = tempfile.TemporaryDirectory()
        cls.source = Path(cls.base.name)/'source'
        cls.source.mkdir()
        registry = v.read(ROOT/v.c.REGISTRY)
        names = {v.c.REGISTRY, *(s['path'] for row in registry['fixtures'] for s in row['sources'])}
        for name in names:
            path = cls.source/name
            path.parent.mkdir(parents=True, exist_ok=True)
            shutil.copy2(ROOT/name, path)
        for command in (['init', '-q'], ['add', '.'], ['-c', 'user.name=synthetic-control', '-c',
                        'user.email=synthetic@example.invalid', 'commit', '-qm', 'synthetic source fixture']):
            subprocess.run(['git', '-C', str(cls.source), *command], check=True, capture_output=True)
        cls.head = v.v.git(cls.source, 'rev-parse', 'HEAD')
        cls.tree = v.v.git(cls.source, 'rev-parse', 'HEAD^{tree}')
        cls.inputs = v.v.committed_inputs(cls.source, cls.head)

    @classmethod
    def tearDownClass(cls):
            cls.base.cleanup()

    def setUp(self):
            self.temp = tempfile.TemporaryDirectory()
            self.addCleanup(self.temp.cleanup)
            self.output = Path(self.temp.name)/'proof'
            self.make('linux')

    def write(self, path, value):
            path.write_bytes(v.c.m.canonical(value))

    def inventory(self, path):
            return {p.relative_to(path).as_posix(): {'bytes': p.stat().st_size, 'sha256': v.digest(p)}
                    for p in path.rglob('*') if p.is_file()}

    def make(self, platform):
            if self.output.exists():
                shutil.rmtree(self.output)
            self.output.mkdir()
            windows = platform == 'win32'
            path = PureWindowsPath if windows else PurePosixPath
            # Archive metadata models its recorded host independently of this test's real host.
            source = str(path('C:/source' if windows else '/synthetic-source'))
            output = str(path('C:/evidence/proof' if windows else '/synthetic-evidence/proof'))
            prior = str(path('C:/evidence/cli' if windows else '/synthetic-evidence/cli'))
            target = str(path('C:/target' if windows else '/synthetic-target'))
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
            for index, label in enumerate(v.v.BUILD_LABELS):
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
            contracts = v.c.contracts(self.source, self.inputs)
            self.contracts = contracts
            self.receipt = {'format': 'zryna.production-corpus-parity.v1', 'head': self.head, 'tree': self.tree,
                            'platform': platform, 'root': source, 'output': output, 'cli_proof': prior,
                            'cli_receipt_sha256': v.digest(build/'ci-receipt.json'), 'inputs': self.inputs,
                            'generated_inputs': {}, 'tools': tools, 'binaries': binaries,
                            'cli_build_files': self.inventory(build), 'python': python,
                            'runtime_path': str(path(output)/'empty-runtime-path'), 'commands': [], 'records': [],
                            'status': 'passed', 'run_id': '123', 'run_attempt': '1', 'public_activation': False,
                            'installed_distribution_acceptance': False, 'scope': copy.deepcopy(v.SCOPE),
                            'counts': {'passed': 69, 'failed': 0, 'ignored': 0}}
            for label, contract in contracts.items():
                accepted = contract['disposition'] == 'accepted'
                stem = 'corpus-build-'+label
                success = {'version': 1, 'command': 'build', 'diagnostics': [], 'results': [], 'ok': True,
                           'manifest': f'.zryna/out/{stem}.build/zryna-manifest-v3.json'}
                files = {}
                if accepted:
                    for role in ('native', 'bootstrap'):
                        bundle = self.output/(label+'-'+role+'-bundle')
                        for target_name, extension in zip(v.c.m.TARGETS, v.c.m.EXTENSIONS):
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
                        expected = v.c.expected_manifest(label, contract, self.inventory(bundle),
                            {key: 'c'*64 for key in ('type_universe_sha256', 'linear32_sha256', 'linux_x86_64_sha256')},
                            {'identifier': 'zryna-ownership-runtime-v1', 'version': 1, 'sha256': 'd'*64})
                        self.write(bundle/'zryna-manifest-v3.json', expected)
                    files = self.inventory(self.output/(label+'-native-bundle'))
                self.receipt['records'].append({'id': label, 'disposition': contract['disposition'],
                    'status': 'passed', 'source': contract['source'], 'profile': 'data-ownership-v1',
                    'files': files, 'initial_output_absent': True, 'final_output_absent': True,
                    'provider_json_exact': True, 'create_only': accepted, 'node_on_path': False, 'pnpm_on_path': False})
                for role in (('native', 'create-only', 'bootstrap') if accepted else ('native', 'bootstrap')):
                    name = label+'-'+role
                    argv = [binaries['default' if role == 'bootstrap' else 'feature']['path'], 'build',
                            contract['source'], '--root', source, '--name', stem, '--target', 'all', '--json',
                            '--profile', 'data-ownership-v1']
                    argv += ['--node', tools['node']['path']] if role == 'bootstrap' else ['--native-frontend']
                    value = success if accepted else v.c.failure(contract)
                    if role == 'create-only':
                        value = {'version': 1, 'command': 'build', 'manifest': None, 'ok': False, 'results': [],
                            'diagnostics': [{'code': 'ZRYNA-C1009', 'guidance':
                            'use a writable real output filesystem and a fresh artifact stem', 'message':
                            'create-only output bundle already exists', 'primary': {'kind': 'global'}, 'severity': 'error'}]}
                    self.write(self.output/(name+'.stdout'), value)
                    (self.output/(name+'.stderr')).write_bytes(b'')
                    logs = {suffix: {'bytes': (self.output/(name+'.'+suffix)).stat().st_size,
                                      'sha256': v.digest(self.output/(name+'.'+suffix))} for suffix in ('stdout', 'stderr')}
                    self.receipt['commands'].append({'label': name, 'argv': argv, 'cwd': source,
                        'exit': 4 if role == 'create-only' else (0 if accepted else 3), 'timed_out': False, **logs})
            self.save()
            baseline = self.admit()
            self.assertEqual(baseline['counts'], {'passed': 69, 'failed': 0, 'ignored': 0})
            self.assertEqual(len(self.inventory(self.output)), 621)

    def save(self):
        self.write(self.output/'receipt.json', self.receipt)

    def admit(self):
        self.save()
        return v.verify(self.source, self.output, self.head, self.receipt['platform'],
                        run_id='123', run_attempt='1')

    def reject(self):
        with self.assertRaises(ERRORS):
            self.admit()

    def hostile(self, *mutations):
        for platform in ('linux', 'win32'):
            for index, mutation in enumerate(mutations):
                with self.subTest(platform=platform, mutation=index):
                    self.make(platform)  # Positively admit before EVERY mutation.
                    mutation()
                    self.reject()

    def row(self, disposition='accepted'):
        return next(row for row in self.receipt['records'] if row['disposition'] == disposition)

    def change_manifests(self, change, label='m3-candidate-modules-main'):
        for role in ('native', 'bootstrap'):
            bundle = self.output/(label+'-'+role+'-bundle')
            value = v.read(bundle/'zryna-manifest-v3.json')
            change(value)
            value['graph_sha256'] = v.c.m.graph_digest(3, value['sources'], value['edges'])
            self.write(bundle/'zryna-manifest-v3.json', value)
        next(row for row in self.receipt['records'] if row['id'] == label)['files'] = self.inventory(
            self.output/(label+'-native-bundle'))

    def log_change(self, label, change):
        path = self.output/(label+'.stdout')
        value = v.strict(path.read_bytes())
        change(value)
        self.write(path, value)
        next(row for row in self.receipt['commands'] if row['label'] == label)['stdout'] = {
            'bytes': path.stat().st_size, 'sha256': v.digest(path)}

    def negative_change(self, change, label='m3-conditional-root-borrow-exclusions'):
        for role in ('native', 'bootstrap'):
            self.log_change(label+'-'+role, change)

    def test_positive_complete_linux_and_windows(self):
        for platform in ('linux', 'win32'):
            self.make(platform)
            self.assertEqual(self.admit()['counts'], {'passed': 69, 'failed': 0, 'ignored': 0})
            self.assertEqual(len(self.receipt['commands']), 171)
            self.assertEqual(sum(len(c['diagnostics']) for c in self.contracts.values()), 67)
            self.assertEqual(sum(not c['edges'] for c in self.contracts.values()), 68)

    def test_missing_duplicate_and_forged_case_census(self):
        self.hostile(lambda: self.receipt['records'].pop(),
                     lambda: self.receipt['records'].__setitem__(1, self.receipt['records'][0]),
                     lambda: self.row().update(id='m3-forged'),
                     lambda: self.receipt['records'].reverse())

    def test_negative_as_positive_and_false_private_export_promotion(self):
        self.hostile(lambda: self.row('rejected').update(disposition='accepted', create_only=True),
                     lambda: self.row().update(disposition='rejected'),
                     lambda: self.row().update(exports=['invented']),
                     lambda: self.change_manifests(lambda m: m.update(invocation={'export': 'caller'})))

    def test_source_path_and_dependency_hash_coherent_tampering(self):
        self.hostile(lambda: self.row().update(source='tests/m3-fixtures/candidate-modules/math.zry'),
                     lambda: self.change_manifests(lambda m: m['sources'][1].update(sha256='f'*64)),
                     lambda: self.change_manifests(lambda m: (m['sources'].pop(), m['edges'].clear())))

    def test_edge_target_and_alias_even_with_joint_graph_rehash(self):
        self.hostile(lambda: self.change_manifests(lambda m: m['edges'][0].update(target=m['entrypoint'])),
                     lambda: self.change_manifests(lambda m: m['edges'][0].update(local='score')),
                     lambda: self.change_manifests(lambda m: m['edges'][0].update(imported='score')))

    def test_all_ordered_diagnostics_required(self):
        self.hostile(lambda: self.negative_change(lambda j: j['diagnostics'].pop()),
                     lambda: self.negative_change(lambda j: j['diagnostics'].reverse()),
                     lambda: self.negative_change(lambda j: j['diagnostics'].append(j['diagnostics'][0])))

    def test_exact_diagnostic_code_message_guidance_and_severity(self):
        for field, value in (('code', 'ZRYNA-C1009'), ('message', 'generic error'),
                             ('guidance', 'retry'), ('severity', 'warning')):
            self.hostile(lambda field=field, value=value: self.negative_change(
                lambda j: j['diagnostics'][0].update({field: value})))

    def test_exact_diagnostic_spans_and_source_file_indices(self):
        for field, value in (('start', 0), ('end', 0), ('file', 1), ('file', True)):
            self.hostile(lambda field=field, value=value: self.negative_change(
                lambda j: j['diagnostics'][0]['primary']['span'].update({field: value})))
        self.hostile(lambda: self.negative_change(lambda j: j['diagnostics'][0].update(primary={'kind': 'global'})))

    def test_negative_phase_exit_and_no_bundle_authority(self):
        def negative_command():
            return next(c for c in self.receipt['commands'] if c['label'].startswith(self.row('rejected')['id']))
        self.hostile(lambda: negative_command().update(exit=4), lambda: negative_command().update(exit=0),
                     lambda: self.negative_change(lambda j: j.update(ok=True)),
                     lambda: self.row('rejected').update(files={'forged': {'bytes': 1, 'sha256': 'f'*64}}))

    def test_target_profile_invocation_results_and_manifest_type(self):
        for field, value in (('targets', ['javascript']), ('version', True), ('protocol_version', 3),
                             ('profile', 'zryna-control-flow-v1'), ('command', 'run'),
                             ('results', [{}]), ('diagnostics', [{}])):
            self.hostile(lambda field=field, value=value: self.change_manifests(lambda m: m.update({field: value})))

    def test_layout_runtime_and_native_metadata_exact_shape(self):
        self.hostile(lambda: self.change_manifests(lambda m: m['runtime_abi'].update(version=True)),
                     lambda: self.change_manifests(lambda m: m['layouts'].update(linear32_sha256='invalid')),
                     lambda: self.change_manifests(lambda m: m['artifacts'][2]['metadata'].update(format='elf-executable')))

    def test_canonical_manifest_wire_bytes_both_providers(self):
        def change(transform):
            label = 'm3-candidate-modules-main'
            for role in ('native', 'bootstrap'):
                path = self.output/(label+'-'+role+'-bundle/zryna-manifest-v3.json')
                path.write_bytes(transform(path.read_bytes()))
            next(r for r in self.receipt['records'] if r['id'] == label)['files'] = self.inventory(
                self.output/(label+'-native-bundle'))
        for transform in (lambda b: b.replace(b'\n', b'\r\n'), lambda b: b+b'\n',
                          lambda b: json.dumps(v.strict(b), sort_keys=True, indent=2).encode()+b'\n'):
            self.hostile(lambda transform=transform: change(transform))

    def test_every_bundle_byte_matches_after_coherent_manifest_rehash(self):
        def mutate():
            label = 'm3-candidate-modules-main'
            bundle = self.output/(label+'-bootstrap-bundle')
            path = bundle/('javascript/corpus-build-'+label+'.mjs')
            path.write_bytes(path.read_bytes()+b'// divergent\n')
            value = v.read(bundle/'zryna-manifest-v3.json')
            value['artifacts'][0].update(bytes=path.stat().st_size, sha256=v.digest(path))
            self.write(bundle/'zryna-manifest-v3.json', value)
        self.hostile(mutate)

    def test_create_only_full_diagnostic_and_exact_exit(self):
        def collision():
            return next(c for c in self.receipt['commands'] if c['label'].endswith('-create-only'))
        self.hostile(lambda: self.log_change(collision()['label'], lambda j: j['diagnostics'][0].update(guidance='retry')),
                     lambda: collision().update(exit=True), lambda: collision().update(exit=0))

    def test_success_envelope_requires_exact_manifest_path(self):
        self.hostile(lambda: [self.log_change('m3-candidate-modules-main-'+role,
                                              lambda j: j.update(manifest='outside.json'))
                              for role in ('native', 'bootstrap')])

    def test_provenance_argv_binary_source_target_cwd_and_run(self):
        for mutation in (lambda c: c['argv'].__setitem__(0, '/different-cli'),
                         lambda c: c['argv'].__setitem__(2, 'different.zry'),
                         lambda c: c['argv'].__setitem__(8, 'javascript'),
                         lambda c: c['argv'].append('--node'), lambda c: c.update(cwd='/other-root')):
            self.hostile(lambda mutation=mutation: mutation(self.receipt['commands'][0]))
        self.hostile(lambda: self.receipt.update(run_attempt='2'), lambda: self.receipt.update(head='0'*40))

    def test_all_or_nothing_and_no_ignored_failed_or_runtime_claim(self):
        self.hostile(lambda: self.receipt.update(status='failed'), lambda: self.receipt['counts'].update(passed=True),
                     lambda: self.receipt['counts'].update(ignored=1), lambda: self.row().update(status='failed'),
                     lambda: self.receipt['scope'].update(runtime_execution=True),
                     lambda: self.receipt.update(public_activation=True),
                     lambda: self.receipt['scope'].update(allocation_count=0))

    def test_command_census_roles_timeout_and_lossless_logs(self):
        def alter_log():
            path = self.output/(self.receipt['commands'][0]['label']+'.stderr')
            path.write_bytes(b'failure evidence')
            self.receipt['commands'][0]['stderr'] = {'bytes': path.stat().st_size, 'sha256': v.digest(path)}
        self.hostile(lambda: self.receipt['commands'].pop(), lambda: self.receipt['commands'].reverse(),
                     lambda: self.receipt['commands'][0].update(timed_out=True), alter_log,
                     lambda: self.receipt['commands'][0]['stdout'].update(sha256='f'*64))

    def test_exact_closed_file_directory_census_and_empty_runtime_path(self):
        self.hostile(lambda: (self.output/'extra').write_bytes(b'extra'),
                     lambda: (self.output/'empty-runtime-path/extra').write_bytes(b'extra'),
                     lambda: (self.output/'unused-directory').mkdir(),
                     lambda: (self.output/(self.receipt['commands'][0]['label']+'.stdout')).unlink())

    def test_checkpoint_labels(self):
        self.hostile(lambda: self.row().update(initial_output_absent=False),
                     lambda: self.row().update(final_output_absent=False),
                     lambda: self.row().update(provider_json_exact=False),
                     lambda: self.row().update(node_on_path=True), lambda: self.row().update(pnpm_on_path=True))

    def test_mocked_link_and_windows_reparse_paths_no_host_skip(self):
        original = Path.lstat
        for platform in ('linux', 'win32'):
            for attributes in ({'st_file_attributes': 0x400}, {'st_mode': 0o120777}):
                self.make(platform)
                selected = self.output/'m3-candidate-modules-main-native-bundle'
                def hostile(path):
                    value = original(path)
                    if path == selected:
                        return SimpleNamespace(**({'st_mode': value.st_mode, 'st_size': value.st_size}|attributes))
                    return value
                with patch.object(Path, 'lstat', hostile):
                    self.reject()

    def test_preceding_cli_build_feature_is_independently_bound(self):
        def mutate():
            path = self.output/'cli-build/ci-receipt.json'
            prior = v.read(path)
            prior['commands'][4]['argv'] = prior['commands'][3]['argv']
            self.write(path, prior)
            self.receipt['cli_receipt_sha256'] = v.digest(path)
            self.receipt['cli_build_files'] = self.inventory(self.output/'cli-build')
        self.hostile(mutate)

    def test_hidden_source_or_registry_corruption_rejects_git_blob_mismatch(self):
        for platform in ('linux', 'win32'):
            for name in (v.c.REGISTRY, 'tests/m3-fixtures/shared-root-borrow.zry'):
                self.make(platform)
                path = self.source/name
                original = path.read_bytes()
                subprocess.run(['git', '-C', str(self.source), 'update-index', '--assume-unchanged', name], check=True)
                try:
                    path.write_bytes(original+b' ')
                    self.assertEqual(v.v.git(self.source, 'status', '--porcelain', '--untracked-files=all'), '')
                    self.reject()
                finally:
                    path.write_bytes(original)
                    subprocess.run(['git', '-C', str(self.source), 'update-index', '--no-assume-unchanged', name], check=True)

    def test_strict_json_duplicate_float_nonfinite(self):
        for platform in ('linux', 'win32'):
            for data in (b'{"version":1,"version":1}', b'{"version":1.0}', b'{"version":NaN}'):
                self.make(platform)
                path = self.output/(self.receipt['commands'][0]['label']+'.stdout')
                path.write_bytes(data)
                self.receipt['commands'][0]['stdout'] = {'bytes': len(data), 'sha256': v.digest(path)}
                self.reject()


if __name__ == '__main__':
    unittest.main()
