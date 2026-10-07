#!/usr/bin/env python3
"""Independent controls for the narrowly reviewed installation-source registration."""
import ast
import pathlib
import subprocess
import unittest

ROOT = pathlib.Path(__file__).resolve().parents[3]
GUARD = ROOT / 'tests/native-provider-qualification/verified-ir/run.py'
OWNED = (
    'crates/zryna-driver/src/distribution/mod.rs',
    'crates/zryna-driver/src/distribution/native_installation/descriptor.rs',
    'crates/zryna-driver/src/distribution/native_installation/descriptor_tests.rs',
    'crates/zryna-driver/src/distribution/native_installation/identity.rs',
    'crates/zryna-driver/src/distribution/native_installation/mod.rs',
    'crates/zryna-driver/src/distribution/native_installation/sources.rs',
    'tests/native-provider-qualification/installed-native-capability/README.md',
    'tests/native-provider-qualification/installed-native-capability/build.py',
    'tests/native-provider-qualification/installed-native-capability/cases.py',
    'tests/native-provider-qualification/installed-native-capability/independent_cases.py',
    'tests/native-provider-qualification/installed-native-capability/mutations.rs',
    'tests/native-provider-qualification/installed-native-capability/probe.rs',
    'tests/native-provider-qualification/installed-native-capability/run.py',
    'tests/native-provider-qualification/installed-native-capability/source_registration_test.py',
    'tests/native-provider-qualification/installed-native-capability/windows_build.py',
    'tests/native-provider-qualification/installed-native-capability/windows_build_test.py',
    'tests/native-provider-qualification/installed-native-capability/toolchain.py',
    'tests/native-provider-qualification/installed-native-capability/ci.py',
    'tests/native-provider-qualification/installed-native-capability/admission.py',
    'tests/native-provider-qualification/installed-native-capability/admission_build.py',
    'tests/native-provider-qualification/installed-native-capability/admission_test.py',
    'tests/native-provider-qualification/installed-native-capability/admission_original_controls.py',
    'tests/native-provider-qualification/installed-native-capability/windows-workflow.test.mjs',
    'tests/native-provider-qualification/installed-native-capability/windows-workflow-job.json',
)


class FakeGit:
    def __init__(self, row):
        self.row = row
        self.argv = None

    def git(self, *argv):
        self.argv = argv
        return self.row


def load_guard():
    tree = ast.parse(GUARD.read_text())
    selected = []
    names = {'RESERVATION', 'REGISTRATION', 'INSTALLATION_PREPARATION'}
    for node in tree.body:
        if isinstance(node, ast.Assign) and len(node.targets) == 1:
            target = node.targets[0]
            if isinstance(target, ast.Name) and target.id in names:
                selected.append(node)
        if isinstance(node, ast.FunctionDef) and node.name == 'qualified_change':
            selected.append(node)
    namespace = {}
    exec(compile(ast.Module(body=selected, type_ignores=[]), str(GUARD), 'exec'), namespace)
    return namespace


class RegistrationControls(unittest.TestCase):
    def setUp(self):
        self.guard = load_guard()
        self.pins = self.guard['INSTALLATION_PREPARATION']
        self.path = OWNED[0]
        self.pin = self.pins[self.path]

    def check_row(self, row, path=None):
        fake = FakeGit(row)
        self.guard['SUPPORT'] = fake
        selected = path or self.path
        result = self.guard['qualified_change'](selected)
        if selected in self.pins:
            self.assertEqual(fake.argv, ('ls-tree', 'HEAD', '--', selected))
        return result

    def row(self, mode=None, kind='blob', blob=None, path=None):
        return f"{mode or self.pin['mode']} {kind} {blob or self.pin['blob']}\t{path or self.path}"

    def test_exact_closed_installation_path_census(self):
        self.assertEqual(set(self.pins), set(OWNED))
        for path, pin in self.pins.items():
            self.assertEqual(set(pin), {'mode', 'blob'})
            self.assertIn(pin['mode'], ('100644', '100755'))
            self.assertRegex(pin['blob'], r'^[a-f0-9]{40}$')
            row = subprocess.check_output(['git', '-C', str(ROOT), 'ls-tree', 'HEAD', '--', path]).decode().strip()
            self.assertEqual(row, f"{pin['mode']} blob {pin['blob']}\t{path}")

    def test_every_exact_reviewed_installation_blob_accepts(self):
        for path, pin in self.pins.items():
            self.assertTrue(self.check_row(f"{pin['mode']} blob {pin['blob']}\t{path}", path))

    def test_every_changed_installation_blob_rejects(self):
        for path, pin in self.pins.items():
            wrong = '0' * 40 if pin['blob'] != '0' * 40 else 'f' * 40
            self.assertFalse(self.check_row(f"{pin['mode']} blob {wrong}\t{path}", path))

    def test_changed_git_mode_rejects(self):
        self.assertFalse(self.check_row(self.row(mode='100755' if self.pin['mode'] == '100644' else '100644')))

    def test_link_mode_rejects(self):
        self.assertFalse(self.check_row(self.row(mode='120000')))

    def test_nonblob_git_kind_rejects(self):
        self.assertFalse(self.check_row(self.row(kind='tree')))

    def test_foreign_reported_path_rejects(self):
        self.assertFalse(self.check_row(self.row(path='crates/zryna-driver/src/pipeline/mod.rs')))

    def test_missing_tree_row_rejects(self):
        self.assertFalse(self.check_row(''))

    def test_extra_tree_row_rejects(self):
        self.assertFalse(self.check_row(self.row() + '\n' + self.row()))

    def test_extra_metadata_field_rejects(self):
        self.assertFalse(self.check_row('extra ' + self.row()))

    def test_unknown_installation_sibling_rejects(self):
        self.assertFalse(self.check_row('', 'crates/zryna-driver/src/distribution/native_installation/unreviewed.rs'))

    def test_unrelated_production_source_rejects(self):
        self.assertFalse(self.check_row('', 'crates/zryna-driver/src/pipeline/mod.rs'))
        self.assertFalse(self.check_row('', 'crates/zryna-source/src/native_sources.rs'))

    def test_existing_ir_reservation_retains_original_policy(self):
        self.assertTrue(self.check_row('', self.guard['RESERVATION'] + 'run.py'))

    def test_existing_explicit_registration_retains_original_policy(self):
        self.assertEqual(self.guard['REGISTRATION'], {
            '.github/workflows/native-provider-activation.yml',
            'tests/native-cli-smoke/workflow-job.json',
            'tests/native-provider-activation-workflow.test.mjs',
        })
        for path in self.guard['REGISTRATION']:
            self.assertTrue(self.check_row('', path))


if __name__ == '__main__':
    unittest.main(verbosity=2)
