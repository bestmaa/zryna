#!/usr/bin/env python3
"""Independent reader guard controls; modeled bytes never authenticate a build."""
import argparse
import base64
import copy
import importlib.util
import json
from pathlib import Path
import tempfile
import sys
from types import SimpleNamespace
import unittest
from unittest import mock

spec = importlib.util.spec_from_file_location('installation_admission', Path(__file__).with_name('admission.py'))
a = importlib.util.module_from_spec(spec)
spec.loader.exec_module(a)
from admission_original_controls import CheckpointControls


def profile():
    vc = a.Win(r'C:\Program Files\Microsoft Visual Studio\2022\Enterprise\VC\Tools\MSVC\14.44.35207')
    sdk = a.Win(r'C:\Program Files (x86)\Windows Kits\10')
    native = vc / 'bin/Hostx64/x64'; version = '10.0.26100.0'
    def row(role, path):
        return {'path': str(path), 'bytes': 100, 'sha256': 'a'*64,
                'raw_relative_path': 'tool-originals/' + role.replace('.', '-') + ('.bin' if role.endswith('.lib') else '.exe')}
    tools = {r: row(r, a.Win(r'C:\rust\bin') / (r + '.exe')) for r in ('cargo', 'rustc', 'rustup')}
    tools.update(linker=row('linker', native / 'link.exe'), dumpbin=row('dumpbin', native / 'dumpbin.exe'))
    libs = {'kernel32.lib': sdk / 'Lib' / version / 'um/x64/kernel32.lib',
            'ucrt.lib': sdk / 'Lib' / version / 'ucrt/x64/ucrt.lib',
            'vcruntime.lib': vc / 'lib/x64/vcruntime.lib', 'libcmt.lib': vc / 'lib/x64/libcmt.lib'}
    search = ';'.join(str(p) for p in [vc / 'lib/x64', sdk / 'Lib' / version / 'um/x64', sdk / 'Lib' / version / 'ucrt/x64'])
    return {'schema_version': 1, 'platform': 'win32', 'target': 'x86_64-pc-windows-msvc',
            'python': row('python', a.Win(r'C:\Python\python.exe')) | {'version': '3.12.10'},
            'tools': tools, 'rust_version': '1.97.1', 'cargo_version': '1.97.1',
            'sdk_inputs': {r: row(r, p) for r, p in libs.items()},
            'msvc': {'root': str(vc), 'version': '14.44.35207', 'sdk_root': str(sdk), 'sdk_version': version},
            'host_environment': {'SystemRoot': r'C:\Windows', 'PATH': ';'.join([r'C:\rust\bin', str(native), r'C:\Windows\System32', r'C:\Windows']),
                                 'LIB': search, 'LIBPATH': search, 'INCLUDE': ';'.join(str(p) for p in [vc / 'include', *[sdk / 'Include' / version / x for x in ('ucrt', 'um', 'shared')]])},
            'cargo_home': r'C:\cargo', 'rustup_home': r'C:\rustup',
            'observation_scope': 'per-run observed exact tools and selected core SDK inputs; no upstream supplier byte authority'}


