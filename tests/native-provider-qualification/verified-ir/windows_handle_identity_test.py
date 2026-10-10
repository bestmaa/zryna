"""Cross-API Windows identity guards; modeled stats, no tool execution."""
import hashlib
import json
from pathlib import Path
import stat
import tempfile
from types import SimpleNamespace
import unittest
from unittest import mock
import tool_capability as c


class WindowsHandleIdentityControls(unittest.TestCase):
    def row(self, **changes):
        fields = dict(st_dev=7, st_ino=19, st_mode=stat.S_IFREG | 0o600,
                      st_size=4, st_mtime_ns=200, st_ctime_ns=300, st_nlink=1,
                      st_birthtime_ns=100)
        fields.update(changes)
        return SimpleNamespace(**fields)

    def windows_pair(self):
        opened = self.row()
        path = self.row(st_ctime_ns=100, st_mode=stat.S_IFREG | 0o755)
        return opened, path

    def admitted_windows(self):
        opened, path = self.windows_pair()
        self.assertNotEqual(c.stable_handle(opened), c.stable_handle(path))
        self.assertEqual(c.canonical_identity(opened), c.canonical_identity(path))
        self.assertIsNone(c.require_open_handle(opened, path))
        return opened, path

    def report(self, opened, path):
        with self.assertRaises(ValueError) as error:
            c.require_open_handle(opened, path)
        prefix = 'stable canonical open handle; '
        self.assertTrue(str(error.exception).startswith(prefix))
        result = json.loads(str(error.exception)[len(prefix):])
        self.assertFalse(result['canonical_identity_equal'])
        self.assertLess(len(str(error.exception)), 4096)
        return result

    def test_windows_creation_time_cross_api_positive(self):
        with mock.patch.object(c.sys, 'platform', 'win32'):
            self.admitted_windows()

    def test_every_windows_comparable_identity_field_rejects(self):
        with mock.patch.object(c.sys, 'platform', 'win32'):
            self.admitted_windows()
            for field, value in [('st_dev', 8), ('st_ino', 20),
                                 ('st_mode', stat.S_IFDIR | 0o755), ('st_size', 5),
                                 ('st_mtime_ns', 201), ('st_nlink', 2)]:
                with self.subTest(field=field):
                    opened, path = self.windows_pair()
                    setattr(path, field, value)
                    self.assertNotEqual(c.canonical_identity(opened), c.canonical_identity(path))
                    self.report(opened, path)

    def test_windows_birthtime_mismatch_rejects_even_raw_equal(self):
        with mock.patch.object(c.sys, 'platform', 'win32'):
            self.admitted_windows()
            opened, path = self.row(), self.row(st_birthtime_ns=101)
            self.assertEqual(c.stable_handle(opened), c.stable_handle(path))
            report = self.report(opened, path)
            self.assertTrue(report['metadata_equal'])
            self.assertEqual(report['canonical_time_field'], 'birthtime_ns')

    def test_missing_windows_birthtime_fails_closed(self):
        with mock.patch.object(c.sys, 'platform', 'win32'):
            self.admitted_windows()
            for missing in ('opened', 'path', 'both'):
                with self.subTest(missing=missing):
                    opened, path = self.windows_pair()
                    if missing in ('opened', 'both'):
                        del opened.st_birthtime_ns
                    if missing in ('path', 'both'):
                        del path.st_birthtime_ns
                    self.report(opened, path)
                    if missing in ('opened', 'both'):
                        self.assertIsNone(c.canonical_identity(opened))
                    if missing in ('path', 'both'):
                        self.assertIsNone(c.canonical_identity(path))

    def test_invalid_windows_birthtime_fails_closed_on_both_sides(self):
        with mock.patch.object(c.sys, 'platform', 'win32'):
            self.admitted_windows()
            for value in (None, True, False, '100', 100.0, 1 << 128, -(1 << 128)):
                for side in ('opened', 'path', 'both'):
                    with self.subTest(value=value, side=side):
                        opened, path = self.windows_pair()
                        if side in ('opened', 'both'):
                            opened.st_birthtime_ns = value
                        if side in ('path', 'both'):
                            path.st_birthtime_ns = value
                        self.report(opened, path)

    def test_bounded_exact_birthtime_no_tolerance(self):
        with mock.patch.object(c.sys, 'platform', 'win32'):
            self.admitted_windows()
            for birth in (0, -1, (1 << 128) - 1, -(1 << 128) + 1):
                with self.subTest(birth=birth):
                    opened, path = self.windows_pair()
                    opened.st_birthtime_ns = path.st_birthtime_ns = birth
                    self.assertIsNone(c.require_open_handle(opened, path))
                    path.st_birthtime_ns = birth - 1
                    self.report(opened, path)

    def test_linux_uses_raw_seven_fields_and_rejects_ctime(self):
        with mock.patch.object(c.sys, 'platform', 'linux'):
            opened, path = self.row(), self.row()
            self.assertEqual(c.canonical_identity(opened), c.stable_handle(opened))
            self.assertIsNone(c.require_open_handle(opened, path))
            path.st_ctime_ns += 1
            report = self.report(opened, path)
            self.assertEqual(report['canonical_time_field'], 'ctime_ns')
            self.assertEqual(report['differing_fields'], ['ctime_ns'])

    def test_linux_does_not_substitute_birthtime(self):
        with mock.patch.object(c.sys, 'platform', 'linux'):
            opened, path = self.row(), self.row(st_birthtime_ns=999)
            self.assertIsNone(c.require_open_handle(opened, path))
            del opened.st_birthtime_ns
            self.assertIsNone(c.require_open_handle(opened, path))
            path.st_ctime_ns += 1
            self.report(opened, path)

    def test_windows_permission_bits_allowed_file_type_preserved(self):
        with mock.patch.object(c.sys, 'platform', 'win32'):
            self.admitted_windows()
            opened, path = self.windows_pair()
            opened.st_mode = stat.S_IFREG | 0o400
            path.st_mode = stat.S_IFREG | 0o777
            self.assertIsNone(c.require_open_handle(opened, path))
            path.st_mode = stat.S_IFLNK | 0o777
            self.report(opened, path)

    def test_same_nonregular_rows_still_rejected(self):
        for platform in ('linux', 'win32'):
            with self.subTest(platform=platform), mock.patch.object(c.sys, 'platform', platform):
                opened, path = self.row(), self.row()
                self.assertIsNone(c.require_open_handle(opened, path))
                opened.st_mode = path.st_mode = stat.S_IFDIR | 0o700
                with self.assertRaises(ValueError) as error:
                    c.require_open_handle(opened, path)
                report = json.loads(str(error.exception).split('; ', 1)[1])
                self.assertFalse(report['regular_handle'])
                self.assertTrue(report['canonical_identity_equal'])

    def test_equal_oversized_regular_rows_still_rejected(self):
        for platform in ('linux', 'win32'):
            with self.subTest(platform=platform), mock.patch.object(c.sys, 'platform', platform):
                opened, path = self.row(), self.row()
                self.assertIsNone(c.require_open_handle(opened, path))
                opened.st_size = path.st_size = c.MAX_BYTES + 1
                with self.assertRaises(ValueError) as error:
                    c.require_open_handle(opened, path)
                report = json.loads(str(error.exception).split('; ', 1)[1])
                self.assertFalse(report['within_byte_limit'])
                self.assertTrue(report['canonical_identity_equal'])

    def test_same_windows_open_handle_requires_raw_ctime_stability(self):
        with mock.patch.object(c.sys, 'platform', 'win32'):
            before, after = self.row(), self.row()
            self.assertTrue(c.same_open_handle(before, after))
            after.st_ctime_ns += 1
            self.assertEqual(c.canonical_identity(before), c.canonical_identity(after))
            self.assertFalse(c.same_open_handle(before, after))

    def test_same_windows_open_handle_requires_birthtime_stability(self):
        with mock.patch.object(c.sys, 'platform', 'win32'):
            before, after = self.row(), self.row()
            self.assertTrue(c.same_open_handle(before, after))
            after.st_birthtime_ns += 1
            self.assertEqual(c.stable_handle(before), c.stable_handle(after))
            self.assertFalse(c.same_open_handle(before, after))
            del after.st_birthtime_ns
            self.assertFalse(c.same_open_handle(before, after))

    def test_same_windows_open_handle_invalid_equal_birthtime_is_false(self):
        with mock.patch.object(c.sys, 'platform', 'win32'):
            before, after = self.row(), self.row()
            self.assertTrue(c.same_open_handle(before, after))
            for value in (True, None, 1 << 128):
                with self.subTest(value=value):
                    before.st_birthtime_ns = after.st_birthtime_ns = value
                    self.assertFalse(c.same_open_handle(before, after))

    def test_same_linux_open_handle_all_original_fields_required(self):
        with mock.patch.object(c.sys, 'platform', 'linux'):
            before, after = self.row(), self.row()
            self.assertTrue(c.same_open_handle(before, after))
            for field in ('st_dev', 'st_ino', 'st_size', 'st_mtime_ns', 'st_ctime_ns', 'st_nlink'):
                with self.subTest(field=field):
                    after = self.row(**{field: getattr(before, field) + 1})
                    self.assertFalse(c.same_open_handle(before, after))

    def canonical_read(self, opened, after_handle, path, after_entry=None):
        raw = b'data'
        entry = dict(mode=stat.S_IFREG | 0o600, file_attributes=0, reparse_tag=None)
        source = mock.MagicMock()
        source.fileno.return_value = 17
        source.read.return_value = raw
        context = mock.MagicMock()
        context.__enter__.return_value = source
        entries = [entry, entry if after_entry is None else after_entry]
        with mock.patch.object(c, 'entry', side_effect=entries), \
                mock.patch.object(c.ir, 'read_bounded', return_value=raw), \
                mock.patch.object(c.os, 'open', return_value=17), \
                mock.patch.object(c.os, 'fdopen', return_value=context), \
                mock.patch.object(c.os, 'fstat', side_effect=[opened, after_handle]), \
                mock.patch.object(Path, 'lstat', return_value=path):
            return c.read_canonical('/modeled/tool')

    def test_modeled_windows_canonical_read_all_guards_positive(self):
        with mock.patch.object(c.sys, 'platform', 'win32'):
            opened, path = self.admitted_windows()
            self.assertEqual(self.canonical_read(opened, self.row(), path),
                             (4, hashlib.sha256(b'data').hexdigest()))

    def test_after_read_handle_ctime_mutation_rejected(self):
        with mock.patch.object(c.sys, 'platform', 'win32'):
            opened, path = self.admitted_windows()
            self.canonical_read(opened, self.row(), path)
            with self.assertRaisesRegex(ValueError, 'canonical bytes and handle unchanged'):
                self.canonical_read(opened, self.row(st_ctime_ns=301), path)

    def test_after_read_handle_birthtime_mutation_rejected(self):
        with mock.patch.object(c.sys, 'platform', 'win32'):
            opened, path = self.admitted_windows()
            self.canonical_read(opened, self.row(), path)
            with self.assertRaisesRegex(ValueError, 'canonical bytes and handle unchanged'):
                self.canonical_read(opened, self.row(st_birthtime_ns=101), path)

    def test_after_read_path_birthtime_mutation_rejected(self):
        with mock.patch.object(c.sys, 'platform', 'win32'):
            opened, path = self.admitted_windows()
            self.canonical_read(opened, self.row(), path)
            calls = 0
            original = c.require_open_handle
            def changed_on_second_comparison(handle, current_path):
                nonlocal calls
                calls += 1
                if calls == 2:
                    current_path.st_birthtime_ns += 1
                return original(handle, current_path)
            with mock.patch.object(c, 'require_open_handle', side_effect=changed_on_second_comparison):
                with self.assertRaises(ValueError):
                    self.canonical_read(opened, self.row(), path)
            self.assertEqual(calls, 2)

    def test_final_same_path_entry_replacement_rejected(self):
        with mock.patch.object(c.sys, 'platform', 'win32'):
            opened, path = self.admitted_windows()
            self.canonical_read(opened, self.row(), path)
            changed = dict(mode=stat.S_IFREG | 0o600, file_attributes=0, reparse_tag=None, inode=999)
            with self.assertRaisesRegex(ValueError, 'canonical same-path metadata unchanged'):
                self.canonical_read(opened, self.row(), path, changed)

    def test_real_host_regular_read_positive(self):
        with tempfile.TemporaryDirectory(prefix='handle-identity-controls-') as temporary:
            path = Path(temporary) / 'ordinary-tool'
            data = b'ordinary bytes; never execute'
            path.write_bytes(data)
            self.assertEqual(c.read_canonical(str(path)), (len(data), hashlib.sha256(data).hexdigest()))


if __name__ == '__main__':
    unittest.main(verbosity=2)
