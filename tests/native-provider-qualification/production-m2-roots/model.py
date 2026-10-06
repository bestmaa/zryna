"""Valid modeled proof packets for evidence attacks; never compiler/provider execution."""
import copy
import importlib.util
from pathlib import Path
import shutil
import subprocess
import sys

sys.dont_write_bytecode = True


def load(name, path):
    spec = importlib.util.spec_from_file_location(name, path)
    result = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(result)
    return result


v = load('m2_modeled_reader', Path(__file__).with_name('verify.py'))
seven = load('m2_existing_cli_model', Path(__file__).parent.parent/'production-manifests/verify_test.py')
ROOT = Path(__file__).resolve().parents[3]


class Model:
    def __init__(self, platform='linux'):
        # Reuse the unchanged CLI-build receipt model; each instance owns its temporary source.
        self.owner = type('OwnedSyntheticSource', (seven.ManifestControls,), {})
        self.owner.setUpClass()
        source = self.owner.source
        for name in (v.c.REGISTRY, *v.c.CASES.values()):
            path = source/name
            path.parent.mkdir(parents=True, exist_ok=True)
            shutil.copy2(ROOT/name, path)
        subprocess.run(['git', '-C', str(source), 'add', '.'], check=True, capture_output=True)
        subprocess.run(['git', '-C', str(source), '-c', 'user.name=synthetic-control',
                        '-c', 'user.email=synthetic@example.invalid', 'commit', '-qm',
                        'synthetic frozen M2 sources'], check=True, capture_output=True)
        self.owner.head = v.b.git(source, 'rev-parse', 'HEAD')
        self.owner.tree = v.b.git(source, 'rev-parse', 'HEAD^{tree}')
        self.owner.inputs = v.b.committed_inputs(source, self.owner.head)
        self.factory = self.owner()
        self.factory.setUp()
        self.source, self.head, self.tree = source, self.owner.head, self.owner.tree
        self.inputs, self.output = self.owner.inputs, self.factory.output
        self.make(platform)

    def close(self):
        self.factory.doCleanups()
        self.owner.tearDownClass()

    def write(self, path, value):
        path.write_bytes(v.m.canonical(value))

    def inventory(self, path):
        return self.factory.inventory(path)

    def save(self):
        self.write(self.output/'receipt.json', self.receipt)

    def make(self, platform):
        self.factory.make(platform)
        self.receipt = copy.deepcopy(self.factory.receipt)
        # Remove only this instance's seven-case modeled leaves; retain its admitted CLI model.
        for path in self.output.iterdir():
            if path.name in ('cli-build', 'empty-runtime-path'):
                continue
            if path.is_dir():
                shutil.rmtree(path)
            else:
                path.unlink()
        self.receipt.update(format='zryna.production-m2-roots.v1', generated_inputs={},
                            scope=copy.deepcopy(v.c.SCOPE), counts={'passed': 3, 'failed': 0, 'ignored': 0},
                            commands=[], records=[], removed_node_environment_present=False)
        output = v.b.absolute(self.receipt['output'], platform)
        self.receipt['metadata_target'] = str(output.parent/(output.name+'-metadata-target'))
        selected = v.environment(self.receipt, platform)
        self.receipt['selected_environment'] = selected
        self.contracts = v.c.contracts(self.source, self.inputs)
        for label, contract in self.contracts.items():
            stem = v.c.stem(label)
            success = {'version': 1, 'command': 'build', 'diagnostics': [], 'results': [], 'ok': True,
                       'manifest': f'.zryna/out/{stem}.build/zryna-manifest-v2.json'}
            for role in ('native', 'bootstrap'):
                bundle = self.output/(label+'-'+role+'-bundle')
                for target, extension in zip(v.m.TARGETS, v.m.EXTENSIONS):
                    directory = bundle/target
                    directory.mkdir(parents=True)
                    data = b'export function value() { return 1; }\n'
                    if target == 'webassembly':
                        data = b'\0asm\x01\0\0\0'
                    elif target == 'native':
                        elf = bytearray(64)
                        elf[:7] = b'\x7fELF\x02\x01\x01'
                        elf[16:24] = b'\x01\0\x3e\0\x01\0\0\0'
                        elf[52:54] = b'\x40\0'
                        data = bytes(elf)
                    (directory/(stem+'.'+extension)).write_bytes(data)
                self.write(bundle/'zryna-manifest-v2.json',
                           v.c.expected_manifest(label, contract, self.inventory(bundle)))
            inventory = self.inventory(self.output/(label+'-native-bundle'))
            self.receipt['records'].append({'id': label, 'status': 'passed', 'source': contract['source'],
                'profile': 'control-flow-v1', 'files': inventory, 'initial_output_absent': True,
                'final_output_absent': True, 'success_json_exact': True, 'manifest_bytes_exact': True,
                'create_only': True, 'node_on_path': False, 'pnpm_on_path': False,
                'before_collision_inventory': copy.deepcopy(inventory),
                'after_collision_inventory': copy.deepcopy(inventory)})
            for role in v.ROLES:
                name = label+'-'+role
                argv = [self.receipt['binaries']['default' if role == 'bootstrap' else 'feature']['path'],
                        'build', contract['source'], '--root', self.receipt['root'], '--name', stem,
                        '--target', 'all', '--json', '--profile', 'control-flow-v1']
                argv += ['--node', self.receipt['tools']['node']['path']] if role == 'bootstrap' else ['--native-frontend']
                value = success if role != 'create-only' else {
                    'version': 1, 'command': 'build', 'manifest': None, 'ok': False, 'results': [],
                    'diagnostics': [{'code': 'ZRYNA-C1009', 'guidance':
                        'use a writable real output filesystem and a fresh artifact stem',
                        'message': 'create-only output bundle already exists',
                        'primary': {'kind': 'global'}, 'severity': 'error'}]}
                self.write(self.output/(name+'.stdout'), value)
                (self.output/(name+'.stderr')).write_bytes(b'')
                logs = {suffix: {'bytes': (self.output/(name+'.'+suffix)).stat().st_size,
                                  'sha256': v.m.digest(self.output/(name+'.'+suffix))}
                        for suffix in ('stdout', 'stderr')}
                self.receipt['commands'].append({'label': name, 'argv': argv, 'cwd': self.receipt['root'],
                    'exit': 4 if role == 'create-only' else 0, 'timed_out': False,
                    'selected_environment': copy.deepcopy(selected), **logs})
        self.save()
        result = v.verify(self.source, self.output, self.head, platform, run_id='123', run_attempt='1')
        assert result['counts'] == {'passed': 3, 'failed': 0, 'ignored': 0}