class ProfileControls(unittest.TestCase):
    def reject(self, mutate):
        valid = profile(); a.profile_contract(valid)
        changed = copy.deepcopy(valid); mutate(changed)
        with self.assertRaises((ValueError, KeyError, TypeError)):
            a.profile_contract(changed)

    def test_observed_profile_positive(self):
        a.profile_contract(profile())

    def test_exact_python_pin(self):
        self.reject(lambda p: p['python'].update(version='3.12.11'))

    def test_rust_and_cargo_exact(self):
        for role in ('rust_version', 'cargo_version'):
            with self.subTest(role=role):
                self.reject(lambda p: p.update({role: '1.97.10'}))

    def test_unknown_and_missing_roles(self):
        self.reject(lambda p: p['tools'].update(node=p['python']))
        self.reject(lambda p: p['sdk_inputs'].pop('libcmt.lib'))

    def test_tool_metadata_not_boolean_or_uppercase(self):
        self.reject(lambda p: p['tools']['cargo'].update(bytes=True))
        self.reject(lambda p: p['tools']['cargo'].update(sha256='A'*64))

    def test_tool_path_traversal(self):
        self.reject(lambda p: p['tools']['rustc'].update(path=r'C:\rust\..\foreign.exe'))

    def test_retained_path_cannot_alias_role(self):
        self.reject(lambda p: p['tools']['rustc'].update(raw_relative_path='tool-originals/cargo.exe'))

    def test_linker_outside_selected_msvc(self):
        self.reject(lambda p: p['tools']['linker'].update(path=r'C:\foreign\link.exe'))

    def test_sdk_library_wrong_directory(self):
        self.reject(lambda p: p['sdk_inputs']['ucrt.lib'].update(path=r'C:\foreign\ucrt.lib'))

    def test_host_search_path_injection(self):
        self.reject(lambda p: p['host_environment'].update(PATH=p['host_environment']['PATH'] + r';C:\node'))

    def test_sdk_version_must_match_paths(self):
        self.reject(lambda p: p['msvc'].update(sdk_version='10.0.22621.0'))

    def test_supplier_claim_not_promoted(self):
        self.reject(lambda p: p.update(observation_scope='trusted complete SDK supplier'))


def tree_row(raw=None):
    row = {'kind': 'directory' if raw is None else 'file', 'bytes': 0 if raw is None else len(raw),
           'mode': 448 if raw is None else 384, 'device': 1, 'inode': 3, 'links': 1, 'mtime_ns': 4, 'ctime_ns': 5}
    if raw is not None:
        row.update(sha256=a.digest(raw), base64=base64.b64encode(raw).decode())
    return row


def make_record(folder, platform):
    pm = a.ntpath if platform == 'win32' else a.posixpath
    cwd = pm.join(r'C:\proof\cases' if platform == 'win32' else '/proof/cases', folder.name)
    exe = a.TARGETS[platform][1]; result = {'status': 'verified', 'mutation': None,
            'summary': {'protocol': 3, 'source_sha256': {}, 'graph_sha256': None},
            'public_default_acceptance': False, 'runtime_or_backend_execution': False}
    raw = a.canonical(result); image = b'modeled non-executable bytes'
    rows = {'.': tree_row(), 'empty-path': tree_row(), 'installation/' + exe: tree_row(image)}
    env = {'PATH': pm.join(cwd, 'live', 'empty-path'), 'HOME': cwd, 'TMPDIR': cwd, 'LANG': 'C', 'LC_ALL': 'C'}
    if platform == 'win32':
        env.update(TEMP=cwd, TMP=cwd)
    record = {'argv': [pm.join(cwd, 'live', 'installation', *exe.split('/')), pm.join(cwd, 'live', 'sources'),
                       pm.join(cwd, 'live', 'mutation-backups'), 'positive', '3'], 'cwd': cwd,
              'selected_environment': env, 'exit': 0, 'direct_child_reaped': True,
              'started_at': '2026-10-06T20:00:00+00:00', 'completed_at': '2026-10-06T20:00:01+00:00',
              'before': rows, 'after': copy.deepcopy(rows), 'result': result,
              'stdout': a.file_binding(raw), 'stderr': a.file_binding(b'')}
    (folder / 'baseline.stdout').write_bytes(raw); (folder / 'baseline.stderr').write_bytes(b'')
    save(folder, record)
    return record, a.digest(image)


def save(folder, row):
    (folder / 'baseline-execution.json').write_bytes(a.canonical(row))


