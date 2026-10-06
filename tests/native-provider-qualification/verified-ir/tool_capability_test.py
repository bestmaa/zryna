"""Modeled capability guards; no compiler, provider or executable tool is run."""
from contextlib import contextmanager
import copy
import json
from pathlib import Path
import stat
import tempfile
from types import SimpleNamespace
import unittest
from unittest import mock
import tool_capability as c


class ToolCapabilityControls(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory(prefix='tool-capability-controls-')
        self.base = Path(self.temporary.name)
        self.tool = self.base / 'rustup'
        self.tool.write_bytes(b'modeled tool bytes; never execute\n')
        self.baseline = c.snapshot(str(self.tool))
        c.validate(self.baseline, c.sys.platform, live=True)

    def tearDown(self):
        self.temporary.cleanup()

    @contextmanager
    def aliases(self, targets):
        # No host symlink privilege or Windows skip: model lstat link metadata only.
        original = c.entry
        for path in targets:
            if not Path(path).exists():
                Path(path).write_bytes(b'non-executable alias placeholder')

        def inspect(path):
            row = original(path)
            if path in targets:
                row.update(mode=stat.S_IFLNK | 0o777, bytes=len(str(targets[path])),
                           link_target=str(targets[path]), links=1)
            return row

        with mock.patch.object(c, 'entry', side_effect=inspect):
            yield

    def reject(self, mutate, live=False):
        # A valid real-file capability is admitted before every hostile mutation.
        c.validate(self.baseline, c.sys.platform, live=True)
        changed = copy.deepcopy(self.baseline)
        mutate(changed)
        with self.assertRaises((ValueError, OSError, KeyError, TypeError)):
            c.validate(changed, c.sys.platform, live=live)

    def windows(self, junction=False):
        size = self.baseline['bytes']
        def row(path, mode, target=None, tag=None):
            return dict(path=path, mode=mode, device=1, inode=100 + len(path),
                        bytes=size if stat.S_ISREG(mode) else 0, mtime_ns=4,
                        ctime_ns=5, links=1, file_attributes=0x400 if tag else 0,
                        reparse_tag=tag, link_target=target)
        entries = {name: row(name, stat.S_IFDIR | 0o700)
                   for name in ('C:\\', 'C:\\tools')}
        if junction:
            entries['C:\\alias'] = row('C:\\alias', stat.S_IFDIR | 0o700,
                                         '\\\\?\\C:\\tools', c.JUNCTION)
            selected = 'C:\\alias\\cargo.exe'
        else:
            entries['C:\\cargo.exe'] = row('C:\\cargo.exe', stat.S_IFLNK | 0o777,
                                             '\\\\?\\C:\\tools\\cargo.exe', c.SYMLINK)
            selected = 'C:\\cargo.exe'
        entries['C:\\tools\\cargo.exe'] = row('C:\\tools\\cargo.exe', stat.S_IFREG | 0o755)
        resolved, chain = c.walk(selected, 'win32', entries.__getitem__)
        _, canonical = c.walk(resolved, 'win32', entries.__getitem__)
        result = dict(format=c.FORMAT, platform='win32', path=selected, resolved_path=resolved,
                      bytes=size, sha256=self.baseline['sha256'], resolution=chain,
                      canonical_components=canonical)
        c.validate(result, 'win32')
        return result

    def test_positive_regular_live_and_archived(self):
        self.assertEqual(c.validate(self.baseline, c.sys.platform), self.baseline)
        self.assertTrue(c.same(self.baseline, c.snapshot(str(self.tool))))

    def test_positive_proxy_preserves_lexical_cargo_path(self):
        proxy = str(self.base / 'cargo')
        with self.aliases({proxy: 'rustup'}):
            row = c.snapshot(proxy)
            self.assertEqual(row['path'], proxy)
            self.assertEqual(row['resolved_path'], str(self.tool))
            self.assertTrue(c.same(row, c.snapshot(proxy)))
            c.validate(row, c.sys.platform, live=True)

    def test_positive_complete_multihop_resolution(self):
        proxy, middle = str(self.base / 'cargo'), str(self.base / 'middle')
        with self.aliases({proxy: 'middle', middle: 'rustup'}):
            row = c.snapshot(proxy)
            links = [item['path'] for item in row['resolution'] if c.linked(item)]
            self.assertEqual(links, [proxy, middle])
            c.validate(row, c.sys.platform, live=True)

    def test_positive_linked_parent_and_relative_parent_target(self):
        real = self.base / 'tools'
        real.mkdir()
        target = real / 'cargo'
        target.write_bytes(self.tool.read_bytes())
        parent = self.base / 'aliases'
        parent.mkdir()
        alias = str(parent / 'cargo')
        with self.aliases({alias: '../tools/cargo'}):
            row = c.snapshot(alias)
            self.assertEqual(row['resolved_path'], str(target))
            self.assertIn(str(self.base), [part['path'] for part in row['resolution']])

    def test_positive_parent_alias_records_complete_target(self):
        real, alias = self.base / 'tools', self.base / 'linked-parent'
        real.mkdir()
        alias.mkdir()
        tool = real / 'cargo'
        tool.write_bytes(self.tool.read_bytes())
        with self.aliases({str(alias): 'tools'}):
            row = c.snapshot(str(alias / 'cargo'))
            self.assertEqual(row['resolved_path'], str(tool))
            self.assertIn(str(alias), [part['path'] for part in row['resolution'] if c.linked(part)])

    def test_windows_symlink_extended_target_positive(self):
        self.assertTrue(c.same(self.windows(), self.windows()))

    def test_windows_junction_extended_target_positive(self):
        self.assertTrue(c.same(self.windows(True), self.windows(True)))

    def test_windows_handle_execute_bits_positive(self):
        tool = self.base / 'modeled.exe'
        tool.write_bytes(self.tool.read_bytes())
        lstat = Path.lstat
        def metadata(path):
            original = lstat(path)
            if path != tool:
                return original
            fields = {name: getattr(original, name) for name in dir(original) if name.startswith('st_')}
            fields['st_mode'] |= 0o111
            return SimpleNamespace(**fields)
        with mock.patch.object(Path, 'lstat', metadata):
            row = c.snapshot(str(tool))
            self.assertTrue(c.same(row, c.snapshot(str(tool))))

    def test_missing_and_extra_row_fields(self):
        for mutation in (lambda row: row.pop('resolution'), lambda row: row.update(source_identity='forged')):
            with self.subTest(mutation=mutation):
                self.reject(mutation)

    def test_forged_digest_rejected_live(self):
        self.reject(lambda row: row.update(sha256='0' * 64), live=True)

    def test_typed_size_and_digest(self):
        for value in (True, -1, c.MAX_BYTES + 1, '10'):
            with self.subTest(value=value):
                self.reject(lambda row: row.update(bytes=value))
        for value in ('F' * 64, '0' * 63, False):
            with self.subTest(value=value):
                self.reject(lambda row: row.update(sha256=value))

    def test_platform_and_format_are_closed(self):
        self.reject(lambda row: row.update(platform='darwin'))
        self.reject(lambda row: row.update(format=c.FORMAT + '-forged'))

    def test_absolute_normalized_path_only(self):
        for value in ('rustup', str(self.base / 'x') + '/../rustup', str(self.base) + '//rustup',
                      str(self.tool) + '\0'):
            with self.subTest(value=value):
                self.reject(lambda row: row.update(path=value))

    def test_missing_duplicate_reordered_component_census(self):
        mutations = [lambda row: row['resolution'].pop(0),
                     lambda row: row['resolution'].append(copy.deepcopy(row['resolution'][-1])),
                     lambda row: row['resolution'].reverse(),
                     lambda row: row.update(resolution=[])]
        for mutate in mutations:
            with self.subTest(mutate=mutate):
                self.reject(mutate)

    def test_omitted_intermediate_alias_rejected(self):
        proxy, middle = str(self.base / 'cargo'), str(self.base / 'middle')
        with self.aliases({proxy: 'middle', middle: 'rustup'}):
            row = c.snapshot(proxy)
            c.validate(row, c.sys.platform, live=True)
            row['resolution'] = [part for part in row['resolution'] if part['path'] != middle]
            with self.assertRaises(ValueError):
                c.validate(row, c.sys.platform)

    def test_alias_retarget_identical_bytes_is_changed_identity(self):
        proxy = str(self.base / 'cargo')
        twin = self.base / 'twin'
        twin.write_bytes(self.tool.read_bytes())
        targets = {proxy: 'rustup'}
        with self.aliases(targets):
            before = c.snapshot(proxy)
            c.validate(before, c.sys.platform, live=True)
            targets[proxy] = 'twin'
            after = c.snapshot(proxy)
            self.assertEqual(before['sha256'], after['sha256'])
            self.assertFalse(c.same(before, after))
            with self.assertRaises(ValueError):
                c.validate(before, c.sys.platform, live=True)

    def test_intermediate_replacement_same_final_target_is_detected(self):
        proxy, middle = str(self.base / 'cargo'), str(self.base / 'middle')
        with self.aliases({proxy: 'middle', middle: 'rustup'}):
            before = c.snapshot(proxy)
            c.validate(before, c.sys.platform, live=True)
            original = c.entry
            def replaced(path):
                row = original(path)
                if path == middle:
                    row['inode'] += 1
                return row
            with mock.patch.object(c, 'entry', side_effect=replaced):
                after = c.snapshot(proxy)
                self.assertEqual(before['resolved_path'], after['resolved_path'])
                self.assertFalse(c.same(before, after))
                with self.assertRaises(ValueError):
                    c.validate(before, c.sys.platform, live=True)

    def test_forged_coherent_canonical_capability_rejected_live(self):
        proxy = str(self.base / 'cargo')
        other = self.base / 'other'
        other.write_bytes(b'coherently forged canonical bytes')
        targets = {proxy: 'other'}
        with self.aliases(targets):
            forged = c.snapshot(proxy)
            c.validate(forged, c.sys.platform, live=True)
            targets[proxy] = 'rustup'
            valid = c.snapshot(proxy)
            c.validate(valid, c.sys.platform, live=True)
            c.validate(forged, c.sys.platform)  # Archive structure is not host attestation.
            self.assertFalse(c.same(forged, valid))
            with self.assertRaises(ValueError):
                c.validate(forged, c.sys.platform, live=True)

    def test_tool_bytes_changed_rejected_live(self):
        c.validate(self.baseline, c.sys.platform, live=True)
        self.tool.write_bytes(b'changed bytes after bound tool selection')
        with self.assertRaises(ValueError):
            c.validate(self.baseline, c.sys.platform, live=True)

    def test_canonical_file_replacement_same_bytes_rejected(self):
        c.validate(self.baseline, c.sys.platform, live=True)
        replacement = self.base / 'replacement'
        replacement.write_bytes(self.tool.read_bytes())
        replacement.replace(self.tool)
        self.assertFalse(c.same(self.baseline, c.snapshot(str(self.tool))))

    def test_directory_content_timestamps_do_not_replace_capability(self):
        c.validate(self.baseline, c.sys.platform, live=True)
        changed = copy.deepcopy(self.baseline)
        for field in ('resolution', 'canonical_components'):
            for component in changed[field]:
                if stat.S_ISDIR(component['mode']):
                    component.update(bytes=component['bytes'] + 1, mtime_ns=999, ctime_ns=999, links=9)
        self.assertTrue(c.same(self.baseline, changed))

    def test_directory_identity_change_is_rejected(self):
        def change(row):
            row['resolution'][0]['inode'] += 1
        self.reject(change)

    def test_component_permissions_and_identity_are_typed(self):
        for field in ('mode', 'inode', 'device', 'mtime_ns', 'links', 'file_attributes'):
            with self.subTest(field=field):
                self.reject(lambda row: row['resolution'][0].update({field: True}))

    def test_special_canonical_type_is_rejected(self):
        self.reject(lambda row: row['canonical_components'][-1].update(mode=stat.S_IFIFO | 0o600))

    def test_symlink_cycles_rejected(self):
        first, second = str(self.base / 'first'), str(self.base / 'second')
        c.validate(self.baseline, c.sys.platform, live=True)
        with self.aliases({first: 'second', second: 'first'}):
            with self.assertRaisesRegex(ValueError, 'cycle'):
                c.snapshot(first)

    def test_dangling_alias_rejected(self):
        proxy = str(self.base / 'cargo')
        c.validate(self.baseline, c.sys.platform, live=True)
        with self.aliases({proxy: 'missing'}):
            with self.assertRaises(FileNotFoundError):
                c.snapshot(proxy)

    def test_alias_link_budget(self):
        targets = {str(self.base / ('link-' + str(index))): 'link-' + str(index + 1)
                   for index in range(c.MAX_LINKS + 1)}
        targets[str(self.base / ('link-' + str(c.MAX_LINKS)))] = 'rustup'
        c.validate(self.baseline, c.sys.platform, live=True)
        with self.aliases(targets):
            with self.assertRaisesRegex(ValueError, 'link bound'):
                c.snapshot(str(self.base / 'link-0'))

    def test_windows_unknown_reparse_tag_rejected(self):
        row = self.windows(True)
        next(part for part in row['resolution'] if c.linked(part))['reparse_tag'] = 0x8000001B
        with self.assertRaisesRegex(ValueError, 'reparse tag'):
            c.validate(row, 'win32')

    def test_windows_forged_reparse_type_rejected(self):
        row = self.windows(True)
        next(part for part in row['resolution'] if c.linked(part))['mode'] = stat.S_IFREG | 0o600
        with self.assertRaises(ValueError):
            c.validate(row, 'win32')

    def test_windows_ambiguous_link_target_rejected(self):
        for value in ('C:relative', '\\relative'):
            row = self.windows()
            next(part for part in row['resolution'] if c.linked(part))['link_target'] = value
            with self.subTest(value=value), self.assertRaises(ValueError):
                c.validate(row, 'win32')

    def test_unknown_live_mode_and_foreign_host_rejected(self):
        c.validate(self.baseline, c.sys.platform, live=True)
        with self.assertRaises(ValueError):
            c.validate(self.baseline, c.sys.platform, live=1)
        if c.sys.platform == 'linux':
            row = self.windows()
            with self.assertRaises(ValueError):
                c.validate(row, 'win32', live=True)

    def test_read_uses_unchanged_artifact_guard(self):
        c.validate(self.baseline, c.sys.platform, live=True)
        with mock.patch.object(c.ir, 'read_bounded', side_effect=ValueError('strict artifact link guard')):
            with self.assertRaisesRegex(ValueError, 'strict artifact link guard'):
                c.snapshot(str(self.tool))

    def test_identity_change_during_snapshot_rejected(self):
        c.validate(self.baseline, c.sys.platform, live=True)
        read = c.read_canonical
        def change(path):
            result = read(path)
            self.tool.write_bytes(b'changed during snapshot')
            return result
        with mock.patch.object(c, 'read_canonical', side_effect=change):
            with self.assertRaises(ValueError):
                c.snapshot(str(self.tool))

    def test_same_bytes_replacement_at_final_read_rejected(self):
        c.validate(self.baseline, c.sys.platform, live=True)
        read, calls = c.read_canonical, []
        def replace(path):
            result = read(path)
            calls.append(path)
            if len(calls) == 2:
                replacement = self.base / 'replacement'
                replacement.write_bytes(self.tool.read_bytes())
                replacement.replace(self.tool)
            return result
        with mock.patch.object(c, 'read_canonical', side_effect=replace):
            with self.assertRaisesRegex(ValueError, 'resolution identity changed'):
                c.snapshot(str(self.tool))


class HandleRejectionDiagnosticControls(unittest.TestCase):
    fields = ('device', 'inode', 'file_type', 'bytes', 'mtime_ns', 'ctime_ns', 'links')

    def metadata(self, **changes):
        values = dict(st_dev=7, st_ino=19, st_mode=stat.S_IFREG | 0o600,
                      st_size=39, st_mtime_ns=17, st_ctime_ns=29, st_nlink=1)
        return SimpleNamespace(**dict(values, **changes))

    def rejected(self, opened, path):
        self.assertIsNone(c.require_open_handle(self.metadata(), self.metadata()))
        with self.assertRaises(ValueError) as raised:
            c.require_open_handle(opened, path)
        message = str(raised.exception)
        self.assertLess(len(message), 4096)
        prefix = 'stable canonical open handle; '
        self.assertTrue(message.startswith(prefix))
        report = json.loads(message[len(prefix):])
        self.assertEqual(report['format'], 'zryna.tool-handle-rejection.v1')
        self.assertEqual(set(report['opened']), set(self.fields))
        self.assertEqual(set(report['path']), set(self.fields))
        self.assertEqual(report['byte_limit'], c.MAX_BYTES)
        return report

    def test_diagnostic_equal_modeled_metadata_accepted(self):
        self.assertIsNone(c.require_open_handle(self.metadata(), self.metadata()))

    def test_diagnostic_actual_same_open_file_metadata_accepted(self):
        with tempfile.TemporaryFile() as source:
            source.write(b'metadata positive; never execute')
            source.flush()
            before, after = c.os.fstat(source.fileno()), c.os.fstat(source.fileno())
            self.assertIsNone(c.require_open_handle(before, after))

    def test_diagnostic_actual_runtime_context_is_reported(self):
        report = self.rejected(self.metadata(), self.metadata(st_ino=20))
        self.assertEqual(report['platform'], c.sys.platform)
        self.assertEqual(report['python_implementation'], c.sys.implementation.name)
        self.assertEqual(report['python_version'], list(c.sys.version_info[:3]))

    def test_diagnostic_each_original_tuple_field_rejects_exact_values(self):
        changes = [('device', 'st_dev', 8), ('inode', 'st_ino', 20),
                   ('file_type', 'st_mode', stat.S_IFDIR | 0o600),
                   ('bytes', 'st_size', 40), ('mtime_ns', 'st_mtime_ns', 18),
                   ('ctime_ns', 'st_ctime_ns', 30), ('links', 'st_nlink', 2)]
        for field, attribute, value in changes:
            with self.subTest(field=field):
                report = self.rejected(self.metadata(), self.metadata(**{attribute: value}))
                expected = stat.S_IFMT(value) if field == 'file_type' else value
                self.assertEqual(report['differing_fields'], [field])
                self.assertEqual(report['path'][field], expected)
                self.assertEqual(report['opened'][field], dict(zip(self.fields, c.stable_handle(self.metadata())))[field])
                self.assertTrue(report['regular_handle'])
                self.assertTrue(report['within_byte_limit'])
                self.assertFalse(report['metadata_equal'])

    def test_diagnostic_multiple_differences_follow_original_field_order(self):
        report = self.rejected(self.metadata(), self.metadata(st_ctime_ns=30, st_ino=20, st_nlink=2))
        self.assertEqual(report['differing_fields'], ['inode', 'ctime_ns', 'links'])

    def test_diagnostic_equal_file_type_still_rejects_nonregular_handle(self):
        info = self.metadata(st_mode=stat.S_IFIFO | 0o600)
        report = self.rejected(info, info)
        self.assertFalse(report['regular_handle'])
        self.assertTrue(report['metadata_equal'])
        self.assertEqual(report['differing_fields'], [])

    def test_diagnostic_equal_size_still_rejects_original_byte_limit(self):
        info = self.metadata(st_size=c.MAX_BYTES + 1)
        report = self.rejected(info, info)
        self.assertTrue(report['regular_handle'])
        self.assertFalse(report['within_byte_limit'])
        self.assertTrue(report['metadata_equal'])
        self.assertEqual(report['opened']['bytes'], c.MAX_BYTES + 1)

    def test_diagnostic_noninteger_values_never_expand_or_call_repr(self):
        class UnsafeRepr:
            def __repr__(self):
                raise AssertionError('diagnostics must not render arbitrary objects')
        for value in (UnsafeRepr(), 'secret-unbounded-stat-value' * 10000, True):
            with self.subTest(kind=type(value).__name__):
                report = self.rejected(self.metadata(st_ino=value), self.metadata())
                self.assertEqual(report['opened']['inode'], 'invalid-or-unbounded-integer')
                self.assertEqual(report['differing_fields'], ['inode'])

    def test_diagnostic_integer_magnitude_report_is_bounded(self):
        for value in (1 << 128, -(1 << 128), 1 << 10000):
            with self.subTest(sign=value > 0):
                report = self.rejected(self.metadata(st_ctime_ns=value), self.metadata())
                self.assertEqual(report['opened']['ctime_ns'], 'invalid-or-unbounded-integer')
                self.assertEqual(report['differing_fields'], ['ctime_ns'])

    def test_diagnostic_malformed_context_is_bounded_without_new_acceptance_rule(self):
        with mock.patch.object(c.sys, 'platform', 'bad\nplatform' * 10000), \
                mock.patch.object(c.sys, 'implementation', SimpleNamespace(name='BAD\n' * 10000)), \
                mock.patch.object(c.sys, 'version_info', (1 << 10000, object(), True)):
            report = self.rejected(self.metadata(), self.metadata(st_ino=20))
        self.assertEqual(report['platform'], 'unknown')
        self.assertEqual(report['python_implementation'], 'unknown')
        self.assertEqual(report['python_version'], ['invalid-or-unbounded-integer'] * 3)

    def test_diagnostic_birthtime_observation_does_not_change_comparison(self):
        opened, path = self.metadata(st_birthtime_ns=7), self.metadata(st_birthtime_ns=8)
        self.assertIsNone(c.require_open_handle(opened, path))
        path.st_ctime_ns = 30
        report = self.rejected(opened, path)
        self.assertEqual(report['observed_birthtime_ns'], {'opened': 7, 'path': 8})
        self.assertEqual(report['differing_fields'], ['ctime_ns'])

    def test_diagnostic_birthtime_absence_and_oversize_remain_bounded(self):
        report = self.rejected(self.metadata(), self.metadata(st_ino=20))
        self.assertEqual(report['observed_birthtime_ns'], {'opened': 'unavailable', 'path': 'unavailable'})
        report = self.rejected(self.metadata(st_birthtime_ns=1 << 10000), self.metadata(st_ino=20))
        self.assertEqual(report['observed_birthtime_ns'], {'opened': 'invalid-or-unbounded-integer', 'path': 'unavailable'})

    def test_diagnostic_read_canonical_uses_the_same_rejection_guard(self):
        raw = b'modeled ordinary tool bytes; never execute'
        opened = self.metadata(st_size=len(raw))
        path = self.metadata(st_size=len(raw), st_ctime_ns=30)
        @contextmanager
        def stream(*args):
            yield SimpleNamespace(fileno=lambda: 7, read=lambda limit: raw)
        row = dict(mode=stat.S_IFREG | 0o600, file_attributes=0, link_target=None)
        with mock.patch.object(c, 'entry', return_value=row), \
                mock.patch.object(c.ir, 'read_bounded', return_value=raw), \
                mock.patch.object(c.os, 'open', return_value=7), \
                mock.patch.object(c.os, 'fdopen', side_effect=stream), \
                mock.patch.object(c.os, 'fstat', return_value=opened), \
                mock.patch.object(Path, 'lstat', return_value=path), \
                mock.patch.object(c, 'require_open_handle', wraps=c.require_open_handle) as guard:
            with self.assertRaisesRegex(ValueError, 'stable canonical open handle;') as raised:
                c.read_canonical('modeled-unexecuted-tool')
        guard.assert_called_once_with(opened, path)
        report = json.loads(str(raised.exception).split('; ', 1)[1])
        self.assertEqual(report['differing_fields'], ['ctime_ns'])
        self.assertEqual((report['opened']['ctime_ns'], report['path']['ctime_ns']), (29, 30))


if __name__ == '__main__':
    unittest.main(verbosity=2)
