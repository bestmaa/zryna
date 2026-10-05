"""Handles-only synthetic reader controls; fabricated frames never prove execution."""
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
ALLOCATION = {'kind': 'trapped', 'code': 'zryna.trap.allocation-v1'}
REFCOUNT = {'kind': 'trapped', 'code': 'zryna.trap.refcount-v1'}
PROBES = {
    'handles-physical-1': (1, 'handles-fault-2-1'),
    'handles-physical-2': (2, 'handles-fault-2-1'),
    'handles-physical-3': (3, 'handles-fault-4-1'),
    'handles-physical-4': (4, 'handles-fault-4-2'),
    'handles-physical-5': (5, 'handles-fault-4-3'),
}
MAIN_SHA = '3367e2543514e4f73d862556b27c4691082acee78aba9e576b7e4168f8397495'
BODY_SHA = 'aace98260d527614117b7ad7b40b0da402ee2e9aed72f1889c202113682fa968'


def cleanup(place):
    return {'kind': 'cleanup', 'module': 1, 'function': 0, 'place': place}


SHARED = {'kind': 'drop', 'value': 'shared'}
WEAK = {'kind': 'drop', 'value': 'weak'}
RELEASES = [{'kind': 'release-implicit-weak'}, {'kind': 'release-control'}]
TRACES = {
    1: [], 2: [],
    3: [cleanup(1), SHARED, *RELEASES],
    4: [cleanup(3), SHARED, cleanup(1), SHARED, *RELEASES],
    5: [cleanup(5), WEAK, cleanup(3), SHARED, cleanup(1), SHARED, *RELEASES],
}