class ExecutionControls(unittest.TestCase):
    def inspect(self, mutate=None, platform='win32'):
        with tempfile.TemporaryDirectory() as temporary:
            folder = Path(temporary) / 'case'; folder.mkdir()
            record, sha = make_record(folder, platform)
            checked = a.execution(folder, 'baseline', platform, 3, 'positive', sha)
            a.positive(checked, record['result']['summary'])
            if mutate:
                mutate(record, folder); save(folder, record)
                with self.assertRaises((ValueError, KeyError, TypeError)):
                    a.execution(folder, 'baseline', platform, 3, 'positive', sha)

    def test_linux_and_windows_valid_record_guard_only(self):
        for platform in ('linux', 'win32'):
            self.inspect(platform=platform)

    def test_boolean_exit_rejected(self):
        self.inspect(lambda r, _: r.update(exit=False))

    def test_unreaped_child_rejected(self):
        self.inspect(lambda r, _: r.update(direct_child_reaped=False))

    def test_wrong_scenario_and_protocol(self):
        self.inspect(lambda r, _: r['argv'].__setitem__(3, 'foreign-mutation'))
        self.inspect(lambda r, _: r['argv'].__setitem__(4, '4'))

    def test_foreign_image_path_and_bytes(self):
        self.inspect(lambda r, _: r['argv'].__setitem__(0, r'C:\foreign\proof.exe'))
        self.inspect(lambda r, _: r['before']['installation/bin/native-installation-proof.exe'].update(sha256='a'*64))

    def test_runtime_node_hook_rejected(self):
        self.inspect(lambda r, _: r['selected_environment'].update(NODE_OPTIONS='--require foreign.js'))

    def test_path_contains_subject_tools_rejected(self):
        self.inspect(lambda r, _: r['selected_environment'].update(PATH=r'C:\cargo\bin'))

    def test_empty_path_child_rejected(self):
        self.inspect(lambda r, _: r['before'].update({'empty-path/node.exe': tree_row(b'node')}))

    def test_forged_result_with_original_stdout(self):
        self.inspect(lambda r, _: r['result'].update(status='rejected'))

    def test_raw_stream_substitution(self):
        self.inspect(lambda _, p: (p / 'baseline.stdout').write_bytes(b'{}'))

    def test_stderr_is_retained_but_not_accepted(self):
        def change(r, p):
            (p / 'baseline.stderr').write_bytes(b'error'); r['stderr'] = a.file_binding(b'error')
        self.inspect(change)

    def test_unordered_or_timeout_timestamps(self):
        self.inspect(lambda r, _: r.update(completed_at='2026-10-06T19:59:59+00:00'))
        self.inspect(lambda r, _: r.update(completed_at='2026-10-06T20:02:01+00:00'))

    def test_journal_body_digest_tampering(self):
        self.inspect(lambda r, _: r['before']['installation/bin/native-installation-proof.exe'].update(base64='Zm9yZ2Vk'))

    def test_windows_journal_alias(self):
        self.inspect(lambda r, _: r['before'].update({'installation\\bin\\native-installation-proof.exe':
                                                    r['before']['installation/bin/native-installation-proof.exe']}))


