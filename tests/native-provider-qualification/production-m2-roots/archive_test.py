"""Synthetic three-root repository ZIP controls; no compiler/provider or older test methods run."""
import copy
import importlib.util
import io
from pathlib import Path
import shutil
import stat
import struct
import sys
import tempfile
from types import SimpleNamespace
import unittest
from unittest.mock import patch
import zipfile

sys.dont_write_bytecode = True


def load(name, filename):
    spec = importlib.util.spec_from_file_location(name, Path(__file__).with_name(filename))
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


a = load('corpus_archive_controls_subject', 'archive.py')
m = load('m2_repository_archive_model', 'model.py')
v = load('m2_repository_archive_reader', 'verify.py')
ERRORS = (AssertionError, ValueError, KeyError, OSError, zipfile.BadZipFile,
          NotImplementedError, RuntimeError)


class ArchiveControls(unittest.TestCase):
    def setUp(self):
        self.fixture = m.Model()
        self.addCleanup(self.fixture.close)
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.archive = Path(self.temp.name)/'proof.zip'
        self.output = Path(self.temp.name)/'recovered'
        self.calls = []
        self.fresh('linux')

    def synthetic_live_reader(self, root, proof, head, platform, *, live):
        self.assertIs(live, True)
        self.calls.append((root, proof, head, platform, live))
        # Explicit Python-only injection: synthetic host/tool paths cannot earn native proof.
        result = v.verify(root, proof, head, platform, run_id='123', run_attempt='1')
        return result | {'live': True}

    def pack(self, **kwargs):
        return a.pack(self.fixture.source, self.fixture.output, self.fixture.head,
                      self.fixture.receipt['platform'], self.archive,
                      reader_override=kwargs.get('reader_override', self.synthetic_live_reader))

    def fresh(self, platform):
        self.archive.unlink(missing_ok=True)
        if self.output.exists():
            shutil.rmtree(self.output)
        self.fixture.make(platform)  # Independent positive complete proof before EVERY attack.
        baseline = v.verify(self.fixture.source, self.fixture.output, self.fixture.head, platform,
                            run_id='123', run_attempt='1')
        self.assertEqual(baseline['counts'], {'passed': 3, 'failed': 0, 'ignored': 0})
        packed = self.pack()
        self.assertEqual(packed['file_count'], 57)
        with zipfile.ZipFile(self.archive) as archive:
            self.entries = [(copy.copy(info), archive.read(info)) for info in archive.infolist()]
        self.assertEqual(len(self.entries), 58)
        self.assertEqual(a.recover(self.archive, self.output)['file_count'], 57)
        baseline = v.verify(self.fixture.source, self.output, self.fixture.head, platform,
                              run_id='123', run_attempt='1')
        self.assertEqual(baseline['counts'], {'passed': 3, 'failed': 0, 'ignored': 0})
        shutil.rmtree(self.output)

    def rewrite(self, entries=None):
        with zipfile.ZipFile(self.archive, 'w') as archive:
            for info, raw in self.entries if entries is None else entries:
                archive.writestr(info, raw)

    def reject(self):
        with self.assertRaises(ERRORS):
            a.recover(self.archive, self.output)
        self.assertFalse(self.output.exists(), 'invalid ZIP created an extraction destination')

    def hostile(self, *mutations):
        for platform in ('linux', 'win32'):
            for index, mutation in enumerate(mutations):
                with self.subTest(platform=platform, mutation=index):
                    self.fresh(platform)
                    entries = copy.deepcopy(self.entries)
                    mutation(entries)
                    with __import__('warnings').catch_warnings():
                        __import__('warnings').simplefilter('ignore', UserWarning)
                        self.rewrite(entries)
                    self.reject()

    def patch_central(self, field_offset, value, format_code='H', index=0):
        raw = bytearray(self.archive.read_bytes())
        end = raw.rindex(b'PK\x05\x06')
        offset = struct.unpack_from('<I', raw, end+16)[0]
        for _ in range(index):
            name, extra, comment = struct.unpack_from('<HHH', raw, offset+28)
            offset += 46+name+extra+comment
        self.assertEqual(raw[offset:offset+4], b'PK\x01\x02')
        struct.pack_into('<'+format_code, raw, offset+field_offset, value)
        self.archive.write_bytes(raw)

    def test_complete_both_hosts_roundtrip_full_bytes_and_real_reader(self):
        for platform in ('linux', 'win32'):
            self.fresh(platform)
            result = a.recover(self.archive, self.output)
            self.assertEqual(result['directory_entries'], ['empty-runtime-path/'])
            self.assertEqual(a.snapshot(self.output), a.snapshot(self.fixture.output))
            with patch.object(a, 'reader', return_value=v.verify):
                second = Path(self.temp.name)/('admitted-'+platform)
                admitted = a.admit(self.fixture.source, self.archive, second, self.fixture.head,
                                   platform, run_id='123', run_attempt='1')
            self.assertEqual(admitted['admission']['counts']['passed'], 3)
            self.assertIs(admitted['admission']['live'], False)

    def test_pack_real_reader_selected_with_live_true(self):
        for platform in ('linux', 'win32'):
            self.fresh(platform)
            self.archive.unlink()
            before = len(self.calls)
            with patch.object(a, 'reader', return_value=self.synthetic_live_reader):
                a.pack(self.fixture.source, self.fixture.output, self.fixture.head, platform, self.archive)
            self.assertEqual(len(self.calls), before+1)

    def test_failed_live_admission_creates_no_archive(self):
        for platform in ('linux', 'win32'):
            self.fresh(platform)
            self.archive.unlink()
            def fail(*args, **kwargs):
                raise AssertionError('synthetic actual live admission failure')
            with self.assertRaises(AssertionError):
                self.pack(reader_override=fail)
            self.assertFalse(self.archive.exists())

    def test_live_result_status_identity_and_counts_cannot_be_substituted(self):
        for platform in ('linux', 'win32'):
            for field, value in (('status', 'failed'), ('live', False), ('head', 'wrong'),
                                 ('platform', 'other'), ('counts', {'passed': 2, 'failed': 0, 'ignored': 0})):
                self.fresh(platform)
                self.archive.unlink()
                def changed(*args, **kwargs):
                    result = self.synthetic_live_reader(*args, **kwargs)
                    return result | {field: value}
                with self.assertRaises(AssertionError):
                    self.pack(reader_override=changed)
                self.assertFalse(self.archive.exists())

    def test_cli_dispatch_has_no_reader_override_option(self):
        for platform in ('linux', 'win32'):
            self.fresh(platform)
            self.archive.unlink()
            argv = ['archive.py', 'pack', '--root', str(self.fixture.source), '--proof',
                    str(self.fixture.output), '--archive', str(self.archive), '--head',
                    self.fixture.head, '--platform', platform]
            with patch.object(sys, 'argv', argv), patch.object(a, 'reader',
                    return_value=self.synthetic_live_reader), patch('sys.stdout', new=io.StringIO()):
                a.main()
            self.assertTrue(self.archive.is_file())
            with patch.object(sys, 'argv', argv + ['--reader-override', 'synthetic']), \
                    patch('sys.stderr', new=io.StringIO()), self.assertRaises(SystemExit) as caught:
                a.main()
            self.assertEqual(caught.exception.code, 2)

    def test_admit_dispatch_binds_recovery_and_hosted_identity(self):
        for platform in ('linux', 'win32'):
            self.fresh(platform)
            with patch.object(a, 'reader', return_value=v.verify) as route:
                result = a.admit(self.fixture.source, self.archive, self.output, self.fixture.head,
                                 platform, run_id='123', run_attempt='1')
            route.assert_called_once_with()
            self.assertEqual(result['recovery']['archive_sha256'], a.digest(self.archive.read_bytes()))
            self.assertEqual(result['admission']['counts'], {'passed': 3, 'failed': 0, 'ignored': 0})
            self.assertIs(result['admission']['live'], False)

    def test_existing_and_inside_proof_pack_destinations(self):
        for platform in ('linux', 'win32'):
            self.fresh(platform)
            with self.assertRaises(AssertionError):
                self.pack()
            self.fresh(platform)
            with self.assertRaises(AssertionError):
                a.pack(self.fixture.source, self.fixture.output, self.fixture.head, platform,
                       self.fixture.output/'inside.zip', reader_override=self.synthetic_live_reader)

    def test_closed_proof_and_empty_runtime_required_before_pack(self):
        for platform in ('linux', 'win32'):
            for mutation in ('missing', 'extra', 'runtime'):
                self.fresh(platform)
                self.archive.unlink()
                if mutation == 'missing':
                    (self.fixture.output/'receipt.json').unlink()
                else:
                    (self.fixture.output/('empty-runtime-path/extra' if mutation == 'runtime' else 'extra')).write_bytes(b'x')
                with self.assertRaises(ERRORS):
                    self.pack()
                self.assertFalse(self.archive.exists())

    def test_original_explicit_empty_directory_cannot_be_reconstructed(self):
        self.hostile(lambda entries: entries.__delitem__(0))

    def test_nonempty_or_wrong_directory_entry(self):
        self.hostile(lambda e: e.__setitem__(0, (a.zip_info(a.EMPTY, True), b'x')),
                     lambda e: e.__setitem__(0, (a.zip_info('other/', True), b'')))

    def test_missing_extra_and_substituted_regular_census(self):
        self.hostile(lambda e: e.pop(), lambda e: e.append((a.zip_info('extra'), b'')),
                     lambda e: e.__setitem__(-1, (a.zip_info('empty-runtime-path/child'), b'')))

    def test_absolute_traversal_backslash_and_normalized_names(self):
        for name in ('./receipt.json', 'cli-build//ci-receipt.json', '/receipt.json', '../receipt.json',
                     'C:/receipt.json', 'cli-build\\receipt.json', 'cli-build/./ci-receipt.json'):
            self.hostile(lambda e, name=name: e.__setitem__(-1, (a.zip_info(name), b'')))

    def test_duplicate_and_casefold_member_names(self):
        self.hostile(lambda e: e.__setitem__(-1, (a.zip_info(e[1][0].filename), b'')),
                     lambda e: e.__setitem__(-1, (a.zip_info(e[1][0].filename.upper()), b'')))

    def test_links_special_types_and_conflicting_directory_markers(self):
        for kind in (stat.S_IFLNK, stat.S_IFIFO, stat.S_IFCHR, stat.S_IFDIR):
            self.hostile(lambda e, kind=kind: setattr(e[1][0], 'external_attr', (kind|0o600)<<16))
        self.hostile(lambda e: setattr(e[0][0], 'external_attr', (stat.S_IFREG|0o600)<<16|0x10),
                     lambda e: setattr(e[1][0], 'external_attr', e[1][0].external_attr|0x10))

    def test_explicit_unix_type_and_dos_directory_marker(self):
        self.hostile(lambda e: setattr(e[1][0], 'create_system', 0),
                     lambda e: setattr(e[0][0], 'external_attr', e[0][0].external_attr & ~0x10))

    def test_noncanonical_permission_and_dos_flags(self):
        for index, extra in ((1, stat.S_ISUID<<16), (0, stat.S_ISVTX<<16), (1, 0x20), (0, 0x20)):
            self.hostile(lambda e, index=index, extra=extra: setattr(e[index][0], 'external_attr',
                                                                  e[index][0].external_attr|extra))

    def test_encrypted_unsupported_flags_and_compression(self):
        for platform in ('linux', 'win32'):
            for offset, value in ((8, 1), (8, 64), (10, 99)):
                self.fresh(platform)
                self.patch_central(offset, value)
                self.reject()

    def test_compressed_member_and_total_resource_limits(self):
        for platform in ('linux', 'win32'):
            for constant in ('MAX_ARCHIVE', 'MAX_FILE', 'MAX_TOTAL'):
                self.fresh(platform)
                with patch.object(a, constant, 1):
                    self.reject()

    def test_crc_corruption_validated_before_extraction(self):
        for platform in ('linux', 'win32'):
            self.fresh(platform)
            index = next(i for i, row in enumerate(self.entries) if row[0].CRC != 0)
            self.patch_central(16, 0, 'I', index=index)
            self.reject()

    def test_raw_nul_member_not_silently_normalized(self):
        for platform in ('linux', 'win32'):
            self.fresh(platform)
            raw = self.archive.read_bytes()
            self.assertEqual(len(b'empty-runtime-path/'), len(b'empty\0runtime-path/'))
            self.archive.write_bytes(raw.replace(b'empty-runtime-path/', b'empty\0runtime-path/'))
            self.reject()

    def test_structural_archive_success_cannot_admit_changed_proof_bytes(self):
        for platform in ('linux', 'win32'):
            self.fresh(platform)
            entries = copy.deepcopy(self.entries)
            index = next(i for i, row in enumerate(entries) if row[0].filename.endswith('native.stdout'))
            entries[index] = (entries[index][0], b'{}\n')
            self.rewrite(entries)
            with patch.object(a, 'reader', return_value=v.verify), self.assertRaises(ERRORS):
                a.admit(self.fixture.source, self.archive, self.output, self.fixture.head,
                        platform, run_id='123', run_attempt='1')

    def test_existing_destination_and_mocked_archive_link_reparse(self):
        real_lstat = Path.lstat
        for platform in ('linux', 'win32'):
            self.fresh(platform)
            self.output.mkdir()
            with self.assertRaises(AssertionError):
                a.recover(self.archive, self.output)
            self.output.rmdir()
            for attr in ('symlink', 'reparse'):
                self.fresh(platform)
                def hostile(path):
                    result = real_lstat(path)
                    if path == self.archive:
                        return SimpleNamespace(st_mode=stat.S_IFLNK if attr == 'symlink' else result.st_mode,
                                               st_file_attributes=0x400 if attr == 'reparse' else 0)
                    return result
                with patch.object(Path, 'lstat', hostile):
                    self.reject()

    def test_mocked_proof_link_and_nonregular_archive(self):
        real_lstat = Path.lstat
        for platform in ('linux', 'win32'):
            self.fresh(platform)
            self.archive.unlink()
            target = self.fixture.output/'receipt.json'
            def hostile(path):
                result = real_lstat(path)
                return SimpleNamespace(st_mode=stat.S_IFLNK, st_file_attributes=0) if path == target else result
            with patch.object(Path, 'lstat', hostile), self.assertRaises(ERRORS):
                self.pack()
            self.assertFalse(self.archive.exists())
            self.fresh(platform)
            self.archive.unlink()
            self.archive.mkdir()
            self.reject()
            self.archive.rmdir()

    def test_pack_detects_proof_changed_during_write(self):
        for platform in ('linux', 'win32'):
            self.fresh(platform)
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


if __name__ == '__main__':
    unittest.main()
