"""Archive guard-only models: no modeled IR/compiler execution is admitted as actual proof."""
import copy
import io
import json
from pathlib import Path
import stat
import struct
import tempfile
from types import SimpleNamespace
import unittest
from unittest import mock
import zipfile
import zlib
import archive as a

ROOT = Path(__file__).resolve().parents[3]
HEAD = 'c' * 40
ERRORS = (ValueError, zipfile.BadZipFile, zlib.error, NotImplementedError)


class Model:
    def __init__(self, platform):
        self.temporary = tempfile.TemporaryDirectory(prefix='ir-archive-guard-model-')
        self.base = Path(self.temporary.name)
        self.root = self.base / 'source'
        self.proof = self.base / 'proof'
        self.output = self.base / 'recovered'
        self.archive = self.base / 'original.zip'
        self.platform = platform
        inventory = (ROOT / a.c.INVENTORY).read_bytes()
        target = self.root / a.c.INVENTORY
        target.parent.mkdir(parents=True)
        target.write_bytes(inventory)
        self.files = a.c.expected_files(inventory, platform)
        self.directories = a.c.expected_directories(self.files)
        self.proof.mkdir()
        for name in sorted(self.directories, key=lambda n: (len(Path(n).parts), n)):
            (self.proof / name).mkdir()
        for name in self.files:
            raw = inventory if name == 'inventory.json' else b'guard-only model; no IR authority\n'
            (self.proof / name).write_bytes(raw)
        self.calls = []

    def reader(self, root, proof, head, platform, **options):
        self.calls.append((root, proof, head, platform, options))
        return {'status': 'passed', 'head': head, 'tree': 'd' * 40, 'platform': platform,
                'live': options['live'], 'counts': {'passed': 107, 'failed': 0, 'ignored': 0},
                'commands': 6, 'hostile_controls': 49, 'public_activation': False,
                'model_only': True, 'actual_ir_execution': False}

    def baseline(self):
        result = a.pack(self.root, self.proof, HEAD, self.platform, self.archive,
                        reader_override=self.reader)
        assert result['file_count'] == 393 and result['live_admission']['model_only'] is True
        assert result['live_admission']['actual_ir_execution'] is False
        files, directories, raw = a.validate(self.root, self.archive, self.platform)
        assert set(files) == self.files and directories == self.directories
        assert result['archive_sha256'] == a.digest(raw)
        return result

    def rewrite(self, mutate):
        with zipfile.ZipFile(self.archive) as source:
            entries = [(copy.copy(item), source.read(item)) for item in source.infolist()]
        mutate(entries)
        output = self.base / 'malformed.zip'
        with zipfile.ZipFile(output, 'w') as archive:
            for item, raw in entries:
                archive.writestr(item, raw)
        return output

    def close(self):
        self.temporary.cleanup()