class CensusAndDispositionControls(unittest.TestCase):
    def test_exact_fifty_positive_census(self):
        a.selection_census([{'id': r[0]} for r in a.selections()])

    def test_missing_duplicate_reordered_and_negative_promoted(self):
        valid = [{'id': r[0]} for r in a.selections()]
        for change in (lambda r: r.pop(), lambda r: r.__setitem__(10, r[9]),
                       lambda r: r.reverse(), lambda r: r[9].update(id='real-native-v5-package')):
            a.selection_census(valid); rows = copy.deepcopy(valid); change(rows)
            with self.assertRaises(ValueError):
                a.selection_census(rows)

    def test_rejection_phase_and_callbacks(self):
        row = {'exit': 2, 'result': {'status': 'rejected', 'phase': 'consumer-postcheck',
               'diagnostics': [{'code': 'ZRYNA-C4220'}], 'mutation': {'effective': True, 'before': {'file': 1}, 'after': {'file': 2}}}}
        self.assertEqual(a.disposition(row, 'consumer-postcheck'), 'rejected')
        for mutate in (lambda r: r.update(exit=0), lambda r: r['result'].update(phase='syntax-verification'),
                       lambda r: r['result'].update(diagnostics=[{'code': 'PROBE-CONSUMER'}]),
                       lambda r: r['result']['mutation'].update(after={'file': 1})):
            current = copy.deepcopy(row); mutate(current)
            with self.assertRaises(ValueError):
                a.disposition(current, 'consumer-postcheck')

    def test_os_prevention_is_blocked_and_not_rejection(self):
        row = {'exit': 0, 'before': {'file': 1}, 'after': {'file': 1}, 'result': {'mutation': {'effective': False,
                'prevention': {'errno': 32, 'original_tree_unchanged': True, 'exercised_rejection_credit': False}}}}
        self.assertEqual(a.disposition(row, 'source-capture'), 'blocked')
        for mutate in (lambda r: r.update(after={'file': 2}),
                       lambda r: r['result']['mutation']['prevention'].update(errno=2),
                       lambda r: r['result']['mutation']['prevention'].update(exercised_rejection_credit=True)):
            current = copy.deepcopy(row); mutate(current)
            with self.assertRaises(ValueError):
                a.disposition(current, 'source-capture')

    def test_source_extra_observation_is_not_forced_positive(self):
        row = {'exit': 2, 'result': {'status': 'rejected', 'phase': 'syntax-verification', 'diagnostics': [{'code': 'ZRYNA-P4004'}],
              'mutation': {'id': 'source-extra-file', 'effective': True, 'before': {}, 'after': {'extra': 1}}}}
        self.assertEqual(a.disposition(row, 'syntax-verification', True), 'observation')
        row['result']['diagnostics'][0]['code'] = 'foreign'
        with self.assertRaises(ValueError):
            a.disposition(row, 'syntax-verification', True)

    def test_duplicate_json_rejected(self):
        self.assertEqual(a.decode(b'{"id":1}'), {'id': 1})
        with self.assertRaises(ValueError):
            a.decode(b'{"id":1,"id":2}')

    def test_closed_file_missing_extra_and_hardlink(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary); (root / 'receipt.json').write_bytes(b'{}')
            a.closed_files(root, {'receipt.json'})
            for census in ({'receipt.json', 'missing'}, set()):
                with self.assertRaises(ValueError):
                    a.closed_files(root, census)
            original = Path.lstat
            def linked(path):
                info = original(path)
                return SimpleNamespace(st_mode=info.st_mode, st_nlink=2) if path.name == 'receipt.json' else info
            with mock.patch.object(Path, 'lstat', linked), self.assertRaises(ValueError):
                a.closed_files(root, {'receipt.json'})

    def test_plain_bytes_cannot_gain_initial_image_authority(self):
        with self.assertRaises(ValueError):
            a.image(b'modeled bytes' * 100, {'descriptor': 'a'*64, 'head': 'b'*40, 'tree': 'c'*40}, 'win32')