class HandlesControls(unittest.TestCase):
    # Only neutral helpers are borrowed; no older test methods are run/inherited.
    write = helpers.PhysicalControls.write
    admit = helpers.PhysicalControls.admit
    mutate = helpers.PhysicalControls.mutate
    reject = helpers.PhysicalControls.reject

    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.output = Path(self.temp.name)
        self.fixtures, self.cases = v.authority(ROOT, group='handles')

    def fresh(self):
        self.output = Path(tempfile.mkdtemp(dir=self.temp.name))

    def create(self, case='handles-physical-5'):
        row, injected = self.cases[case]
        sources, edges = v.source_contract(ROOT, self.fixtures, 'handles')
        ordinal, oracle = PROBES[case] if injected else (None, None)
        # Logical 4/x rows have Refcount outcomes: never copy them as physical outcomes.
        outcome = copy.deepcopy(ALLOCATION) if injected else {
            'kind': 'returned', 'value': {'type': 'i32', 'value': 23}}
        expected = [{'target': 'native', 'outcome': outcome}]
        if injected and row['trace']:
            expected[0]['trace'] = copy.deepcopy(row['trace'])
        fault = ({'mode': 'physical-allocation', 'code': 6, 'ordinal': ordinal,
                  'command': 0x26000000 | ordinal} if injected else None)
        for provider in ('native-retained', 'bootstrap'):
            directory = self.output / f'native-{case}-{provider}'
            (directory / 'sources').mkdir(parents=True)
            for source in sources:
                fixture = self.fixtures['handles' if source['id'] == 0 else 'handles-body']
                (directory / 'sources' / source['path']).write_bytes(
                    (ROOT / fixture['path']).read_bytes())
            observation = {
                'case': case, 'fixture': 'handles', 'injected': injected,
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
            # ELF header fixture only, with no emitted program or native execution.
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

    def coherent_results(self, values, change):
        def mutate(observation, manifest, directory):
            change(observation['results'][0])
            manifest['results'] = copy.deepcopy(observation['results'])
        self.mutate(values, mutate)

    def test_exact_five_probes_positive_and_default_scope(self):
        self.assertEqual(set(self.cases), {*PROBES, 'positive-handles'})
        self.assertEqual(v.GROUPS['handles'], 'handles')
        self.assertNotIn('all', v.GROUPS)
        self.assertNotIn('aggregate', v.GROUPS)
        self.assertEqual(v.authority.__defaults__, ('owned-shared',))
        self.assertEqual(v.verify.__defaults__[-1], 'owned-shared')
        self.assertEqual(v.TESTS['handles'],
                         'ownership_commands::conformance::native_provider_faults::physical::'
                         'handles_physical_group_executes_through_retained_source')
        self.assertEqual(v.REGISTRY_SHA,
                         '34cd29a5f146d77e7163b32d21e71e4f5a1fc5fd50f688d197de8bef9b38a508')
        for case, (ordinal, oracle) in PROBES.items():
            row, injected = self.cases[case]
            self.assertTrue(injected)
            self.assertEqual(row['id'], oracle)
            self.assertEqual(row['trace'], TRACES[ordinal])
            self.assertEqual(row['expected'], ALLOCATION if ordinal < 3 else REFCOUNT)
            self.assertEqual(v.fault_for(case), {
                'mode': 'physical-allocation', 'code': 6, 'ordinal': ordinal,
                'command': 0x26000000 | ordinal,
            })
            self.assertEqual(v.oracle_for(case), oracle)
            expected = [{'target': 'native', 'outcome': ALLOCATION}]
            if TRACES[ordinal]:
                expected[0]['trace'] = TRACES[ordinal]
            self.assertEqual(v.expected_result(row, True, 'native'), expected)
        for case in self.cases:
            self.admit(self.create(case))

    def test_two_frozen_sources_and_handles_import_graph(self):
        sources, edges = v.source_contract(ROOT, self.fixtures, 'handles')
        self.assertEqual(sources, [{'id': 0, 'path': 'main.zry', 'sha256': MAIN_SHA},
                                   {'id': 1, 'path': 'math.zry', 'sha256': BODY_SHA}])
        self.assertEqual(edges, [{'importer': 'main.zry', 'target': 'math.zry',
                                 'specifier': './math.zry', 'imported': 'handles',
                                 'local': 'handles'}])
        self.assertEqual((ROOT / self.fixtures['handles']['path']).read_text(),
                         'import { handles } from "./math.zry";\n'
                         'export function score(): i32 { return handles(); }\n')

    def test_logical_authority_must_validate_refcount_oracle(self):
        registry = v.cold.read(ROOT / 'tests/m3-conformance-v1.json')
        real_read = v.cold.read
        for oracle in ('handles-fault-4-1', 'handles-fault-4-2', 'handles-fault-4-3'):
            altered = copy.deepcopy(registry)
            next(row for row in altered['faults'] if row['id'] == oracle)['expected'] = ALLOCATION
            with patch.object(v.cold, 'read', lambda path: altered if path.name ==
                              'm3-conformance-v1.json' else real_read(path)):
                with self.assertRaises(AssertionError):
                    v.authority(ROOT, group='handles')

    def test_empty_traces_must_be_omitted_for_ordinals_one_and_two(self):
        for case in ('handles-physical-1', 'handles-physical-2'):
            for trace in ([], TRACES[3]):
                self.fresh()
                values = self.create(case)
                self.coherent_results(values, lambda result: result.update(trace=trace))
                self.reject(values)

    def test_exact_ordinal_command_mode_and_code(self):
        for case, (ordinal, oracle) in PROBES.items():
            for key, value in (('ordinal', ordinal + 1), ('ordinal', True),
                               ('command', 0x26000000 | (ordinal + 1)),
                               ('command', False), ('command', 0x22000000 | ordinal),
                               ('mode', 'logical'), ('code', 2), ('code', True)):
                self.fresh()
                values = self.create(case)
                self.mutate(values, lambda o, m, p: o['fault'].update({key: value}))
                self.reject(values)

    def test_identical_empty_trace_does_not_authorize_other_selector(self):
        for field, value in (('case', 'handles-physical-2'),
                             ('trace_oracle', 'handles-fault-4-1')):
            self.fresh()
            values = self.create('handles-physical-1')
            self.mutate(values, lambda o, m, p: o.update({field: value}))
            self.reject(values)

    def test_logical_refcount_outcome_rejected_with_identical_trace(self):
        for case in ('handles-physical-3', 'handles-physical-4', 'handles-physical-5'):
            self.fresh()
            values = self.create(case)
            self.coherent_results(values, lambda result: result.update(outcome=REFCOUNT))
            self.reject(values)

    def test_wrong_allocation_outcome_types_and_returned_calibration(self):
        for outcome in ({'kind': 'trapped', 'code': 'zryna.trap.capacity-v1'},
                        {'kind': 'trapped', 'code': True},
                        {'kind': 'returned', 'value': {'type': 'i32', 'value': 23}}):
            self.fresh()
            values = self.create()
            self.coherent_results(values, lambda result: result.update(outcome=outcome))
            self.reject(values)

    def test_complete_cleanup_order_releases_places_and_multiplicity(self):
        for ordinal in (3, 4, 5):
            trace = copy.deepcopy(TRACES[ordinal])
            variants = [list(reversed(trace)), trace[:-1], trace[:-2],
                        trace + [trace[-1]], trace[:1] + trace[2:],
                        [{**trace[0], 'place': 7}, *trace[1:]],
                        [{**trace[0], 'module': 0}, *trace[1:]],
                        [{**trace[0], 'function': True}, *trace[1:]],
                        [*trace[:-2], *reversed(trace[-2:])],
                        [trace[0], WEAK if ordinal < 5 else SHARED, *trace[2:]]]
            if ordinal > 3:
                variants.append([*trace[2:4], *trace[:2], *trace[4:]])
            for altered in variants:
                self.fresh()
                values = self.create(f'handles-physical-{ordinal}')
                self.coherent_results(values, lambda result: result.update(trace=altered))
                self.reject(values)

    def test_coherent_missing_or_wrong_import_edge_and_source(self):
        for field in ('missing-edge', 'missing-source', 'imported', 'local',
                      'specifier', 'importer', 'target', 'duplicate-source', 'source-id'):
            self.fresh()
            values = self.create()
            def change(observation, manifest, directory):
                if field == 'missing-edge':
                    manifest['edges'] = []
                elif field == 'missing-source':
                    manifest['sources'].pop()
                    manifest['edges'] = []
                    observation['sources'].pop('math.zry')
                    (directory / 'sources/math.zry').unlink()
                elif field == 'duplicate-source':
                    manifest['sources'].append(copy.deepcopy(manifest['sources'][1]))
                elif field == 'source-id':
                    manifest['sources'][1]['id'] = 2
                elif field in ('importer', 'target'):
                    manifest['edges'][0][field] = ('math.zry' if field == 'importer' else 'main.zry')
                else:
                    manifest['edges'][0][field] = 'aggregate'
                if field not in ('duplicate-source', 'source-id'):
                    manifest['graph_sha256'] = v.graph_digest(3, manifest['sources'], manifest['edges'])
            self.mutate(values, change)
            self.reject(values)

    def test_coherent_source_bytes_hash_graph_substitution(self):
        for path in ('main.zry', 'math.zry'):
            self.fresh()
            values = self.create()
            def change(observation, manifest, directory):
                source = directory / 'sources' / path
                source.write_bytes(b'export function handles(): i32 { return 23; }\n')
                observation['sources'][path] = v.digest(source)
                next(s for s in manifest['sources'] if s['path'] == path)['sha256'] = v.digest(source)
                manifest['graph_sha256'] = v.graph_digest(3, manifest['sources'], manifest['edges'])
            self.mutate(values, change)
            self.reject(values)

    def test_null_counts_zero_live_cleanup_and_native_checkpoints(self):
        for field, value in (
                ('physical_allocation_release_counts', {'allocated': 5, 'released': 5}),
                ('zero_live_finalization', False), ('cleanup_entries', 1),
                ('cleanup_entries', False), ('execution_checkpoints', 2),
                ('execution_checkpoints', True),
                ('publication_checkpoints', ['Manifest', 'Native', 'Commit']),
                ('native_first', False), ('public_activation', True),
                ('installed_no_cargo_acceptance', True), ('injected', 1),
                ('registry_sha256', 'a' * 64)):
            self.fresh()
            values = self.create()
            self.mutate(values, lambda o, m, p: o.update({field: value}))
            self.reject(values)

    def test_bootstrap_must_have_zero_execution_and_publication_checkpoints(self):
        for field, value in (('execution_checkpoints', 3),
                             ('publication_checkpoints', ['Native', 'Manifest', 'Commit'])):
            self.fresh()
            values = self.create()
            directory = self.output / f'native-{values[0]}-bootstrap'
            observation = v.cold.read(directory / 'observation.json')
            observation[field] = value
            self.write(directory / 'observation.json', observation)
            self.reject(values)

    def test_complete_provider_byte_equality_and_no_extra_bundle_files(self):
        for field in ('bytes', 'layout', 'extra-file'):
            self.fresh()
            values = self.create()
            bundle = self.output / f'native-{values[0]}-bootstrap/bundle'
            manifest = v.cold.read(bundle / 'zryna-manifest-v3.json')
            if field == 'bytes':
                artifact = bundle / 'native/fixed-oracle.elf'
                artifact.write_bytes(artifact.read_bytes() + b'different')
                manifest['artifacts'][0].update(bytes=artifact.stat().st_size, sha256=v.digest(artifact))
            elif field == 'layout':
                manifest['layouts'][next(iter(manifest['layouts']))] = 'b' * 64
            else:
                (bundle / 'extra').write_bytes(b'unaccounted')
            self.write(bundle / 'zryna-manifest-v3.json', manifest)
            self.reject(values)

    def test_positive_requires_exact_score_twenty_three_and_no_fault_or_trace(self):
        for field, value in (('returned', 24), ('returned', True), ('type', 'bool'),
                             ('fault', v.fault_for('handles-physical-1')),
                             ('trace', []), ('trace_oracle', 'handles-fault-2-1')):
            self.fresh()
            values = self.create('positive-handles')
            def change(observation, manifest, directory):
                result = observation['results'][0]
                if field == 'returned':
                    result['outcome']['value']['value'] = value
                elif field == 'type':
                    result['outcome']['value']['type'] = value
                elif field == 'trace':
                    result['trace'] = value
                else:
                    observation[field] = value
                manifest['results'] = copy.deepcopy(observation['results'])
            self.mutate(values, change)
            self.reject(values)

    def proof(self):
        # Synthetic accounting fixture; mocked provenance cannot prove native execution.
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
            'format': 'zryna.retained-physical-corpus.v2', 'group': 'handles',
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
                {'label': 'physical-corpus', 'argv': [binary, v.TESTS['handles'],
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
            return v.verify(ROOT, proof, 'a' * 40, 'linux', archived=True, group='handles')

    def test_six_pairs_twelve_observations_and_failed_producer_all_or_nothing(self):
        proof, receipt = self.proof()
        baseline = self.accounting(proof)
        self.assertEqual(baseline['discharged_physical_probes'], list(PROBES))
        self.assertEqual((baseline['passed_pairs'], baseline['observations']), (6, 12))
        self.assertEqual(baseline['other_physical_probes_remaining'], 7)
        self.assertIsNone(baseline['physical_allocation_release_counts'])
        receipt['status'] = 'failed'
        receipt['commands'][1]['exit'] = 101
        self.write(proof / 'receipt.json', receipt)
        (proof / 'physical-corpus.stdout').write_text(
            'test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 361 filtered out;\n')
        failed = self.accounting(proof)
        self.assertEqual((failed['status'], failed['passed_pairs']), ('failed', 6))
        self.assertEqual(failed['discharged_physical_probes'], [])
        self.assertEqual(failed['remaining_physical_probes'], list(PROBES))
        proof, receipt = self.proof()
        self.mutate(('handles-physical-5', *self.cases['handles-physical-5']),
                    lambda o, m, p: o.update(zero_live_finalization=False))
        partial = self.accounting(proof)
        self.assertEqual((partial['status'], partial['passed_pairs'], partial['failed_pairs']),
                         ('failed', 5, 1))
        self.assertEqual(partial['discharged_physical_probes'], [])
        self.assertEqual(partial['remaining_physical_probes'], list(PROBES))

    def test_missing_duplicate_extra_probe_or_missing_positive_rejects_census(self):
        for change in ('missing-probe', 'duplicate-probe', 'missing-positive'):
            proof, receipt = self.proof()
            if change == 'missing-probe':
                shutil.rmtree(self.output / 'native-handles-physical-1-bootstrap')
            elif change == 'duplicate-probe':
                shutil.copytree(self.output / 'native-handles-physical-1-bootstrap',
                                self.output / 'native-handles-physical-1-bootstrap-duplicate')
            else:
                shutil.rmtree(self.output / 'native-positive-handles-bootstrap')
            with self.assertRaises(AssertionError):
                self.accounting(proof)


if __name__ == '__main__':
    unittest.main()
