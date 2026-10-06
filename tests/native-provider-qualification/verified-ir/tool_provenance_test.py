"""Local modeled tool-path controls; never execute versions, compilers or providers."""
from contextlib import ExitStack, contextmanager
import os
from pathlib import Path
import stat
import tempfile
from types import SimpleNamespace
import unittest
from unittest import mock
import tool_provenance as d


class ToolProvenanceControls(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory(prefix='tool-provenance-controls-')
        self.base = Path(self.temporary.name)
        self.tools = self.base / 'real-tools'
        self.tools.mkdir()
        self.tool = self.tools / 'cargo'
        self.tool.write_bytes(b'modeled tool bytes; never execute\n')
        self.baseline = d.inspect_tool(self.tool)
        self.assertTrue(self.baseline['original_strict_guard_accepted'])

    def tearDown(self):
        self.temporary.cleanup()

    @contextmanager
    def modeled_link(self, proxy, target, parent=False):
        # Exercise identical lstat/readlink/resolve contracts without host link privileges.
        if parent:
            proxy.mkdir()
            (proxy / 'cargo').write_bytes(self.tool.read_bytes())
        else:
            proxy.write_bytes(self.tool.read_bytes())
        selected = [target]
        original_stat, original_resolve, original_readlink = Path.lstat, Path.resolve, d.os.readlink
        def linked(path):
            info = original_stat(path)
            if path != proxy:
                return info
            values = {name: getattr(info, name) for name in
                      ('st_dev', 'st_ino', 'st_size', 'st_mtime_ns', 'st_ctime_ns', 'st_nlink')}
            values.update(st_mode=stat.S_IFLNK | 0o777)
            return SimpleNamespace(**values)
        def resolve(path, *args, **kwargs):
            if path == proxy:
                return selected[0]
            if parent and path == proxy / 'cargo':
                return selected[0] / 'cargo'
            return original_resolve(path, *args, **kwargs)
        def readlink(path, *args, **kwargs):
            return str(selected[0]) if Path(path) == proxy else original_readlink(path, *args, **kwargs)
        with ExitStack() as stack:
            stack.enter_context(mock.patch.object(Path, 'lstat', linked))
            stack.enter_context(mock.patch.object(Path, 'resolve', resolve))
            stack.enter_context(mock.patch.object(d.os, 'readlink', readlink))
            yield selected

    def test_ordinary_positive_retains_exact_component_identity(self):
        row = d.inspect_tool(self.tool)
        self.assertEqual(row, self.baseline)
        self.assertEqual(row['canonical_bytes'], len(self.tool.read_bytes()))
        self.assertEqual(row['original_components_before'], row['original_components_after'])
        self.assertTrue(row['stable_components_and_tool_bytes'])

    def test_file_link_retains_original_rejection_and_canonical_bytes(self):
        proxy = self.base / 'proxy'
        with self.modeled_link(proxy, self.tool):
            row = d.inspect_tool(proxy)
        self.assertFalse(row['original_strict_guard_accepted'])
        self.assertEqual(row['observed_offending_component']['path'], str(proxy))
        self.assertEqual(row['observed_offending_component']['readlink_target'], str(self.tool))
        self.assertEqual(row['canonical_sha256'], self.baseline['canonical_sha256'])

    def test_parent_link_names_actual_offending_component(self):
        proxy = self.base / 'parent-proxy'
        with self.modeled_link(proxy, self.tools, parent=True):
            row = d.inspect_tool(proxy / 'cargo')
        self.assertFalse(row['original_strict_guard_accepted'])
        self.assertEqual(row['observed_offending_component']['path'], str(proxy))
        self.assertEqual(row['canonical_resolved_path'], str(self.tool))

    def test_link_and_parent_still_rejected_by_original_guard(self):
        for parent in (False, True):
            with self.subTest(parent=parent):
                proxy = self.base / ('parent' if parent else 'file')
                path = proxy / 'cargo' if parent else proxy
                with self.modeled_link(proxy, self.tools if parent else self.tool, parent):
                    with self.assertRaisesRegex(ValueError, 'no linked or reparse artifact parents'):
                        d.ir.read_bounded(path, d.MAX_TOOL)

    def test_changed_original_component_identity_rejects(self):
        original = d.components
        calls = []
        def changed(path):
            rows = original(path)
            calls.append(True)
            if len(calls) == 3:
                rows[0]['inode'] += 1
            return rows
        with mock.patch.object(d, 'components', changed), self.assertRaises(ValueError):
            d.inspect_tool(self.tool)

    def test_changed_capability_bytes_rejects(self):
        original = d.canonical_bytes
        calls = []
        def changed(path):
            calls.append(True)
            raw = original(path)
            return raw if len(calls) == 1 else raw + b'changed'
        with mock.patch.object(d, 'canonical_bytes', changed), self.assertRaises(ValueError):
            d.inspect_tool(self.tool)

    def test_changed_link_target_rejects(self):
        proxy = self.base / 'proxy'
        other = self.tools / 'other'
        other.write_bytes(self.tool.read_bytes())
        original = d.ir.read_bounded
        with self.modeled_link(proxy, self.tool) as target:
            def replace(path, limit):
                if path == proxy:
                    target[0] = other
                return original(path, limit)
            with mock.patch.object(d.ir, 'read_bounded', replace), self.assertRaises(ValueError):
                d.inspect_tool(proxy)

    def test_parent_content_time_changes_do_not_replace_capability_identity(self):
        original = d.components
        calls = []
        def changed(path):
            rows = original(path)
            calls.append(True)
            if len(calls) >= 3:
                rows[1]['mtime_ns'] += 1
                rows[1]['ctime_ns'] += 1
            return rows
        with mock.patch.object(d, 'components', changed):
            self.assertTrue(d.inspect_tool(self.tool)['stable_components_and_tool_bytes'])

    def modeled_queries(self, mutate=False, node_version='v22.22.1'):
        paths = {name: self.tools / name for name in ('rustup', 'node', 'rustc.exe' if os.name == 'nt' else 'rustc')}
        for path in paths.values():
            path.write_bytes(b'query model only; never execute\n')
        rustc = next(path for name, path in paths.items() if name.startswith('rustc'))
        calls = []
        def query(argv, env):
            args = list(map(str, argv)); calls.append(args)
            if args[1:] == ['which', '--toolchain', '1.97.1', 'cargo']:
                stdout = str(self.tool)
            elif args[1:] == ['which', '--toolchain', '1.97.1', 'rustc']:
                stdout = str(rustc)
            elif args[1:] == ['--version']:
                stdout = node_version if args[0] == str(paths['node']) else ('rustc 1.97.1 model' if args[0] == str(rustc) else 'cargo 1.97.1 model')
            else:
                raise AssertionError('unapproved query: ' + repr(args))
            if mutate and len(calls) == 6:
                self.tool.write_bytes(b'changed selected capability after query\n')
            return dict(argv=args, stdout=stdout + '\n')
        with mock.patch.object(d.shutil, 'which', return_value=str(self.tool)), mock.patch.object(d, 'query', query):
            result = d.diagnose(self.tool, paths['rustup'], paths['node'])
        self.assertEqual(len(calls), 7)
        return result

    def test_full_selection_uses_only_modeled_metadata_queries(self):
        row = self.modeled_queries()
        self.assertEqual(row['status'], 'UNQUALIFIED')
        self.assertFalse(row['IR_admission'])
        self.assertEqual(row['compiler_provider_executions'], 0)
        self.assertEqual(row['builds'], 0)
        self.assertEqual(set(row['selected_rows']), {'cargo', 'rustc', 'node', 'rustup', 'supplied_cargo'})

    def test_capability_change_across_metadata_queries_rejects(self):
        with self.assertRaisesRegex(ValueError, 'capability changed'):
            self.modeled_queries(mutate=True)

    def test_full_selection_rejects_Node_version_prefix_collision(self):
        with self.assertRaisesRegex(ValueError, 'exact pinned Node metadata version'):
            self.modeled_queries(node_version='v22.22.10')

    def test_reparse_model_guard_is_unchanged(self):
        original = Path.lstat
        def reparse(path):
            info = original(path)
            if path != self.tool:
                return info
            return SimpleNamespace(st_mode=info.st_mode, st_file_attributes=0x400)
        with mock.patch.object(Path, 'lstat', reparse), self.assertRaisesRegex(ValueError, 'reparse'):
            d.ir.read_bounded(self.tool, d.MAX_TOOL)

    def test_special_tool_type_rejects(self):
        original = Path.lstat
        def special(path):
            info = original(path)
            if path != self.tool:
                return info
            return SimpleNamespace(st_mode=stat.S_IFIFO | 0o600)
        with mock.patch.object(Path, 'lstat', special), self.assertRaises(ValueError):
            d.canonical_bytes(self.tool)


if __name__ == '__main__':
    unittest.main(verbosity=2)