class AdditionalIndependentControls(unittest.TestCase):
    def test_recursive_typed_equality(self):
        original = {'files': [{'bytes': 1, 'sha256': 'a'*64}], 'exit': 0}
        self.assertTrue(a.exact(original, copy.deepcopy(original)))
        for changed in ({'files': [{'bytes': True, 'sha256': 'a'*64}], 'exit': 0},
                        {'files': [{'bytes': 1, 'sha256': 'a'*64}], 'exit': False}):
            self.assertFalse(a.exact(original, changed))

    def test_access_time_is_excluded_other_identity_fields_retained(self):
        from admission_build import stable
        row = dict(st_dev=1, st_ino=2, st_nlink=1, st_size=5, st_mtime_ns=6, st_ctime_ns=7, st_mode=0o100600)
        before = SimpleNamespace(**row, st_atime_ns=8)
        self.assertEqual(stable(before), stable(SimpleNamespace(**row, st_atime_ns=100)))
        for field in row:
            with self.subTest(field=field):
                changed = dict(row); changed[field] += 1
                self.assertNotEqual(stable(before), stable(SimpleNamespace(**changed, st_atime_ns=8)))

    def setup_journal(self):
        rows = {'.': tree_row(), 'file': tree_row(b'original')}
        return {'case': 'descriptor-symlink', 'status': 'blocked-pre-mutation-setup', 'baseline_passed': True,
                'probe_executed_after_mutation': False, 'before': rows, 'after': copy.deepcopy(rows),
                'original_tree_unchanged': True, 'error': {'type': 'PermissionError', 'errno': 13, 'winerror': 1314, 'message': 'denied'}}, rows

    def test_denied_setup_is_incomplete_not_exercised_rejection(self):
        j, previous = self.setup_journal()
        self.assertEqual(a.pre_setup(j, previous, 'descriptor-symlink'), 'blocked')

    def test_nonpermission_setup_error_is_not_os_prevention(self):
        j, previous = self.setup_journal(); a.pre_setup(j, previous, 'descriptor-symlink')
        j['error'].update(errno=2, winerror=2, type='FileNotFoundError')
        self.assertEqual(a.pre_setup(j, previous, 'descriptor-symlink'), 'setup_failed')

    def test_partial_setup_change_is_failure_not_prevention(self):
        j, previous = self.setup_journal(); a.pre_setup(j, previous, 'descriptor-symlink')
        j['after']['file'] = tree_row(b'changed'); j['original_tree_unchanged'] = False
        self.assertEqual(a.pre_setup(j, previous, 'descriptor-symlink'), 'setup_failed')
        j['original_tree_unchanged'] = True
        with self.assertRaises(ValueError):
            a.pre_setup(j, previous, 'descriptor-symlink')

    def test_setup_requires_actual_baseline_and_no_candidate(self):
        j, previous = self.setup_journal()
        for field in ('baseline_passed', 'probe_executed_after_mutation'):
            a.pre_setup(j, previous, 'descriptor-symlink'); changed = copy.deepcopy(j); changed[field] = not changed[field]
            with self.assertRaises(ValueError):
                a.pre_setup(changed, previous, 'descriptor-symlink')

    def test_runtime_setup_failure_not_rejection(self):
        record = {'exit': 3, 'result': {'status': 'setup-failed', 'phase': 'mutation-setup'}}
        self.assertEqual(a.disposition(record, 'source-capture'), 'setup_failed')
        record['result']['status'] = 'rejected'
        with self.assertRaises(ValueError):
            a.disposition(record, 'source-capture')

    def test_pre_mutation_exact_descriptor_bytes(self):
        original = b'{"protocols":[2,3,4]}\n'; path = 'installation/metadata/native-provider.json'
        before = {'.': tree_row(), path: tree_row(original)}
        after = {'.': tree_row(), path: tree_row(original.replace(b'[2,3,4]', b'[2,3,5]'))}
        a.pre_mutation('descriptor-byte-change', before, after)
        after[path] = tree_row(b'wrong other mutation')
        with self.assertRaises(ValueError):
            a.pre_mutation('descriptor-byte-change', before, after)

    def test_same_byte_replacement_and_coherent_wrong_journal_rejected(self):
        key = 'metadata/native-provider.json'; path = 'installation/' + key
        first = tree_row(b'original'); last = copy.deepcopy(first); last['inode'] = 4
        def own(row):
            return {'': {'bytes': 0, 'directory': True, 'file': False}, key: {'bytes': row['bytes'], 'file': True, 'directory': False,
                    'sha256': row['sha256'], 'inode': row['inode']}}
        record = {'before': {'installation': tree_row(), path: first}, 'after': {'installation': tree_row(), path: last,
                  'mutation-backups/original-file': copy.deepcopy(first)}}
        mutation = {'id': 'descriptor-replaced-identical-bytes', 'effective': True, 'before': own(first), 'after': own(last)}
        a.mutation_binding(mutation, record, mutation['id'], 'win32')
        mutation['after'][key]['sha256'] = 'a'*64
        with self.assertRaises(ValueError):
            a.mutation_binding(mutation, record, mutation['id'], 'win32')


    def test_graph_oracles_independently_match_existing_pure_serializer(self):
        root = Path(__file__).resolve().parents[3]
        spec = importlib.util.spec_from_file_location('source_case_fixture_encoder', Path(__file__).with_name('cases.py'))
        fixture = importlib.util.module_from_spec(spec); spec.loader.exec_module(fixture)
        for protocol in (2, 3, 4):
            self.assertEqual(a.summary(root, protocol), fixture.expected_summary(root, protocol))



