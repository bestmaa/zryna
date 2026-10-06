"""Hostile in-memory views of an actual retained original proof; never executes it."""
import copy
from pathlib import Path
from types import SimpleNamespace
import unittest
from unittest import mock
import admission as a


class CheckpointControls(unittest.TestCase):
    def rows(self):
        lock = {'kind': 'file', 'bytes': 4, 'sha256': a.digest(b'lock'), 'base64': 'bG9jaw==', 'mode': 384,
                'device': 1, 'inode': 2, 'links': 1, 'mtime_ns': 3, 'ctime_ns': 4}
        directory = {'kind': 'directory', 'bytes': 0, 'mode': 448, 'device': 1, 'inode': 5,
                     'links': 1, 'mtime_ns': 3, 'ctime_ns': 4}
        before = {'sources/packages/app/zryna.lock.json': lock, 'sources/packages/app': directory,
                  'installation/LICENSE': copy.deepcopy(lock)}
        after = copy.deepcopy(before)
        after['sources/packages/app/zryna.lock.json'].update(inode=9, mtime_ns=10, ctime_ns=11)
        after['sources/packages/app'].update(mtime_ns=10, ctime_ns=11)
        return before, after

    def test_exact_update_checkpoint_both_path_spellings(self):
        before, after = self.rows()
        self.assertTrue(a.package_update_only(before, after))
        self.assertTrue(a.package_update_only({p.replace('/', '\\'): r for p, r in before.items()},
                                            {p.replace('/', '\\'): r for p, r in after.items()}))

    def test_content_identity_census_and_other_paths_cannot_change(self):
        before, valid = self.rows()
        for path, field, value in [('sources/packages/app/zryna.lock.json', 'sha256', 'a'*64),
                                   ('sources/packages/app/zryna.lock.json', 'links', 2),
                                   ('sources/packages/app/zryna.lock.json', 'mode', 511),
                                   ('sources/packages/app/zryna.lock.json', 'device', 2),
                                   ('sources/packages/app', 'inode', 2), ('installation/LICENSE', 'ctime_ns', 20),
                                   ('sources/packages/app/zryna.lock.json', 'inode', True)]:
            self.assertTrue(a.package_update_only(before, valid)); changed = copy.deepcopy(valid); changed[path][field] = value
            self.assertFalse(a.package_update_only(before, changed))
        changed = copy.deepcopy(valid); changed['foreign'] = changed['installation/LICENSE']
        self.assertFalse(a.package_update_only(before, changed))

    def test_os_prevention_checkpoint_is_blocked_and_incomplete(self):
        before, after = self.rows()
        result = {'exit': 0, 'before': before, 'after': after, 'result': {'mutation': {'effective': False,
                  'prevention': {'errno': 32, 'original_tree_unchanged': True, 'exercised_rejection_credit': False}}}}
        self.assertEqual(a.disposition(result, 'source-capture'), 'blocked')
        result['after']['installation/LICENSE']['sha256'] = 'a'*64
        with self.assertRaises(ValueError):
            a.disposition(result, 'source-capture')

    def test_update_alone_cannot_supply_effective_mutation_credit(self):
        before, after = self.rows(); mutation = {'id': 'license-replaced-identical-bytes', 'effective': True}
        with self.assertRaises(ValueError):
            a.mutation_binding(mutation, {'before': before, 'after': after}, mutation['id'], 'win32')

    def test_python_identity_fields_reject_boolean_and_unbounded_values(self):
        before, _ = self.rows(); a.journal(before)
        for field in ('device', 'inode', 'links', 'mode', 'mtime_ns', 'ctime_ns'):
            for value in (False, 2**128):
                changed = copy.deepcopy(before); changed['sources/packages/app/zryna.lock.json'][field] = value
                with self.assertRaises(ValueError):
                    a.journal(changed)


class OriginalPacketControls(unittest.TestCase):
    def test_actual_original_packet_and_coherent_hostile_views(self):
        supplied = self.inputs
        args = SimpleNamespace(**supplied)
        baseline = a.verify(**supplied)
        self.assertTrue(baseline['evidence_valid'])
        folder = Path(args.proof)
        doc = a.document
        attacks = [('cases/receipt.json', lambda r: r['cases'].pop()),
                   ('cases/receipt.json', lambda r: r['cases'].__setitem__(10, r['cases'][9])),
                   ('cases/receipt.json', lambda r: r['cases'].reverse()),
                   ('cases/receipt.json', lambda r: r['cases'][9].update(id='real-native-v5-package')),
                   ('build/build-receipt.json', lambda r: r['binding'].update(head='f'*40)),
                   ('build/pinned-cargo-version-execution.json', lambda r: r['selected_environment'].update(RUSTFLAGS='forged')),
                   ('cases/installation-parent-replaced-identical-tree/candidate-execution.json', lambda r: r.update(after=copy.deepcopy(r['before']))),
                   ('cases/descriptor-replaced-identical-bytes/candidate-execution.json', lambda r:
                       r['after'].__setitem__('installation/metadata/native-provider.json', copy.deepcopy(r['before']['installation/metadata/native-provider.json'])))]
        if args.platform == 'win32':
            attacks += [('private-descriptor-units-execution.json', mutate) for mutate in (
                lambda r: r['selected_environment'].update(RUSTC_WRAPPER=r'C:\foreign.exe'),
                lambda r: r['selected_environment'].update(CARGO_HOME=r'C:\foreign'),
                lambda r: r['selected_environment'].update(RUSTUP_TOOLCHAIN='stable'), lambda r: r.update(cwd=r'C:\foreign'))]
        for relative, mutate in attacks:
            with self.subTest(original_record=relative):
                changed = copy.deepcopy(doc(folder / relative)); mutate(changed)
                def view(path):
                    return copy.deepcopy(changed) if Path(path) == folder / relative else doc(path)
                with mock.patch.object(a, 'document', view), mock.patch('admission_build.document', view), self.assertRaises((ValueError, KeyError, TypeError)):
                    a.verify(**supplied)
        self.assertTrue(a.verify(**supplied)['evidence_valid'])
