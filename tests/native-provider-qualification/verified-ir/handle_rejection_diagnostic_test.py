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


class HandleRejectionDiagnosticControls(unittest.TestCase):
    fields = ('device', 'inode', 'file_type', 'bytes', 'mtime_ns', 'ctime_ns', 'links')

    def metadata(self, **changes):
        values = dict(st_dev=7, st_ino=19, st_mode=stat.S_IFREG | 0o600,
                      st_size=39, st_mtime_ns=17, st_ctime_ns=29, st_nlink=1)
        if c.sys.platform == 'win32':
            values['st_birthtime_ns'] = changes.get('st_ctime_ns', 29)
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

    def test_diagnostic_birthtime_observation_and_platform_comparison(self):
        opened, path = self.metadata(st_birthtime_ns=7), self.metadata(st_birthtime_ns=8)
        if c.sys.platform == 'win32':
            with self.assertRaises(ValueError):
                c.require_open_handle(opened, path)
        else:
            self.assertIsNone(c.require_open_handle(opened, path))
        path.st_ctime_ns = 30
        report = self.rejected(opened, path)
        self.assertEqual(report['observed_birthtime_ns'], {'opened': 7, 'path': 8})
        self.assertEqual(report['differing_fields'], ['ctime_ns'])

    def test_diagnostic_birthtime_absence_and_oversize_remain_bounded(self):
        opened, path = self.metadata(), self.metadata(st_ino=20)
        for info in (opened, path):
            info.__dict__.pop('st_birthtime_ns', None)
        report = self.rejected(opened, path)
        self.assertEqual(report['observed_birthtime_ns'], {'opened': 'unavailable', 'path': 'unavailable'})
        report = self.rejected(self.metadata(st_birthtime_ns=1 << 10000), path)
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