class ArchiveGuardControls(unittest.TestCase):
    def models(self, action):
        for platform in ('linux', 'win32'):
            with self.subTest(platform=platform):
                model = Model(platform)
                try:
                    model.baseline()  # A valid host-specific envelope precedes every mutation.
                    action(model)
                finally:
                    model.close()

    def reject(self, mutate):
        def action(model):
            malformed = model.rewrite(mutate)
            with self.assertRaises(ERRORS):
                a.recover(model.root, malformed, model.output, model.platform)
            self.assertFalse(model.output.exists(), 'all original bytes checked before mkdir')
        self.models(action)

    def altered_regular(self, change):
        def mutate(entries):
            index = next(i for i, (item, _) in enumerate(entries) if not item.is_dir())
            change(entries, index)
        self.reject(mutate)

    def test_guard_model_roundtrip_both_hosts(self):
        def action(model):
            result = a.recover(model.root, model.archive, model.output, model.platform)
            self.assertEqual(result['file_count'], 393)
            self.assertEqual(set(result['files']), model.files)
            self.assertEqual(a.snapshot(model.output, model.files, model.directories, model.platform),
                             a.snapshot(model.proof, model.files, model.directories, model.platform))
            self.assertEqual(set(result['directory_entries']), {n + '/' for n in model.directories})
            for name in model.files:
                self.assertEqual((model.output / name).read_bytes(), (model.proof / name).read_bytes())
        self.models(action)

    def test_guard_admit_forwards_archived_identity_without_execution(self):
        def action(model):
            with mock.patch.object(a, 'reader', return_value=model.reader):
                result = a.admit(model.root, model.archive, model.output, HEAD,
                                 model.platform, '17', '2')
            self.assertTrue(result['admission']['model_only'])
            self.assertEqual(model.calls[-1][-1], {'live': False, 'run_id': '17', 'run_attempt': '2'})
        self.models(action)

    def test_windows_filename_execute_bits_do_not_change_handle_identity(self):
        def action(model):
            binary = model.proof / a.c.binary_name(model.platform)
            binary.chmod(0o755)
            expected = binary.lstat()
            original = a.os.fstat
            def handle_stat(fd):
                info = original(fd)
                if (info.st_dev, info.st_ino) != (expected.st_dev, expected.st_ino): return info
                return SimpleNamespace(st_dev=info.st_dev, st_ino=info.st_ino,
                                       st_size=info.st_size, st_mtime_ns=info.st_mtime_ns,
                                       st_mode=info.st_mode & ~0o111)
            with mock.patch.object(a.os, 'fstat', handle_stat):
                self.assertEqual(a.read_file(binary, a.c.MAX_BINARY), binary.read_bytes())
                result = a.pack(model.root, model.proof, HEAD, model.platform,
                                model.base / 'windows-stat-model.zip', reader_override=model.reader)
            self.assertEqual(result['file_count'], 393)
        self.models(action)

    def test_changed_handle_identity_size_time_and_type_still_reject(self):
        for field in ('st_ino', 'st_size', 'st_mtime_ns', 'st_mode'):
            def action(model):
                binary = model.proof / a.c.binary_name(model.platform)
                original = a.os.fstat
                def changed(fd):
                    info = original(fd)
                    values = {n: getattr(info, n) for n in
                              ('st_dev', 'st_ino', 'st_size', 'st_mtime_ns', 'st_mode')}
                    values[field] = stat.S_IFIFO | 0o600 if field == 'st_mode' else values[field] + 1
                    return SimpleNamespace(**values)
                with mock.patch.object(a.os, 'fstat', changed), self.assertRaises(ValueError):
                    a.read_file(binary, a.c.MAX_BINARY)
            with self.subTest(field=field): self.models(action)

    def test_same_path_permission_change_still_rejects(self):
        def action(model):
            binary = model.proof / a.c.binary_name(model.platform)
            original = Path.lstat
            calls = []
            def changed(path):
                info = original(path)
                if path != binary: return info
                calls.append(True)
                if len(calls) == 1: return info
                values = {n: getattr(info, n) for n in
                          ('st_dev', 'st_ino', 'st_size', 'st_mtime_ns', 'st_mode')}
                values['st_mode'] ^= 0o100
                return SimpleNamespace(**values)
            with mock.patch.object(Path, 'lstat', changed), self.assertRaises(ValueError):
                a.read_file(binary, a.c.MAX_BINARY)
        self.models(action)

    def test_missing_regular(self):
        self.altered_regular(lambda rows, index: rows.pop(index))

    def test_extra_regular(self):
        self.reject(lambda rows: rows.append((a.zip_info('unreported.json'), b'{}')))

    def test_duplicate_regular_same_entry_count(self):
        self.altered_regular(lambda rows, index: rows.__setitem__(index, copy.deepcopy(rows[-1])))

    def test_case_colliding_regular(self):
        def mutate(rows, index):
            item = copy.copy(rows[-1][0]); item.filename = item.filename.upper()
            rows[index] = item, rows[-1][1]
        self.altered_regular(mutate)

    def test_missing_original_directory(self):
        self.reject(lambda rows: rows.pop(next(i for i, (item, _) in enumerate(rows) if item.is_dir())))

    def test_extra_original_directory(self):
        self.reject(lambda rows: rows.append((a.zip_info('foreign-empty', True), b'')))

    def test_duplicate_directory_same_entry_count(self):
        def mutate(rows):
            indices = [i for i, (item, _) in enumerate(rows) if item.is_dir()]
            rows[indices[0]] = copy.deepcopy(rows[indices[1]])
        self.reject(mutate)

    def test_nonempty_directory_entry(self):
        def mutate(rows):
            index = next(i for i, (item, _) in enumerate(rows) if item.is_dir())
            rows[index] = rows[index][0], b'not an empty directory entry'
        self.reject(mutate)

    def test_directory_disguised_as_regular(self):
        def mutate(rows):
            index = next(i for i, (item, _) in enumerate(rows) if item.is_dir())
            rows[index][0].external_attr = (stat.S_IFREG | 0o600) << 16
        self.reject(mutate)

    def test_link_type(self):
        self.altered_regular(lambda rows, i: setattr(rows[i][0], 'external_attr', (stat.S_IFLNK | 0o600) << 16))

    def test_special_type(self):
        self.altered_regular(lambda rows, i: setattr(rows[i][0], 'external_attr', (stat.S_IFIFO | 0o600) << 16))

    def test_wrong_regular_permission_and_dos_bits(self):
        for attr in ((stat.S_IFREG | 0o644) << 16, (stat.S_IFREG | 0o4600) << 16,
                     ((stat.S_IFREG | 0o600) << 16) | 0x10):
            with self.subTest(attr=attr):
                self.altered_regular(lambda rows, i: setattr(rows[i][0], 'external_attr', attr))

    def test_missing_portable_type_origin(self):
        self.altered_regular(lambda rows, i: setattr(rows[i][0], 'create_system', 0))

    def test_traversal_absolute_backslash_and_normalized_names(self):
        for name in ('../escape', '/escape', 'C:/escape', 'folder\\escape', './admission.json',
                     'observations//case-000/worker.json', 'CON.json', 'admission.json.',
                     'admission.json ', 'line\nbreak.json'):
            with self.subTest(name=name):
                self.altered_regular(lambda rows, i: setattr(rows[i][0], 'filename', name))

    def test_unsupported_compression(self):
        self.altered_regular(lambda rows, i: setattr(rows[i][0], 'compress_type', zipfile.ZIP_BZIP2))

    def raw_failure(self, mutate):
        def action(model):
            raw = bytearray(model.archive.read_bytes())
            mutate(raw)
            malformed = model.base / 'raw-malformed.zip'; malformed.write_bytes(raw)
            with self.assertRaises(ERRORS):
                a.recover(model.root, malformed, model.output, model.platform)
            self.assertFalse(model.output.exists())
        self.models(action)

    def test_encrypted_flag(self):
        def mutate(raw):
            offset = raw.index(b'PK\x01\x02')
            struct.pack_into('<H', raw, offset + 8, 1)
        self.raw_failure(mutate)

    def test_unsupported_flags(self):
        def mutate(raw):
            offset = raw.index(b'PK\x01\x02'); struct.pack_into('<H', raw, offset + 8, 0x10)
        self.raw_failure(mutate)

    def test_raw_nul_name(self):
        def mutate(raw):
            offset = raw.index(b'PK\x01\x02'); raw[offset + 46] = 0
        self.raw_failure(mutate)

    def test_crc_bad_before_any_extraction(self):
        def mutate(raw):
            offset = raw.rindex(b'PK\x01\x02')  # Last member: all preceding bytes remain valid.
            current = struct.unpack_from('<I', raw, offset + 16)[0]
            struct.pack_into('<I', raw, offset + 16, current ^ 1)
        self.raw_failure(mutate)

    def test_truncated_archive(self):
        self.raw_failure(lambda raw: raw.__delitem__(slice(-40, None)))

    def test_inventory_substitution(self):
        def mutate(rows):
            index = next(i for i, (item, _) in enumerate(rows) if item.filename == 'inventory.json')
            value = a.c.strict_json(rows[index][1]); value['provenance'] = 'forged model'
            rows[index] = rows[index][0], json.dumps(value).encode()
        self.reject(mutate)

    def test_source_inventory_alias_and_context_census(self):
        for change in ('alias', 'missing-context', 'duplicate-context', 'boolean-fileid'):
            def action(model):
                path = model.root / a.c.INVENTORY
                value = a.c.strict_json(path.read_bytes())
                if change == 'alias': value['cases'][0]['sources'][0]['path'] = 'CON.zry'
                elif change == 'missing-context': value['cases'].pop()
                elif change == 'duplicate-context': value['cases'][1]['id'] = value['cases'][0]['id']
                else: value['cases'][0]['sources'][0]['file_id'] = False
                path.write_bytes(json.dumps(value).encode())
                with self.assertRaises(ValueError):
                    a.recover(model.root, model.archive, model.output, model.platform)
                self.assertFalse(model.output.exists())
            with self.subTest(change=change): self.models(action)

    def test_normal_member_size_bound(self):
        def action(model):
            with mock.patch.object(a.c, 'MAX_FILE', 2):
                with self.assertRaises(ValueError):
                    a.recover(model.root, model.archive, model.output, model.platform)
            self.assertFalse(model.output.exists())
        self.models(action)

    def test_binary_member_separate_size_bound(self):
        def action(model):
            with mock.patch.object(a.c, 'MAX_BINARY', 2):
                with self.assertRaises(ValueError):
                    a.recover(model.root, model.archive, model.output, model.platform)
            self.assertFalse(model.output.exists())
        self.models(action)

    def test_aggregate_size_bound(self):
        def action(model):
            with mock.patch.object(a.c, 'MAX_TOTAL', 2):
                with self.assertRaises(ValueError):
                    a.recover(model.root, model.archive, model.output, model.platform)
            self.assertFalse(model.output.exists())
        self.models(action)

    def test_compressed_archive_size_bound(self):
        def action(model):
            with mock.patch.object(a.c, 'MAX_ARCHIVE', 2):
                with self.assertRaises(ValueError):
                    a.recover(model.root, model.archive, model.output, model.platform)
            self.assertFalse(model.output.exists())
        self.models(action)

    def test_create_only_archive_and_recovery(self):
        def action(model):
            raw = model.archive.read_bytes()
            with self.assertRaises(ValueError):
                a.pack(model.root, model.proof, HEAD, model.platform, model.archive,
                       reader_override=model.reader)
            self.assertEqual(model.archive.read_bytes(), raw)
            model.output.mkdir(); marker = model.output / 'foreign'; marker.write_bytes(b'keep')
            with self.assertRaises(ValueError):
                a.recover(model.root, model.archive, model.output, model.platform)
            self.assertEqual(marker.read_bytes(), b'keep')
        self.models(action)

    def test_generated_outputs_remain_outside_controlled_source(self):
        def action(model):
            archive = model.root / 'foreign.zip'
            with self.assertRaises(ValueError):
                a.pack(model.root, model.proof, HEAD, model.platform, archive,
                       reader_override=model.reader)
            self.assertFalse(archive.exists())
            output = model.root / 'foreign-recovery'
            with self.assertRaises(ValueError):
                a.recover(model.root, model.archive, output, model.platform)
            self.assertFalse(output.exists())
        self.models(action)

    def test_pack_missing_extra_and_empty_directory_roles(self):
        for mutation in ('missing', 'extra', 'empty-directory'):
            def action(model):
                if mutation == 'missing': (model.proof / 'corpus.json').unlink()
                elif mutation == 'extra': (model.proof / 'extra').write_bytes(b'extra')
                else: (model.proof / 'foreign-empty').mkdir()
                archive = model.base / 'new.zip'
                with self.assertRaises(ValueError):
                    a.pack(model.root, model.proof, HEAD, model.platform, archive,
                           reader_override=model.reader)
                self.assertFalse(archive.exists())
            with self.subTest(mutation=mutation): self.models(action)

    def test_proof_change_during_pack_preserves_failed_archive(self):
        def action(model):
            original = a.snapshot
            calls = []
            def changing(*args):
                calls.append(True)
                if len(calls) == 2: (model.proof / 'corpus.json').write_bytes(b'changed after pack')
                return original(*args)
            archive = model.base / 'failed-original.zip'
            with mock.patch.object(a, 'snapshot', changing), self.assertRaises(ValueError):
                a.pack(model.root, model.proof, HEAD, model.platform, archive,
                       reader_override=model.reader)
            self.assertTrue(archive.is_file(), 'failed original retained; no deletion or rebuilding')
        self.models(action)

    def test_mock_link_reparse_and_special_entries_without_host_skips(self):
        for kind in ('link', 'reparse', 'fifo'):
            def action(model):
                original = Path.lstat
                def hostile(path):
                    info = original(path)
                    if path != model.proof / 'corpus.json': return info
                    fields = {n: getattr(info, n) for n in
                              ('st_dev', 'st_ino', 'st_size', 'st_mtime_ns', 'st_mode')}
                    if kind == 'reparse': fields['st_file_attributes'] = 0x400
                    else: fields['st_mode'] = (stat.S_IFLNK if kind == 'link' else stat.S_IFIFO) | 0o600
                    return SimpleNamespace(**fields)
                with mock.patch.object(Path, 'lstat', hostile), self.assertRaises(ValueError):
                    a.pack(model.root, model.proof, HEAD, model.platform, model.base / 'new.zip',
                           reader_override=model.reader)
            with self.subTest(kind=kind): self.models(action)

    def test_live_failed_or_false_runner_admission(self):
        for key, value in (('status', 'failed'), ('head', 'e' * 40), ('platform', 'other'),
                           ('live', False), ('hostile_controls', 48), ('commands', 5),
                           ('public_activation', True), ('counts', {'passed': 107, 'failed': False, 'ignored': 0})):
            def action(model):
                def wrong(*args, **options):
                    result = model.reader(*args, **options); result[key] = value; return result
                archive = model.base / 'new.zip'
                with self.assertRaises(ValueError):
                    a.pack(model.root, model.proof, HEAD, model.platform, archive, reader_override=wrong)
                self.assertFalse(archive.exists())
            with self.subTest(key=key): self.models(action)


if __name__ == '__main__':
    unittest.main(verbosity=2)
