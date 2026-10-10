"""Host-independent malformed evidence attacks; no compiler/provider executions."""
import copy
import importlib.util
import json
from pathlib import Path
import subprocess
import stat
import sys
from types import SimpleNamespace
import unittest
from unittest.mock import patch

sys.dont_write_bytecode = True
spec = importlib.util.spec_from_file_location('m2_root_control_model', Path(__file__).with_name('model.py'))
model = importlib.util.module_from_spec(spec)
spec.loader.exec_module(model)
v = model.v
ERRORS = (AssertionError, ValueError, KeyError, TypeError, OSError)


class M2RootControls(unittest.TestCase):
    def setUp(self):
        self.model = model.Model()
        self.addCleanup(self.model.close)

    def admit(self):
        self.model.save()
        return v.verify(self.model.source, self.model.output, self.model.head,
                        self.model.receipt['platform'], run_id='123', run_attempt='1')

    def attack(self, change):
        for platform in ('linux', 'win32'):
            with self.subTest(platform=platform):
                self.model.make(platform)
                change()
                with self.assertRaises(ERRORS):
                    self.admit()

    def raw(self, suffix, raw, index=1):
        command = self.model.receipt['commands'][index]
        path = self.model.output/(command['label']+'.'+suffix)
        path.write_bytes(raw)
        command[suffix] = {'bytes': len(raw), 'sha256': v.m.digest(path)}

    def manifests(self, change):
        label = next(iter(v.c.CASES))
        record = self.model.receipt['records'][0]
        for role in ('native', 'bootstrap'):
            bundle = self.model.output/(label+'-'+role+'-bundle')
            path = bundle/'zryna-manifest-v2.json'
            manifest = v.m.read(path)
            change(manifest)
            self.model.write(path, manifest)
            found = self.model.inventory(bundle)
            for key in ('files', 'before_collision_inventory', 'after_collision_inventory'):
                record[key] = copy.deepcopy(found)

    def test_valid_linux_and_windows(self):
        for platform in ('linux', 'win32'):
            self.model.make(platform)
            self.assertEqual(self.admit()['counts'], {'passed': 3, 'failed': 0, 'ignored': 0})

    def test_wrong_native_binary(self):
        self.attack(lambda: self.model.receipt['commands'][0]['argv'].__setitem__(0,
                    self.model.receipt['binaries']['default']['path']))

    def test_wrong_source(self):
        self.attack(lambda: self.model.receipt['commands'][0]['argv'].__setitem__(2,
                    'tests/m2-fixtures/valid/main.zry'))

    def test_missing_native_selector(self):
        self.attack(lambda: self.model.receipt['commands'][0]['argv'].pop())

    def test_wrong_cwd(self):
        self.attack(lambda: self.model.receipt['commands'][0].update(cwd='/other-source'))

    def test_wrong_command_environment(self):
        self.attack(lambda: self.model.receipt['commands'][0]['selected_environment'].update(PATH='/usr/bin'))

    def test_wrong_metadata_target(self):
        self.attack(lambda: self.model.receipt.update(metadata_target=self.model.receipt['root']))

    def test_node_on_path(self):
        self.attack(lambda: (self.model.output/'empty-runtime-path/node').write_bytes(b'modeled node'))

    def test_boolean_native_exit(self):
        self.attack(lambda: self.model.receipt['commands'][0].update(exit=False))

    def test_wrong_collision_exit(self):
        self.attack(lambda: self.model.receipt['commands'][1].update(exit=1))

    def test_wrong_collision_span(self):
        def change():
            raw = v.m.strict((self.model.output/(self.model.receipt['commands'][1]['label']+'.stdout')).read_bytes())
            raw['diagnostics'][0]['primary'] = {'kind': 'source', 'file': 'wrong.zry'}
            self.raw('stdout', json.dumps(raw).encode())
        self.attack(change)

    def test_duplicate_json_keys(self):
        def change():
            command = self.model.receipt['commands'][1]
            raw = (self.model.output/(command['label']+'.stdout')).read_bytes()
            self.raw('stdout', raw.replace(b'"ok": false', b'"ok": false, "ok": false'))
        self.attack(change)

    def test_nonempty_stderr(self):
        self.attack(lambda: self.raw('stderr', b'unexpected modeled warning\n'))

    def test_coherent_graph_mutation(self):
        self.attack(lambda: self.manifests(lambda m: m.update(graph_sha256='0'*64)))

    def test_coherent_source_mutation(self):
        self.attack(lambda: self.manifests(lambda m: m['sources'][0].update(sha256='0'*64)))

    def test_coherent_source_alias(self):
        self.attack(lambda: self.manifests(lambda m: m['sources'][0].update(path='tests/m2-fixtures/valid/main.zry')))

    def test_boolean_manifest_version(self):
        self.attack(lambda: self.manifests(lambda m: m.update(version=True)))

    def test_noncanonical_manifest_bytes(self):
        def change():
            label = next(iter(v.c.CASES))
            record = self.model.receipt['records'][0]
            for role in ('native', 'bootstrap'):
                bundle = self.model.output/(label+'-'+role+'-bundle')
                path = bundle/'zryna-manifest-v2.json'
                path.write_bytes(json.dumps(v.m.read(path), sort_keys=True).encode())
                found = self.model.inventory(bundle)
                for key in ('files', 'before_collision_inventory', 'after_collision_inventory'):
                    record[key] = copy.deepcopy(found)
        self.attack(change)

    def test_missing_bundle_file(self):
        self.attack(lambda: next((self.model.output/(next(iter(v.c.CASES))+'-native-bundle')/'native').iterdir()).unlink())

    def test_unlisted_proof_file(self):
        self.attack(lambda: (self.model.output/'extra.bin').write_bytes(b'extra'))

    def test_unlisted_empty_directory(self):
        self.attack(lambda: (self.model.output/'extra-directory').mkdir())

    def test_linked_proof_file(self):
        self.metadata_attack(True)

    def test_reparse_proof_file(self):
        self.metadata_attack(False)

    def metadata_attack(self, linked):
        # Deterministic no-follow controls also run without Windows symlink privileges.
        for platform in ('linux', 'win32'):
            with self.subTest(platform=platform):
                self.model.make(platform)
                target = self.model.output/'receipt.json'
                original = Path.lstat

                def metadata(path, *args, **kwargs):
                    found = original(path, *args, **kwargs)
                    if path == target:
                        return SimpleNamespace(st_mode=(stat.S_IFLNK | 0o600) if linked else found.st_mode,
                                               st_size=found.st_size, st_file_attributes=0 if linked else 0x400)
                    return found

                with patch.object(Path, 'lstat', metadata), self.assertRaises(ERRORS):
                    self.admit()

    def test_divergent_provider_bytes(self):
        def change():
            label = next(iter(v.c.CASES))
            path = next((self.model.output/(label+'-bootstrap-bundle')/'javascript').iterdir())
            with path.open('ab') as stream:
                stream.write(b'\n// changed provider\n')
        self.attack(change)

    def test_changed_collision_inventory(self):
        self.attack(lambda: self.model.receipt['records'][0]['after_collision_inventory']['zryna-manifest-v2.json'].update(bytes=1))

    def test_wrong_head(self):
        self.attack(lambda: self.model.receipt.update(head='f'*40))

    def test_wrong_hosted_run(self):
        self.attack(lambda: self.model.receipt.update(run_id='456'))

    def test_omitted_case(self):
        self.attack(lambda: self.model.receipt['records'].pop())

    def test_duplicate_case(self):
        self.attack(lambda: self.model.receipt['records'].__setitem__(1, self.model.receipt['records'][0]))

    def test_wrong_cli_source_authority(self):
        def change():
            path = self.model.output/'cli-build/ci-receipt.json'
            cli = v.m.read(path)
            cli['head'] = 'f'*40
            self.model.write(path, cli)
            self.model.receipt['cli_receipt_sha256'] = v.m.digest(path)
            self.model.receipt['cli_build_files'] = self.model.inventory(self.model.output/'cli-build')
        self.attack(change)

    def test_dirty_working_bytes_hidden_by_git(self):
        path = next(iter(v.c.CASES.values()))
        subprocess.run(['git', '-C', str(self.model.source), 'update-index', '--assume-unchanged', path], check=True)
        with (self.model.source/path).open('ab') as stream:
            stream.write(b'\n// changed source\n')
        self.assertEqual(v.b.git(self.model.source, 'status', '--porcelain'), '')
        with self.assertRaises(ERRORS):
            self.admit()


if __name__ == '__main__':
    unittest.main(verbosity=2)
