"""Selected helper metadata only; no installed capture, probe or sandbox acceptance."""
import copy
import pathlib
import sys
import unittest

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parents[1] /
                      "examples/playground/restricted"))
from errors import PolicyError
from host import validate_helper_receipt


def fixture():
    helpers = [
        {"role": "python", "path": "/usr/bin/python3.12", "bytes": 1,
         "sha256": "a" * 64, "version": "3.12.3"},
        {"role": "bubblewrap", "path":
         "/usr/local/libexec/zryna-playground-check/bubblewrap-0.12.0/bin/bwrap",
         "bytes": 97_040, "sha256":
         "f64e9068e26f30246f2409b134c4deed8bac88a54af207eff8d81410404e19f8", "version": "0.12.0"},
    ]
    libraries = [
        {"path": "/usr/lib/x86_64-linux-gnu/ld-linux-x86-64.so.2", "bytes": 236_616,
         "sha256": "cd4df4f3c7b83673d61189bf2eaebd33ca4f2853ab9772b8a25e025ef99b1e81"},
        {"path": "/usr/lib/x86_64-linux-gnu/libc.so.6", "bytes": 2_125_328,
         "sha256": "8db37cf3f2169f59a0f07ef1fea308c35656668c64c8ff294e1860f4121eb161"},
        {"path": "/usr/lib/x86_64-linux-gnu/libcap.so.2.66", "bytes": 51_536,
         "sha256": "6ac6abc86ac891c6e13486470e26f1d939f47fda9e6b5d5508a7f5ec881adc84"},
    ]
    return helpers, libraries


class HelperPolicy(unittest.TestCase):
    def test_independently_selected_candidate_metadata_is_recognized_without_executing_it(self):
        helpers, libraries = fixture()
        self.assertIsNone(validate_helper_receipt(helpers, libraries))
        self.assertIsNone(validate_helper_receipt(list(reversed(helpers)), libraries))

    def test_legacy_global_path_other_prefix_version_size_or_bytes_cannot_replace_fixed_helper(self):
        for change in [{"path": "/usr/bin/bwrap"}, {"path": "/tmp/bwrap"},
                       {"version": "0.9.0"}, {"version": "0.12.1"},
                       {"bytes": 97_039}, {"sha256": "b" * 64}, {"extra": True}]:
            helpers, libraries = fixture()
            helpers[1].update(change)
            with self.subTest(change=change), self.assertRaises(PolicyError):
                validate_helper_receipt(helpers, libraries)

    def test_exact_recursive_library_identities_are_mandatory(self):
        for index in range(3):
            for change in [{"path": "/tmp/lib.so"}, {"bytes": 1}, {"sha256": "b" * 64}, {"extra": True}]:
                helpers, libraries = fixture()
                libraries[index].update(change)
                with self.subTest(index=index, change=change), self.assertRaises(PolicyError):
                    validate_helper_receipt(helpers, libraries)
        helpers, libraries = fixture()
        for other in [libraries[:-1], libraries + [copy.deepcopy(libraries[0])], list(reversed(libraries))]:
            with self.assertRaises(PolicyError):
                validate_helper_receipt(helpers, other)

    def test_python_identity_stays_closed_and_bounded(self):
        for change in [{"path": "/usr/bin/python3"}, {"version": "3.14.4"}, {"bytes": 0},
                       {"bytes": True}, {"bytes": 134_217_729}, {"sha256": "A" * 64},
                       {"sha256": None}, {"version": None}, {"extra": True}]:
            helpers, libraries = fixture()
            helpers[0].update(change)
            with self.subTest(change=change), self.assertRaises(PolicyError):
                validate_helper_receipt(helpers, libraries)

    def test_missing_duplicate_or_unknown_helper_roles_cannot_add_authority(self):
        helpers, libraries = fixture()
        for other in [None, {}, [], helpers[:1], helpers + [helpers[0]], [helpers[0], helpers[0]],
                      [None, helpers[1]], [{**helpers[0], "role": "shell"}, helpers[1]]]:
            with self.assertRaises(PolicyError):
                validate_helper_receipt(other, libraries)


if __name__ == "__main__":
    unittest.main()
