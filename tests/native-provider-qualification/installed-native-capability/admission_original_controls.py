"""Hostile in-memory views of an actual retained original proof; never executes it."""
import copy
from pathlib import Path
from types import SimpleNamespace
import unittest
from unittest import mock
import admission as a


def journal_key(rows, target):
    """Select one journal entry while preserving its actual host spelling."""
    keys = [key for key in rows if type(key) is str and key.replace('\\', '/') == target]
    if len(keys) != 1:
        raise ValueError('hostile view requires one unaliased journal entry')
    return keys[0]


def replacement_hostile_view(row, scenario, target, kind):
    """Attack an effective replacement or the distinct OS-prevention contract."""
    mutation = row['result']['mutation']
    if mutation['id'] != scenario or type(mutation['effective']) is not bool:
        raise ValueError('hostile replacement view requires its typed original mutation')
    first = row['before'][journal_key(row['before'], target)]
    last_key = journal_key(row['after'], target)
    last = row['after'][last_key]
    if first['kind'] != kind or last['kind'] != kind:
        raise ValueError('hostile replacement view requires the actual target kind')
    if mutation['effective']:
        if [first['device'], first['inode']] == [last['device'], last['inode']]:
            raise ValueError('effective replacement lacks changed target identity')
        if kind == 'file':
            if (first['bytes'], first['sha256']) != (last['bytes'], last['sha256']):
                raise ValueError('identity replacement must preserve actual file bytes')
            row['after'][last_key] = copy.deepcopy(first)
        else:
            row['after'] = copy.deepcopy(row['before'])
    else:
        if type(last['inode']) is not int or not 0 <= last['inode'] < 2**128:
            raise ValueError('OS-prevention target inode must be a bounded integer')
        # Preserve result/raw streams and file bytes. Only the copied Python
        # checkpoint changes, contradicting the unchanged-fixture denial claim.
        row['after'][last_key] = copy.deepcopy(last)
        row['after'][last_key]['inode'] ^= 1


def parent_replacement_view(row):
    replacement_hostile_view(row, 'installation-parent-replaced-identical-tree', 'installation', 'directory')


