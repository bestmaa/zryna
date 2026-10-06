"""Modeled capability guards; no compiler, provider or executable tool is run."""
from contextlib import contextmanager
import copy
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


if __name__ == '__main__':
    unittest.main(verbosity=2)