def link_model(platform):
    pm = a.ntpath if platform == 'win32' else a.posixpath
    root = r'C:\cases\source-symlink-substitution\live' if platform == 'win32' else '/cases/source-symlink-substitution/live'
    original = tree_row(b'original'); link = tree_row(); link.update(kind='link', bytes=140, mode=511, inode=4,
                                                                  target=pm.join(root, 'mutation-backups', 'original-file'))
    entry = {'bytes': 140, 'file': False, 'directory': False, 'modified_ns': str(link['mtime_ns'])}
    if platform == 'linux':
        entry.update(mode=0o120777, device=1, inode=4, links=1, ctime=[0, link['ctime_ns']])
    else:
        entry.update(attributes=0x400, creation_time=116444736000000000,
                     last_write_time=116444736000000000 + link['mtime_ns'] // 100)
        link['mtime_ns'] = (entry['last_write_time'] - 116444736000000000) * 100
        entry['modified_ns'] = str(link['mtime_ns'])
    before = {'sources': tree_row(), 'sources/packages/app/main.zry': original}
    after = {'sources': tree_row(), 'sources/packages/app/main.zry': link,
             'mutation-backups/original-file': copy.deepcopy(original)}
    raw_before = {'': {'bytes': 0, 'file': False, 'directory': True},
                  'packages/app/main.zry': {'bytes': original['bytes'], 'sha256': original['sha256'], 'file': True, 'directory': False}}
    observation = {'substituted_path': pm.join(root, 'sources', 'packages', 'app', 'main.zry'),
                   'symlink_target': link['target'], 'path_entry': entry,
                   'full_tree_snapshot': 'unavailable: deliberate link is rejected by snapshot guard'}
    mutation = {'id': 'source-symlink-substitution', 'effective': True, 'before': raw_before, 'after': observation,
                'backup': link['target'], 'original_bytes_sha256': original['sha256']}
    record = {'before': before, 'after': after, 'argv': ['image', pm.join(root, 'sources'), pm.join(root, 'mutation-backups')]}
    return mutation, record


class JournalCheckpointCorrectionControls(unittest.TestCase):
    def reject(self, mutate, platform='linux'):
        mutation, record = link_model(platform)
        a.mutation_binding(mutation, record, mutation['id'], platform)
        mutate(mutation, record)
        with self.assertRaises((ValueError, KeyError, TypeError)):
            a.mutation_binding(mutation, record, mutation['id'], platform)

    def test_complete_python_tree_binds_sparse_rust_link_both_hosts(self):
        for platform in ('linux', 'win32'):
            m, r = link_model(platform); a.mutation_binding(m, r, m['id'], platform)

    def test_substituted_path_must_be_selected_source(self):
        self.reject(lambda m, r: m['after'].update(substituted_path='/foreign/main.zry'))

    def test_link_target_must_be_original_backup(self):
        self.reject(lambda m, r: m['after'].update(symlink_target='/foreign/backup'))

    def test_link_cannot_be_ordinary_file(self):
        self.reject(lambda m, r: r['after']['sources/packages/app/main.zry'].update(kind='file', sha256='a'*64, base64=''))

    def test_backup_original_bytes_required(self):
        self.reject(lambda m, r: m.update(original_bytes_sha256='a'*64))

    def test_sparse_after_cannot_escape_to_other_mutation(self):
        self.reject(lambda m, r: m.update(id='source-byte-change'))

    def test_unix_link_inode_crossbinding(self):
        self.reject(lambda m, r: m['after']['path_entry'].update(inode=999))

    def test_windows_link_write_time_crossbinding(self):
        self.reject(lambda m, r: m['after']['path_entry'].update(last_write_time=116444736000000001), 'win32')

    def test_sparse_after_unknown_fields_rejected(self):
        self.reject(lambda m, r: m['after'].update(trusted=True))

    def test_other_file_mutation_cannot_hide_in_link_case(self):
        self.reject(lambda m, r: r['after'].update({'sources/foreign.zry': tree_row(b'foreign')}))

    def test_forged_rust_identity_cannot_promote_unchanged_python_target(self):
        raw = tree_row(b'original'); target = 'installation/metadata/native-provider.json'
        whole = {'installation': tree_row(), target: raw}
        own = {'': {'bytes': 0, 'file': False, 'directory': True}, 'metadata/native-provider.json':
               {'bytes': raw['bytes'], 'file': True, 'directory': False, 'sha256': raw['sha256'], 'inode': 3}}
        mutation = {'id': 'descriptor-replaced-identical-bytes', 'effective': True, 'before': own, 'after': copy.deepcopy(own)}
        mutation['after']['metadata/native-provider.json']['inode'] = 999
        record = {'before': whole, 'after': copy.deepcopy(whole)}
        record['after']['mutation-backups/original-file'] = copy.deepcopy(raw)
        with self.assertRaises(ValueError):
            a.mutation_binding(mutation, record, mutation['id'], 'linux')

    def test_parent_replacement_binds_actual_parent_and_backup(self):
        old = tree_row(); new = copy.deepcopy(old); new['inode'] = 4
        own = {'': {'bytes': 999, 'file': False, 'directory': True, 'inode': 3}}
        mutation = {'id': 'installation-parent-replaced-identical-tree', 'effective': True, 'before': own, 'after': copy.deepcopy(own)}
        mutation['after']['']['inode'] = 4
        record = {'before': {'installation': old}, 'after': {'installation': new, 'mutation-backups/original-tree': copy.deepcopy(old)}}
        a.mutation_binding(mutation, record, mutation['id'], 'linux')
        record['after']['installation']['inode'] = 3
        with self.assertRaises(ValueError):
            a.mutation_binding(mutation, record, mutation['id'], 'linux')

    def test_overlapping_file_identity_and_prior_lock_checkpoint(self):
        row = tree_row(b'original')
        own = {'file': True, 'directory': False, 'device': 1, 'inode': 3, 'links': 1, 'mode': 0o100600,
               'modified_ns': '4', 'ctime': [0, 5]}
        a.file_overlap(own, row, 'linux')
        changed = copy.deepcopy(own); changed['inode'] = 999
        with self.assertRaises(ValueError):
            a.file_overlap(changed, row, 'linux')
        a.file_overlap(changed, row, 'linux', prior_update_lock=True)
        changed['links'] = 2
        with self.assertRaises(ValueError):
            a.file_overlap(changed, row, 'linux', prior_update_lock=True)


class UnitEnvironmentCorrectionControls(unittest.TestCase):
    def test_full_original_recipe_positive_then_all_substitutions_rejected(self):
        from admission_build import unit_contract, windows_environment
        p = profile(); a.profile_contract(p)
        origin = r'C:\checkout\zryna'; output = r'C:\proof'; target = r'C:\target'
        env = windows_environment(p, output, target)
        env.update(CARGO_PROFILE_TEST_DEBUG='0', CARGO_PROFILE_TEST_OPT_LEVEL='1', CARGO_PROFILE_TEST_CODEGEN_UNITS='16')
        row = {'cwd': origin, 'selected_environment': env, 'argv': [p['tools']['cargo']['path'], 'test', '--locked', '--offline',
               '-p', 'zryna-driver', '--features', 'native-provider-internal', '--lib', '--message-format=json',
               'distribution::native_installation::descriptor::tests', '--', '--nocapture']}
        for mutate in (lambda r: r['selected_environment'].update(RUSTFLAGS='--cfg forged'),
                       lambda r: r['selected_environment'].update(RUSTC_WRAPPER=r'C:\foreign.exe'),
                       lambda r: r['selected_environment'].update(CARGO_HOME=r'C:\foreign'),
                       lambda r: r['selected_environment'].update(RUSTUP_TOOLCHAIN='stable'),
                       lambda r: r.update(cwd=r'C:\foreign'),
                       lambda r: r['selected_environment'].update(CARGO_TARGET_DIR=r'C:\foreign'),
                       lambda r: r['selected_environment'].update(TEMP=r'C:\foreign')):
            unit_contract(row, p, origin, output, target)
            changed = copy.deepcopy(row); mutate(changed)
            with self.assertRaises(ValueError):
                unit_contract(changed, p, origin, output, target)



if __name__ == '__main__':
    parser = argparse.ArgumentParser()
    for name in ('root', 'proof', 'head', 'platform', 'run-id', 'run-attempt'):
        parser.add_argument('--' + name)
    args, remaining = parser.parse_known_args()
    supplied = vars(args)
    if any(supplied.values()):
        parser.error('original packet requires --root --proof --head --platform') if not all(supplied[k] for k in ('root', 'proof', 'head', 'platform')) else None
        from admission_original_controls import OriginalPacketControls
        OriginalPacketControls.inputs = supplied
    unittest.main(argv=[sys.argv[0], *remaining], verbosity=2)
