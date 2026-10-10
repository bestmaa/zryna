"""Synthetic complete-archive controls; no compiler, provider or old test methods run."""
import copy
import importlib.util
from pathlib import Path
import stat
import struct
import tempfile
import unittest
from unittest.mock import patch
import zipfile


def load(name, filename):
    spec = importlib.util.spec_from_file_location(name, Path(__file__).with_name(filename))
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


a = load('archive_controls_subject', 'archive.py')
c = load('archive_neutral_fixture_helpers', 'verify_test.py')
ERRORS = (AssertionError, ValueError, KeyError, TypeError, OSError, zipfile.BadZipFile,
          NotImplementedError, RuntimeError)


class ArchiveControls(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        c.ManifestControls.setUpClass()

    @classmethod
    def tearDownClass(cls):
        c.ManifestControls.tearDownClass()

    def setUp(self):
        self.fixture = c.ManifestControls()
        self.fixture.setUp()
        self.addCleanup(self.fixture.doCleanups)
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.archive = Path(self.temp.name) / 'proof.zip'
        self.output = Path(self.temp.name) / 'recovered'
        self.calls = []
        self.pack()
        with zipfile.ZipFile(self.archive) as archive:
            self.entries = [(copy.copy(info), archive.read(info)) for info in archive.infolist()]
        self.assertEqual(len(self.entries), 114)
        self.assertEqual(a.recover(self.archive, self.output)['file_count'], 113)
        baseline = c.v.verify(self.fixture.source, self.output, self.fixture.head, 'linux',
                              run_id='123', run_attempt='1')
        self.assertEqual(baseline['counts'], {'passed': 7, 'failed': 0, 'ignored': 0})
        # Every hostile control begins with an admitted baseline and a fresh destination.
        import shutil
        shutil.rmtree(self.output)

    def synthetic_live_reader(self, root, proof, head, platform, *, live):
        self.assertIs(live, True)
        self.calls.append((root, proof, head, platform, live))
        # This fixture has intentionally synthetic tool bytes and recorded host paths.
        result = c.v.verify(root, proof, head, platform, run_id='123', run_attempt='1')
        return result | {'live': True}

    def pack(self, **kwargs):
        return a.pack(self.fixture.source, self.fixture.output, self.fixture.head,
                      self.fixture.receipt['platform'], self.archive,
                      reader_override=kwargs.get('reader_override', self.synthetic_live_reader))

    def rewrite(self, entries=None):
        with zipfile.ZipFile(self.archive, 'w') as archive:
            for info, raw in self.entries if entries is None else entries:
                archive.writestr(info, raw)

    def reject(self):
        with self.assertRaises(ERRORS):
            a.recover(self.archive, self.output)
        self.assertFalse(self.output.exists(), 'invalid archive created an extraction destination')

    def test_complete_roundtrip_exact_bytes_and_reader(self):
        self.assertEqual(len(self.calls), 1)
        result = a.recover(self.archive, self.output)
        self.assertEqual(a.snapshot(self.output), a.snapshot(self.fixture.output))
        self.assertEqual(result['directory_entries'], ['empty-runtime-path/'])
        with patch.object(a, 'reader', return_value=c.v.verify):
            second = Path(self.temp.name) / 'second'
            admitted = a.admit(self.fixture.source, self.archive, second, self.fixture.head,
                               'linux', run_id='123', run_attempt='1')
        self.assertEqual(admitted['admission']['counts']['passed'], 7)
        self.assertIs(admitted['admission']['live'], False)

    def test_windows_recorded_paths_roundtrip_without_windows_execution(self):
        self.archive.unlink()
        self.fixture.make('win32')
        self.pack()
        a.recover(self.archive, self.output)
        self.assertEqual(c.v.verify(self.fixture.source, self.output, self.fixture.head,
                                   'win32', run_id='123', run_attempt='1')['counts']['passed'], 7)

    def test_pack_failed_live_admission_leaves_no_archive(self):
        self.archive.unlink()
        def fail(*args, **kwargs):
            raise AssertionError('synthetic live rejection')
        with self.assertRaises(AssertionError):
            self.pack(reader_override=fail)
        self.assertFalse(self.archive.exists())

    def test_pack_defaults_to_real_reader_with_live_true(self):
        self.archive.unlink()
        with patch.object(a, 'reader', return_value=self.synthetic_live_reader):
            a.pack(self.fixture.source, self.fixture.output, self.fixture.head, 'linux', self.archive)
        self.assertEqual(len(self.calls), 2)

    def test_pack_existing_or_inside_proof_destination_rejected(self):
        with self.assertRaises(AssertionError):
            self.pack()
        with self.assertRaises(AssertionError):
            a.pack(self.fixture.source, self.fixture.output, self.fixture.head, 'linux',
                   self.fixture.output/'inside.zip', reader_override=self.synthetic_live_reader)

    def test_pack_proof_missing_extra_or_nonempty_runtime_rejected(self):
        for mutation in ('missing', 'extra', 'runtime'):
            with self.subTest(mutation=mutation):
                self.archive.unlink(missing_ok=True)
                self.fixture.make('linux')
                if mutation == 'missing':
                    (self.fixture.output/'receipt.json').unlink()
                else:
                    path = self.fixture.output/('empty-runtime-path/extra' if mutation == 'runtime' else 'extra')
                    path.write_bytes(b'hostile')
                with self.assertRaises(ERRORS):
                    self.pack()
                self.assertFalse(self.archive.exists())

    def test_missing_explicit_directory_never_reconstructed(self):
        self.rewrite([row for row in self.entries if row[0].filename != a.EMPTY])
        self.reject()

    def test_nonempty_or_wrong_directory_rejected(self):
        for name, raw in ((a.EMPTY, b'nonempty'), ('other/', b'')):
            with self.subTest(name=name):
                entries = copy.deepcopy(self.entries)
                entries[0] = (a.zip_info(name, True), raw)
                self.rewrite(entries)
                self.reject()

    def test_missing_extra_and_substituted_file_census(self):
        for mutation in ('missing', 'extra', 'substituted'):
            with self.subTest(mutation=mutation):
                entries = copy.deepcopy(self.entries)
                if mutation == 'missing':
                    entries.pop()
                elif mutation == 'extra':
                    entries.append((a.zip_info('extra'), b''))
                else:
                    entries[-1] = (a.zip_info('empty-runtime-path/child'), b'')
                self.rewrite(entries)
                self.reject()

    def test_raw_noncanonical_absolute_and_windows_names(self):
        for name in ('./receipt.json', 'cli-build//ci-receipt.json', '/receipt.json',
                     '../receipt.json', 'C:/receipt.json', 'cli-build\\receipt.json',
                     'cli-build/./ci-receipt.json'):
            with self.subTest(name=name):
                entries = copy.deepcopy(self.entries)
                entries[-1] = (a.zip_info(name), b'')
                self.rewrite(entries)
                self.reject()

    def test_duplicate_and_casefold_collisions(self):
        for name in (self.entries[1][0].filename, self.entries[1][0].filename.upper()):
            with self.subTest(name=name):
                entries = copy.deepcopy(self.entries)
                entries[-1] = (a.zip_info(name), b'')
                with self.assertWarns(UserWarning) if name == entries[1][0].filename else self.subTest():
                    self.rewrite(entries)
                self.reject()

    def test_links_special_types_and_directory_marker_conflicts(self):
        for kind in (stat.S_IFLNK, stat.S_IFIFO, stat.S_IFCHR, stat.S_IFDIR):
            with self.subTest(kind=kind):
                entries = copy.deepcopy(self.entries)
                entries[1][0].external_attr = (kind | 0o600) << 16
                self.rewrite(entries)
                self.reject()
        for directory in (True, False):
            entries = copy.deepcopy(self.entries)
            if directory:
                entries[0][0].external_attr = (stat.S_IFREG | 0o600) << 16 | 0x10
            else:
                entries[1][0].external_attr |= 0x10
            self.rewrite(entries)
            self.reject()

    def test_missing_explicit_unix_type_and_directory_dos_marker(self):
        for index, change in ((1, 'system'), (0, 'dos')):
            entries = copy.deepcopy(self.entries)
            if change == 'system':
                entries[index][0].create_system = 0
            else:
                entries[index][0].external_attr &= ~0x10
            self.rewrite(entries)
            self.reject()

    def test_noncanonical_permissions_and_dos_flags(self):
        for index, extra in ((1, stat.S_ISUID << 16), (0, stat.S_ISVTX << 16),
                             (1, 0x20), (0, 0x20)):
            entries = copy.deepcopy(self.entries)
            entries[index][0].external_attr |= extra
            self.rewrite(entries)
            self.reject()

    def patch_central(self, field_offset, value, format_code='H', index=0):
        raw = bytearray(self.archive.read_bytes())
        end = raw.rindex(b'PK\x05\x06')
        offset = struct.unpack_from('<I', raw, end+16)[0]
        for _ in range(index):
            name, extra, comment = struct.unpack_from('<HHH', raw, offset+28)
            offset += 46 + name + extra + comment
        self.assertEqual(raw[offset:offset+4], b'PK\x01\x02')
        struct.pack_into('<'+format_code, raw, offset+field_offset, value)
        self.archive.write_bytes(raw)

    def test_encrypted_and_unsupported_flags_or_compression(self):
        for offset, value in ((8, 1), (8, 64), (10, 99)):
            self.rewrite()
            self.patch_central(offset, value)
            self.reject()

    def test_archive_member_and_total_size_limits(self):
        for constant in ('MAX_ARCHIVE', 'MAX_FILE', 'MAX_TOTAL'):
            with self.subTest(constant=constant), patch.object(a, constant, 1):
                self.reject()

    def test_corrupt_payload_and_raw_nul_member(self):
        index = next(i for i, row in enumerate(self.entries) if row[0].CRC != 0)
        self.patch_central(16, 0, 'I', index=index)
        self.reject()
        self.rewrite()
        raw = bytearray(self.archive.read_bytes())
        # Both header spellings carry the hostile NUL, preserving equal name lengths.
        name = b'empty-runtime-path/'
        altered = b'empty\0runtime-path/'
        self.assertEqual(len(name), len(altered))
        self.archive.write_bytes(bytes(raw).replace(name, altered))
        self.reject()

    def test_admit_rejects_changed_bytes_after_structural_recovery(self):
        entries = copy.deepcopy(self.entries)
        index = next(i for i, row in enumerate(entries) if row[0].filename.endswith('native.stdout'))
        entries[index] = (entries[index][0], b'{}\n')
        self.rewrite(entries)
        with patch.object(a, 'reader', return_value=c.v.verify), self.assertRaises(ERRORS):
            a.admit(self.fixture.source, self.archive, self.output, self.fixture.head, 'linux',
                    run_id='123', run_attempt='1')

    def test_existing_destination_and_mocked_symlink_reparse_rejected(self):
        self.output.mkdir()
        with self.assertRaises(AssertionError):
            a.recover(self.archive, self.output)
        self.output.rmdir()
        real_lstat = Path.lstat
        for attr in ('symlink', 'reparse'):
            def hostile(path):
                result = real_lstat(path)
                if path == self.archive:
                    from types import SimpleNamespace
                    return SimpleNamespace(st_mode=stat.S_IFLNK if attr == 'symlink' else result.st_mode,
                                           st_file_attributes=0x400 if attr == 'reparse' else 0)
                return result
            with patch.object(Path, 'lstat', hostile):
                self.reject()

    def test_pack_detects_proof_change_during_zip_write(self):
        self.archive.unlink()
        original = zipfile.ZipFile.writestr
        changed = False
        def write(archive, *args, **kwargs):
            nonlocal changed
            result = original(archive, *args, **kwargs)
            if not changed:
                changed = True
                (self.fixture.output/'receipt.json').write_bytes(b'changed after admission')
            return result
        with patch.object(zipfile.ZipFile, 'writestr', write), self.assertRaises(AssertionError):
            self.pack()

    def test_archive_not_regular_and_proof_link_rejected_without_host_symlinks(self):
        self.archive.unlink()
        target = self.fixture.output/'receipt.json'
        real_lstat = Path.lstat
        def hostile(path):
            result = real_lstat(path)
            if path == target:
                from types import SimpleNamespace
                return SimpleNamespace(st_mode=stat.S_IFLNK, st_file_attributes=0)
            return result
        with patch.object(Path, 'lstat', hostile), self.assertRaises(ERRORS):
            self.pack()
        self.assertFalse(self.archive.exists())
        self.archive.mkdir()
        self.reject()


if __name__ == '__main__':
    unittest.main()