def descriptor_replacement_view(row):
    replacement_hostile_view(row, 'descriptor-replaced-identical-bytes',
                             'installation/metadata/native-provider.json', 'file')


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


    def fixture(self, scenario, effective, windows):
        before, after = CheckpointControls().rows()
        directory = copy.deepcopy(before['sources/packages/app'])
        directory['inode'] = 18
        descriptor = copy.deepcopy(before['installation/LICENSE'])
        descriptor['inode'] = 20
        before.update({'installation': directory, 'installation/metadata': copy.deepcopy(directory),
                       'installation/metadata/native-provider.json': descriptor})
        after.update(copy.deepcopy({k: v for k, v in before.items() if k.startswith('installation')}))
        target = 'installation' if scenario.startswith('installation-') else 'installation/metadata/native-provider.json'
        backup = 'mutation-backups/original-tree' if scenario.startswith('installation-') else 'mutation-backups/original-file'
        if effective:
            after[target]['inode'] += 1
            after[backup] = copy.deepcopy(before[target])

        def own(rows):
            result = {}
            for key, value in rows.items():
                if key != 'installation' and not key.startswith('installation/'):
                    continue
                name = '' if key == 'installation' else key[len('installation/'):]
                if windows:
                    name = name.replace('/', '\\')
                result[name] = {k: value[k] for k in ('device', 'inode', 'links', 'bytes')}
                result[name].update(file=value['kind'] == 'file', directory=value['kind'] == 'directory')
                if value['kind'] == 'file':
                    result[name]['sha256'] = value['sha256']
            return result

        mutation = {'id': scenario, 'effective': effective, 'before': own(before), 'after': own(after)}
        if not effective:
            mutation['prevention'] = {'errno': 32, 'original_tree_unchanged': True, 'exercised_rejection_credit': False}
        if windows:
            before = {k.replace('/', '\\'): v for k, v in before.items()}
            after = {k.replace('/', '\\'): v for k, v in after.items()}
        result = {'mutation': mutation, 'status': 'rejected' if effective else 'verified',
                  'phase': 'source-capture', 'diagnostics': [{'code': 'modeled'}]}
        return {'exit': 2 if effective else 0, 'before': before, 'after': after, 'result': result,
                'stdout': a.file_binding(a.canonical(result)), 'stderr': a.file_binding(b'')}

    def exercise(self, scenario, effective, mutate):
        for platform, windows in (('linux', False), ('win32', True)):
            with self.subTest(platform=platform, effective=effective):
                row = self.fixture(scenario, effective, windows)
                a.mutation_binding(row['result']['mutation'], row, scenario, platform)
                if not effective:
                    self.assertEqual(a.disposition(row, 'source-capture'), 'blocked')
                original = copy.deepcopy(row)
                mutate(row)
                self.assertEqual(row['result'], original['result'])
                self.assertEqual(row['before'], original['before'])
                self.assertEqual(row['stdout'], original['stdout'])
                self.assertEqual(row['stderr'], original['stderr'])
                old_bytes = {k: (v.get('bytes'), v.get('sha256'), v.get('base64'))
                             for k, v in original['after'].items() if v['kind'] == 'file'}
                new_bytes = {k: (v.get('bytes'), v.get('sha256'), v.get('base64'))
                             for k, v in row['after'].items() if v['kind'] == 'file'}
                if not (effective and scenario.startswith('installation-')):
                    self.assertEqual(new_bytes, old_bytes)
                with self.assertRaises(ValueError):
                    a.mutation_binding(row['result']['mutation'], row, scenario, platform)
                if not effective:
                    with self.assertRaises(ValueError):
                        a.disposition(row, 'source-capture')

    def test_effective_parent_identity_restore_rejected_both_spellings(self):
        self.exercise('installation-parent-replaced-identical-tree', True, parent_replacement_view)

    def test_effective_descriptor_identity_restore_rejected_both_spellings(self):
        self.exercise('descriptor-replaced-identical-bytes', True, descriptor_replacement_view)

    def test_OS_prevented_parent_identity_forgery_rejected_both_spellings(self):
        self.exercise('installation-parent-replaced-identical-tree', False, parent_replacement_view)

    def test_OS_prevented_descriptor_identity_forgery_rejected_both_spellings(self):
        self.exercise('descriptor-replaced-identical-bytes', False, descriptor_replacement_view)

    def test_unchanged_OS_prevention_view_is_legitimate_incomplete_evidence(self):
        for windows in (False, True):
            row = self.fixture('installation-parent-replaced-identical-tree', False, windows)
            row['after'] = copy.deepcopy(row['before'])
            a.mutation_binding(row['result']['mutation'], row, row['result']['mutation']['id'], 'win32' if windows else 'linux')
            self.assertEqual(a.disposition(row, 'source-capture'), 'blocked')

    def test_unique_journal_key_preserves_spelling_and_rejects_aliases(self):
        target = 'installation/metadata/native-provider.json'
        for key in (target, target.replace('/', '\\')):
            self.assertEqual(journal_key({key: {}}, target), key)
        for rows in ({}, {target: {}, target.replace('/', '\\'): {}}):
            with self.assertRaises(ValueError):
                journal_key(rows, target)

    def test_effective_claim_without_replaced_identity_is_not_rewritten(self):
        row = self.fixture('descriptor-replaced-identical-bytes', False, False)
        row['result']['mutation']['effective'] = True
        original = copy.deepcopy(row)
        with self.assertRaises(ValueError):
            descriptor_replacement_view(row)
        self.assertEqual(row, original)

    def test_typed_original_effect_flag_required(self):
        for flag in (None, 0, 1):
            row = self.fixture('descriptor-replaced-identical-bytes', False, False)
            row['result']['mutation']['effective'] = flag
            with self.assertRaises(ValueError):
                descriptor_replacement_view(row)


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
                   ('cases/installation-parent-replaced-identical-tree/candidate-execution.json', parent_replacement_view),
                   ('cases/descriptor-replaced-identical-bytes/candidate-execution.json', descriptor_replacement_view)]
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
