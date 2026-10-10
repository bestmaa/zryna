"""Vec-only hostile reader controls; fabricated frames never prove execution."""
import copy
import importlib.util
import json
from pathlib import Path
import shutil
import tempfile
import unittest
from unittest.mock import patch

spec = importlib.util.spec_from_file_location(
    'physical_neutral_helpers', Path(__file__).with_name('verify_test.py'))
helpers = importlib.util.module_from_spec(spec)
spec.loader.exec_module(helpers)
v = helpers.v
ROOT = Path(__file__).resolve().parents[3]
PROBES = {
    'vec-physical-2': (2, 'vec-fault-2-1'),
    'vec-physical-3': (3, 'vec-fault-2-2'),
    'vec-physical-4': (4, 'vec-fault-2-3'),
}


class VecControls(unittest.TestCase):
    # Neutral operations only: no inheritance or invocation of older test cases.
    write = helpers.PhysicalControls.write
    admit = helpers.PhysicalControls.admit
    mutate = helpers.PhysicalControls.mutate
    reject = helpers.PhysicalControls.reject

    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.output = Path(self.temp.name)
        self.fixtures, self.cases = v.authority(ROOT, group='vec')

    def create(self, case='vec-physical-3'):
        row, injected = self.cases[case]
        sources, edges = v.source_contract(ROOT, self.fixtures, 'vec')
        expected = v.expected_result(row, injected, 'native')
        ordinal, oracle = PROBES[case] if injected else (None, None)
        fault = ({'mode': 'physical-allocation', 'code': 6, 'ordinal': ordinal,
                  'command': 0x26000000 | ordinal} if injected else None)
        for provider in ('native-retained', 'bootstrap'):
            directory = self.output / f'native-{case}-{provider}'
            (directory / 'sources').mkdir(parents=True)
            (directory / 'sources/main.zry').write_bytes(
                (ROOT / self.fixtures['vec']['path']).read_bytes())
            observation = {
                'case': case, 'fixture': 'vec', 'injected': injected,
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
            # Format-header fixture only; no emitted program or native execution.
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

    def fresh(self):
        self.output = Path(tempfile.mkdtemp(dir=self.temp.name))

    def coherent_results(self, values, change):
        def mutate(observation, manifest, directory):
            change(observation['results'][0])
            manifest['results'] = copy.deepcopy(observation['results'])
        self.mutate(values, mutate)

    def test_exact_four_census_single_source_and_all_frames(self):
        self.assertEqual(set(self.cases), {*PROBES, 'positive-vec'})
        self.assertIsNone(v.GROUPS['vec'])
        sources, edges = v.source_contract(ROOT, self.fixtures, 'vec')
        self.assertEqual(sources, [{'id': 0, 'path': 'main.zry',
                                    'sha256': self.fixtures['vec']['sha256']}])
        self.assertEqual(edges, [])
        for case, (ordinal, oracle) in PROBES.items():
            self.assertEqual(v.fault_for(case), {
                'mode': 'physical-allocation', 'code': 6, 'ordinal': ordinal,
                'command': 0x26000000 | ordinal,
            })
            self.assertEqual(v.oracle_for(case), oracle)
        for case in self.cases:
            self.admit(self.create(case))

    def test_ordinal_two_trace_must_be_omitted(self):
        for trace in ([], self.cases['vec-physical-3'][0]['trace']):
            self.fresh()
            values = self.create('vec-physical-2')
            self.assertNotIn('trace', v.expected_result(values[1], True, 'native')[0])
            self.coherent_results(values, lambda result: result.update(trace=trace))
            self.reject(values)

    def test_logical_fault_cannot_substitute_for_physical(self):
        values = self.create()
        self.mutate(values, lambda o, m, p: o.update(fault={
            'mode': 'logical', 'code': 2, 'ordinal': 2, 'command': 0x22000002}))
        self.reject(values)

    def test_wrong_physical_ordinal_command_and_boolean(self):
        for key, value in (('ordinal', 2), ('command', 0x26000004),
                           ('ordinal', True), ('command', False)):
            self.fresh()
            values = self.create()
            self.mutate(values, lambda o, m, p: o['fault'].update({key: value}))
            self.reject(values)

    def test_same_trace_wrong_case_or_oracle(self):
        # Ordinals3/4 have identical frozen traces; their selectors stay distinct.
        for field, value in (('case', 'vec-physical-4'),
                             ('trace_oracle', 'vec-fault-2-3')):
            self.fresh()
            values = self.create()
            self.mutate(values, lambda o, m, p: o.update({field: value}))
            self.reject(values)

    def test_wrong_physical_mode_or_code(self):
        for key, value in (('mode', 'logical-allocation'), ('code', 2), ('code', True)):
            self.fresh()
            values = self.create()
            self.mutate(values, lambda o, m, p: o['fault'].update({key: value}))
            self.reject(values)

    def test_allocation_trap_requires_exact_type_and_code(self):
        for outcome in ({'kind': 'trapped', 'code': 'zryna.trap.capacity-v1'},
                        {'kind': 'trapped', 'code': True},
                        {'kind': 'returned', 'value': {'type': 'i32', 'value': 13}}):
            self.fresh()
            values = self.create()
            self.coherent_results(values, lambda result: result.update(outcome=outcome))
            self.reject(values)

    def test_cleanup_trace_order_identity_and_multiplicity(self):
        trace = self.cases['vec-physical-3'][0]['trace']
        self.assertEqual(trace, [
            {'kind': 'cleanup', 'module': 0, 'function': 0, 'place': 1},
            {'kind': 'drop', 'value': 'sequence'},
        ])
        variants = [list(reversed(trace)), trace[:1], trace + [trace[-1]],
                    [{**trace[0], 'place': 2}, trace[1]],
                    [{**trace[0], 'module': False}, trace[1]]]
        for altered in variants:
            self.fresh()
            values = self.create()
            self.coherent_results(values, lambda result: result.update(trace=altered))
            self.reject(values)

    def test_single_source_rejects_coherent_import_graph(self):
        values = self.create()
        def change(observation, manifest, directory):
            edge = {'importer': 'main.zry', 'target': 'main.zry',
                    'specifier': './main.zry', 'imported': 'score', 'local': 'score'}
            manifest['edges'] = [edge]
            manifest['graph_sha256'] = v.graph_digest(3, manifest['sources'], [edge])
        self.mutate(values, change)
        self.reject(values)

    def test_coherent_source_hash_and_graph_substitution(self):
        values = self.create()
        def change(observation, manifest, directory):
            source = directory / 'sources/main.zry'
            source.write_bytes(b'export function score(): i32 { return 13; }\n')
            observation['sources']['main.zry'] = v.digest(source)
            manifest['sources'][0]['sha256'] = v.digest(source)
            manifest['graph_sha256'] = v.graph_digest(3, manifest['sources'], [])
        self.mutate(values, change)
        self.reject(values)

    def test_null_counts_zero_live_cleanup_and_checkpoints(self):
        mutations = [('physical_allocation_release_counts', {'allocated': 4, 'released': 4}),
                     ('zero_live_finalization', False), ('cleanup_entries', 1),
                     ('cleanup_entries', False), ('execution_checkpoints', 2),
                     ('execution_checkpoints', True),
                     ('publication_checkpoints', ['Manifest', 'Native', 'Commit']),
                     ('native_first', False), ('public_activation', True),
                     ('installed_no_cargo_acceptance', True)]
        for field, value in mutations:
            self.fresh()
            values = self.create()
            self.mutate(values, lambda o, m, p: o.update({field: value}))
            self.reject(values)

    def test_rehashed_complete_provider_bundle_divergence(self):
        values = self.create()
        bundle = self.output / f'native-{values[0]}-bootstrap/bundle'
        artifact = bundle / 'native/fixed-oracle.elf'
        artifact.write_bytes(artifact.read_bytes() + b'different')
        manifest = v.cold.read(bundle / 'zryna-manifest-v3.json')
        manifest['artifacts'][0].update(bytes=artifact.stat().st_size, sha256=v.digest(artifact))
        self.write(bundle / 'zryna-manifest-v3.json', manifest)
        self.reject(values)

    def test_positive_requires_thirteen_without_fault_or_trace(self):
        for field in ('returned', 'fault', 'trace', 'trace_oracle'):
            self.fresh()
            values = self.create('positive-vec')
            def change(observation, manifest, directory):
                if field == 'returned':
                    observation['results'][0]['outcome']['value']['value'] = True
                elif field == 'trace':
                    observation['results'][0]['trace'] = []
                else:
                    observation[field] = (v.fault_for('vec-physical-2')
                                          if field == 'fault' else 'vec-fault-2-1')
                manifest['results'] = copy.deepcopy(observation['results'])
            self.mutate(values, change)
            self.reject(values)

    def proof(self):
        # Accounting fixture only; mocked generic provenance never proves a run.
        proof = Path(tempfile.mkdtemp(dir=self.temp.name))
        self.output = proof / 'observations'
        self.output.mkdir()
        for case in self.cases:
            self.create(case)
        tools = {name: {'path': '/synthetic/' + name, 'sha256': 'a' * 64,
                       'version': version} for name, version in
                 [('cargo', 'cargo 1.97.1 synthetic'), ('rustc', 'rustc 1.97.1 synthetic'),
                  ('node', 'v22.22.1')]}
        binary = '/synthetic/test-binary'
        receipt = {
            'format': 'zryna.retained-physical-corpus.v2', 'group': 'vec',
            'head': 'a' * 40, 'tree': 'b' * 40, 'platform': 'linux', 'root': str(ROOT),
            'target': '/synthetic/target', 'inputs': {}, 'tools': tools,
            'run_id': None, 'run_attempt': None, 'status': 'passed',
            'public_activation': False, 'installed_no_cargo_acceptance': False,
            'test_binary': {'path': binary, 'sha256': 'a' * 64},
            'commands': [
                {'label': 'feature-test-build', 'argv': [tools['cargo']['path'], 'test',
                 '--locked', '--offline', '-p', 'zryna-driver', '--features',
                 'native-provider-internal', '--lib', '--no-run', '--message-format=json'],
                 'cwd': str(ROOT), 'exit': 0},
                {'label': 'physical-corpus', 'argv': [binary, v.TESTS['vec'],
                 '--exact', '--nocapture', '--test-threads=1'], 'cwd': str(ROOT), 'exit': 0},
            ],
        }
        self.write(proof / 'receipt.json', receipt)
        self.write(proof / 'feature-test-build.stdout', {
            'reason': 'compiler-artifact', 'executable': binary,
            'target': {'name': 'zryna_driver'}, 'profile': {'test': True},
        })
        (proof / 'physical-corpus.stdout').write_text(
            'test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 361 filtered out;\n')
        return proof, receipt

    def accounting(self, proof):
        def git(root, *args):
            return {'rev-parse HEAD': 'a' * 40, 'rev-parse HEAD^{tree}': 'b' * 40,
                    'status --porcelain --untracked-files=all': '', 'ls-files': ''}[' '.join(args)]
        with patch.object(v, 'git', git), patch.object(v, 'committed_inputs', return_value={}), \
                patch.object(v.cold, 'cargo_package_identity'):
            return v.verify(ROOT, proof, 'a' * 40, 'linux', archived=True, group='vec')

    def test_full_census_and_failed_producer_never_discharge(self):
        proof, receipt = self.proof()
        baseline = self.accounting(proof)
        self.assertEqual(baseline['discharged_physical_probes'], list(PROBES))
        self.assertEqual((baseline['passed_pairs'], baseline['observations']), (4, 8))
        self.assertEqual(baseline['other_physical_probes_remaining'], 9)
        receipt['status'] = 'failed'
        receipt['commands'][1]['exit'] = 101
        self.write(proof / 'receipt.json', receipt)
        (proof / 'physical-corpus.stdout').write_text(
            'test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 361 filtered out;\n')
        failed = self.accounting(proof)
        self.assertEqual(failed['status'], 'failed')
        self.assertEqual(failed['passed_pairs'], 4)
        self.assertEqual(failed['discharged_physical_probes'], [])
        self.assertEqual(failed['remaining_physical_probes'], list(PROBES))
        proof, receipt = self.proof()
        values = ('vec-physical-4', *self.cases['vec-physical-4'])
        self.mutate(values, lambda o, m, p: o.update(zero_live_finalization=False))
        partial = self.accounting(proof)
        self.assertEqual((partial['status'], partial['passed_pairs'], partial['failed_pairs']),
                         ('failed', 3, 1))
        self.assertEqual(partial['discharged_physical_probes'], [])
        self.assertEqual(partial['remaining_physical_probes'], list(PROBES))
        shutil.rmtree(proof / 'observations/native-vec-physical-2-bootstrap')
        with self.assertRaises(AssertionError):
            self.accounting(proof)


if __name__ == '__main__':
    unittest.main()
