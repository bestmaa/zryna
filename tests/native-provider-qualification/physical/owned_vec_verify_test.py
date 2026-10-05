"""OwnedVec-only hostile reader controls; synthetic bundles are not execution proof."""
import copy
import importlib.util
import json
from pathlib import Path
import tempfile
import unittest

spec = importlib.util.spec_from_file_location(
    'owned_vec_physical_verify', Path(__file__).with_name('verify.py'))
v = importlib.util.module_from_spec(spec)
spec.loader.exec_module(v)
ROOT = Path(__file__).resolve().parents[3]
CASE = 'owned-vec-physical-10'
POSITIVE = 'positive-owned-vec'
ORACLE = 'owned-vec-fault-2-6'
FAULT = {'mode': 'physical-allocation', 'code': 6, 'ordinal': 10,
         'command': 0x2600000a}
TRACE = [
    {'kind': 'drop', 'value': 'sequence'},
    {'kind': 'drop', 'value': 'string'},
    {'kind': 'cleanup', 'module': 1, 'function': 0, 'place': 3},
    {'kind': 'drop', 'value': 'sequence'},
    {'kind': 'drop', 'value': 'string'},
    {'kind': 'drop', 'value': 'string'},
]


class OwnedVecControls(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.output = Path(self.temp.name)
        self.fixtures, self.cases = v.authority(ROOT, group='owned-vec')

    def write(self, path, value):
        path.write_text(json.dumps(value))

    def admit(self, values):
        case, row, injected = values
        v.pair(ROOT, self.output, self.fixtures, case, row, injected, 'native', 'linux')

    def mutate(self, values, change):
        for provider in ('native-retained', 'bootstrap'):
            directory = self.output / f'native-{values[0]}-{provider}'
            observation = v.cold.read(directory / 'observation.json')
            manifest = v.cold.read(directory / 'bundle/zryna-manifest-v3.json')
            change(observation, manifest, directory)
            self.write(directory / 'observation.json', observation)
            self.write(directory / 'bundle/zryna-manifest-v3.json', manifest)

    def reject(self, values):
        with self.assertRaises((AssertionError, ValueError, KeyError, OSError, TypeError)):
            self.admit(values)

    def create(self, case=CASE):
        row, injected = self.cases[case]
        sources, edges = v.source_contract(ROOT, self.fixtures, 'owned-vec')
        expected = v.expected_result(row, injected, 'native')
        for provider in ('native-retained', 'bootstrap'):
            directory = self.output / f'native-{case}-{provider}'
            (directory / 'sources').mkdir(parents=True)
            for source in sources:
                fixture = 'owned-vec' if source['id'] == 0 else 'owned-vec-body'
                original = ROOT / self.fixtures[fixture]['path']
                (directory / 'sources' / source['path']).write_bytes(original.read_bytes())
            observation = {
                'case': case, 'fixture': 'owned-vec', 'injected': injected,
                'provider': provider, 'target': 'native', 'platform': 'linux',
                'registry_sha256': v.REGISTRY_SHA,
                'fault': copy.deepcopy(FAULT) if injected else None,
                'trace_oracle': ORACLE if injected else None,
                'results': copy.deepcopy(expected),
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
            # Declared-format header only, deliberately not a compiler product.
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

    def test_exact_owned_vec_census_and_two_accepted_frames(self):
        self.assertEqual(set(self.cases), {CASE, POSITIVE})
        self.assertEqual(v.oracle_for(CASE), ORACLE)
        self.assertEqual(v.fault_for(CASE), FAULT)
        self.assertEqual(self.cases[CASE][0]['fault'], {'code': 2, 'ordinal': 6})
        self.assertEqual(self.cases[CASE][0]['trace'], TRACE)
        self.assertEqual(self.cases[POSITIVE][0]['expected'], 41)
        for case in self.cases:
            self.admit(self.create(case))

    def test_logical_fault_cannot_substitute_for_physical(self):
        values = self.create()
        self.mutate(values, lambda o, m, p: o.update(fault={
            'mode': 'logical', 'code': 2, 'ordinal': 6, 'command': 0x22000006}))
        self.reject(values)

    def test_coherent_wrong_physical_selector_rejects(self):
        for field, value in [('ordinal', 6), ('command', 0x26000006),
                             ('code', 2), ('mode', 'logical'), ('ordinal', True)]:
            with tempfile.TemporaryDirectory() as temp:
                self.output = Path(temp)
                values = self.create()
                fault = copy.deepcopy(FAULT)
                fault[field] = value
                if field == 'ordinal' and type(value) is int:
                    fault['command'] = 0x26000000 | value
                self.mutate(values, lambda o, m, p: o.update(fault=fault))
                self.reject(values)

    def test_wrong_case_fixture_and_trace_oracle_reject(self):
        for field, value in [('case', 'owned-vec-fault-2-6'),
                             ('fixture', 'owned-shared'),
                             ('trace_oracle', 'owned-vec-fault-2-5')]:
            with tempfile.TemporaryDirectory() as temp:
                self.output = Path(temp)
                values = self.create()
                self.mutate(values, lambda o, m, p: o.update({field: value}))
                self.reject(values)

    def test_ordered_cleanup_trace_preserves_repeated_drops(self):
        traces = [list(reversed(TRACE)), TRACE[:-1], TRACE + [TRACE[-1]],
                  [TRACE[0], TRACE[2], TRACE[1], *TRACE[3:]]]
        for trace in traces:
            with tempfile.TemporaryDirectory() as temp:
                self.output = Path(temp)
                values = self.create()

                def change(observation, manifest, directory):
                    observation['results'][0]['trace'] = copy.deepcopy(trace)
                    manifest['results'] = copy.deepcopy(observation['results'])

                self.mutate(values, change)
                self.reject(values)

    def test_typed_allocation_outcome_and_cleanup_place_reject(self):
        for field in ('wrong-trap', 'wrong-kind', 'extra-value', 'bool-module'):
            with tempfile.TemporaryDirectory() as temp:
                self.output = Path(temp)
                values = self.create()

                def change(observation, manifest, directory):
                    result = observation['results'][0]
                    if field == 'wrong-trap':
                        result['outcome']['code'] = 'zryna.trap.refcount-v1'
                    elif field == 'wrong-kind':
                        result['outcome']['kind'] = 'returned'
                    elif field == 'extra-value':
                        result['outcome']['value'] = None
                    else:
                        result['trace'][2]['module'] = True
                    manifest['results'] = copy.deepcopy(observation['results'])

                self.mutate(values, change)
                self.reject(values)

    def test_coherent_source_hash_and_graph_substitution_reject(self):
        values = self.create()

        def change(observation, manifest, directory):
            body = directory / 'sources/math.zry'
            body.write_bytes(b'export function aggregate(): i32 { return 41; }\n')
            observation['sources']['math.zry'] = v.digest(body)
            manifest['sources'][1]['sha256'] = v.digest(body)
            manifest['graph_sha256'] = v.graph_digest(3, manifest['sources'], manifest['edges'])

        self.mutate(values, change)
        self.reject(values)

    def test_graph_edge_target_and_binding_substitution_reject(self):
        for field, value in [('target', 'main.zry'), ('local', 'changed')]:
            with tempfile.TemporaryDirectory() as temp:
                self.output = Path(temp)
                values = self.create()

                def change(observation, manifest, directory):
                    manifest['edges'][0][field] = value
                    manifest['graph_sha256'] = v.graph_digest(
                        3, manifest['sources'], manifest['edges'])

                self.mutate(values, change)
                self.reject(values)

    def test_null_counts_zero_live_and_checkpoint_claims_reject(self):
        mutations = [
            ('physical_allocation_release_counts', {'allocated': 10, 'released': 10}),
            ('zero_live_finalization', False), ('zero_live_finalization', 1),
            ('cleanup_entries', 1), ('cleanup_entries', False),
            ('execution_checkpoints', 2), ('execution_checkpoints', True),
            ('publication_checkpoints', ['Manifest', 'Native', 'Commit']),
            ('native_first', False), ('public_activation', True),
            ('installed_no_cargo_acceptance', True),
        ]
        for field, value in mutations:
            with tempfile.TemporaryDirectory() as temp:
                self.output = Path(temp)
                values = self.create()
                self.mutate(values, lambda o, m, p: o.update({field: value}))
                self.reject(values)

    def test_rehashed_complete_provider_bundle_divergence_rejects(self):
        values = self.create()
        bundle = self.output / f'native-{CASE}-bootstrap/bundle'
        artifact = bundle / 'native/fixed-oracle.elf'
        artifact.write_bytes(artifact.read_bytes() + b'changed')
        manifest = v.cold.read(bundle / 'zryna-manifest-v3.json')
        manifest['artifacts'][0].update(bytes=artifact.stat().st_size, sha256=v.digest(artifact))
        self.write(bundle / 'zryna-manifest-v3.json', manifest)
        self.reject(values)

    def test_positive_requires_typed_forty_one(self):
        for value in (43, True, '41'):
            with tempfile.TemporaryDirectory() as temp:
                self.output = Path(temp)
                values = self.create(POSITIVE)

                def change(observation, manifest, directory):
                    observation['results'][0]['outcome']['value']['value'] = value
                    manifest['results'] = copy.deepcopy(observation['results'])

                self.mutate(values, change)
                self.reject(values)

    def test_positive_requires_no_fault_or_cleanup_trace(self):
        for field in ('fault', 'trace_oracle', 'trace'):
            with tempfile.TemporaryDirectory() as temp:
                self.output = Path(temp)
                values = self.create(POSITIVE)

                def change(observation, manifest, directory):
                    if field == 'fault':
                        observation['fault'] = copy.deepcopy(FAULT)
                    elif field == 'trace_oracle':
                        observation['trace_oracle'] = ORACLE
                    else:
                        observation['results'][0]['trace'] = []
                        manifest['results'] = copy.deepcopy(observation['results'])

                self.mutate(values, change)
                self.reject(values)


if __name__ == '__main__':
    unittest.main()
