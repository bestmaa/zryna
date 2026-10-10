"""String-only hostile reader controls; synthetic bundles are not execution proof."""
import copy
import importlib.util
from pathlib import Path
import tempfile
import unittest

spec = importlib.util.spec_from_file_location(
    'physical_control_helpers', Path(__file__).with_name('verify_test.py'))
helpers = importlib.util.module_from_spec(spec)
spec.loader.exec_module(helpers)
v = helpers.v
ROOT = Path(__file__).resolve().parents[3]
PROBES = {
    'string-physical-2': (2, 'string-fault-2-1'),
    'string-physical-4': (4, 'string-fault-2-2'),
}


class StringControls(unittest.TestCase):
    # Reuse the existing neutral frame operations, without inheriting or running
    # the owned-shared test methods or its hard-coded fixture constructor.
    write = helpers.PhysicalControls.write
    admit = helpers.PhysicalControls.admit
    mutate = helpers.PhysicalControls.mutate
    reject = helpers.PhysicalControls.reject

    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.output = Path(self.temp.name)
        self.fixtures, self.cases = v.authority(ROOT, group='string')

    def create(self, case='string-physical-4'):
        row, injected = self.cases[case]
        sources, edges = v.source_contract(ROOT, self.fixtures, 'string')
        expected = v.expected_result(row, injected, 'native')
        ordinal, oracle = PROBES[case] if injected else (None, None)
        fault = ({'mode': 'physical-allocation', 'code': 6, 'ordinal': ordinal,
                  'command': 0x26000000 | ordinal} if injected else None)
        for provider in ('native-retained', 'bootstrap'):
            directory = self.output / f'native-{case}-{provider}'
            (directory / 'sources').mkdir(parents=True)
            for source in sources:
                fixture = 'string' if source['id'] == 0 else 'string-body'
                original = ROOT / self.fixtures[fixture]['path']
                (directory / 'sources' / source['path']).write_bytes(original.read_bytes())
            observation = {
                'case': case, 'fixture': 'string', 'injected': injected,
                'provider': provider, 'target': 'native', 'platform': 'linux',
                'registry_sha256': v.REGISTRY_SHA, 'fault': fault,
                'trace_oracle': oracle, 'results': copy.deepcopy(expected),
                'sources': {source['path']: source['sha256'] for source in sources},
                'native_first': True,
                'execution_checkpoints': 3 if provider == 'native-retained' else 0,
                'publication_checkpoints': (['Native', 'Manifest', 'Commit']
                                           if provider == 'native-retained' else []),
                'cleanup_entries': 0, 'zero_live_finalization': True,
                'physical_allocation_release_counts': None,
                'public_activation': False, 'installed_no_cargo_acceptance': False,
            }
            bundle = directory / 'bundle'
            (bundle / 'native').mkdir(parents=True)
            artifact = bundle / 'native/fixed-oracle.elf'
            # Minimal declared-format header, deliberately not a compiler product.
            artifact.write_bytes(b'\x7fELF\x02\x01\x01' + b'\0' * 9 + b'\x02\0')
            manifest = {
                'version': 3, 'profile': 'zryna-data-ownership-v1',
                'protocol_version': 4, 'command': 'run', 'entrypoint': 'main.zry',
                'graph_sha256': v.graph_digest(3, sources, edges),
                'sources': sources, 'edges': edges,
                'layouts': {name: 'a' * 64 for name in v.cold.LAYOUTS},
                'runtime_abi': v.cold.RUNTIME, 'stem': 'fixed-oracle',
                'targets': ['native'], 'invocation': {'export': 'score', 'arguments': []},
                'results': copy.deepcopy(expected), 'diagnostics': [],
                'artifacts': [{
                    'target': 'native', 'kind': 'linux-x86-64-invocation-executable',
                    'filename': 'fixed-oracle.elf', 'bytes': artifact.stat().st_size,
                    'sha256': v.digest(artifact), 'metadata': {
                        'profile': 'linux-x86-64-elf', 'triple': 'x86_64-unknown-linux-gnu',
                        'format': 'elf-executable', 'scalar_abi': 'v1',
                        'program_object_sha256': 'a' * 64,
                        'runtime_object_sha256': 'b' * 64, 'harness_sha256': 'c' * 64,
                    },
                }],
            }
            self.write(directory / 'observation.json', observation)
            self.write(bundle / 'zryna-manifest-v3.json', manifest)
        return case, row, injected

    def test_exact_string_census_and_three_accepted_frames(self):
        self.assertEqual(set(self.cases), {*PROBES, 'positive-string'})
        for case, (ordinal, oracle) in PROBES.items():
            self.assertEqual(v.oracle_for(case), oracle)
            self.assertEqual(v.fault_for(case), {
                'mode': 'physical-allocation', 'code': 6,
                'ordinal': ordinal, 'command': 0x26000000 | ordinal,
            })
        for case in self.cases:
            self.admit(self.create(case))

    def test_ordinal_two_requires_omitted_empty_trace(self):
        values = self.create('string-physical-2')
        self.assertNotIn('trace', v.expected_result(values[1], True, 'native')[0])
        self.admit(values)

        def change(observation, manifest, directory):
            observation['results'][0]['trace'] = []
            manifest['results'] = copy.deepcopy(observation['results'])

        self.mutate(values, change)
        self.reject(values)

    def test_ordinal_two_rejects_invented_cleanup(self):
        values = self.create('string-physical-2')

        def change(observation, manifest, directory):
            observation['results'][0]['trace'] = copy.deepcopy(
                self.cases['string-physical-4'][0]['trace'])
            manifest['results'] = copy.deepcopy(observation['results'])

        self.mutate(values, change)
        self.reject(values)

    def test_logical_fault_cannot_substitute_for_physical(self):
        values = self.create('string-physical-2')
        self.mutate(values, lambda o, m, p: o.update(fault={
            'mode': 'logical', 'code': 2, 'ordinal': 1, 'command': 0x22000001}))
        self.reject(values)

    def test_coherent_wrong_physical_ordinal_or_case(self):
        for field, value in [('fault', {'mode': 'physical-allocation', 'code': 6,
                                       'ordinal': 2, 'command': 0x26000002}),
                             ('case', 'string-physical-2')]:
            with tempfile.TemporaryDirectory() as temp:
                self.output = Path(temp)
                values = self.create('string-physical-4')
                self.mutate(values, lambda o, m, p: o.update({field: value}))
                self.reject(values)

    def test_coherent_wrong_cleanup_oracle(self):
        values = self.create('string-physical-4')
        self.mutate(values, lambda o, m, p: o.update(trace_oracle='string-fault-2-1'))
        self.reject(values)

    def test_coherent_wrong_trap(self):
        values = self.create()

        def change(observation, manifest, directory):
            observation['results'][0]['outcome']['code'] = 'zryna.trap.capacity-v1'
            manifest['results'] = copy.deepcopy(observation['results'])

        self.mutate(values, change)
        self.reject(values)

    def test_coherent_reordered_cleanup_trace(self):
        values = self.create()

        def change(observation, manifest, directory):
            observation['results'][0]['trace'].reverse()
            manifest['results'] = copy.deepcopy(observation['results'])

        self.mutate(values, change)
        self.reject(values)

    def test_coherent_source_and_graph_substitution(self):
        values = self.create()

        def change(observation, manifest, directory):
            body = directory / 'sources/math.zry'
            body.write_bytes(b'export function text(): i32 { return 17; }\n')
            observation['sources']['math.zry'] = v.digest(body)
            manifest['sources'][1]['sha256'] = v.digest(body)
            manifest['graph_sha256'] = v.graph_digest(
                3, manifest['sources'], manifest['edges'])

        self.mutate(values, change)
        self.reject(values)
        self.output = Path(self.temp.name) / 'graph-only'
        self.output.mkdir()
        values = self.create()

        def remove_edge(observation, manifest, directory):
            manifest['edges'] = []
            manifest['graph_sha256'] = v.graph_digest(3, manifest['sources'], [])

        self.mutate(values, remove_edge)
        self.reject(values)

    def test_counts_cleanup_and_checkpoint_claims_reject(self):
        mutations = [
            ('physical_allocation_release_counts', {'allocated': 4, 'released': 4}),
            ('zero_live_finalization', False), ('cleanup_entries', 1),
            ('cleanup_entries', False), ('execution_checkpoints', 2),
            ('publication_checkpoints', ['Manifest', 'Native', 'Commit']),
            ('native_first', False), ('public_activation', True),
        ]
        for field, value in mutations:
            with tempfile.TemporaryDirectory() as temp:
                self.output = Path(temp)
                values = self.create()
                self.mutate(values, lambda o, m, p: o.update({field: value}))
                self.reject(values)

    def test_complete_provider_bundle_divergence_rejects(self):
        values = self.create()
        bundle = self.output / f'native-{values[0]}-bootstrap/bundle'
        artifact = bundle / 'native/fixed-oracle.elf'
        artifact.write_bytes(artifact.read_bytes() + b'changed')
        manifest = v.cold.read(bundle / 'zryna-manifest-v3.json')
        manifest['artifacts'][0].update(
            bytes=artifact.stat().st_size, sha256=v.digest(artifact))
        self.write(bundle / 'zryna-manifest-v3.json', manifest)
        self.reject(values)

    def test_positive_requires_seventeen_and_no_fault(self):
        for field in ('returned', 'fault'):
            with tempfile.TemporaryDirectory() as temp:
                self.output = Path(temp)
                values = self.create('positive-string')

                def change(observation, manifest, directory):
                    if field == 'returned':
                        observation['results'][0]['outcome']['value']['value'] = 43
                        manifest['results'] = copy.deepcopy(observation['results'])
                    else:
                        observation['fault'] = {'mode': 'physical-allocation', 'code': 6,
                                                'ordinal': 2, 'command': 0x26000002}

                self.mutate(values, change)
                self.reject(values)


if __name__ == '__main__':
    unittest.main()
