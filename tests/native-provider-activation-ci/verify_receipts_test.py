"""Mutation checks for CI receipt admission; fixtures are not native execution proof."""

import copy
import importlib.util
import json
from pathlib import Path
import sys
import tempfile
import unittest

sys.dont_write_bytecode = True
SPEC = importlib.util.spec_from_file_location("ci_receipts", Path(__file__).with_name("verify_receipts.py"))
VERIFY = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(VERIFY)

# Independent spelling/order fixture; never import the consumer's expected-case generator.
FRONTEND_CASES = """
v2:accepted v2:identity v2:version v2:protocol v2:module v2:semantic v2:extra
v2:id v2:snapshot-version v2:snapshot-path v2:frame
v3:accepted v3:identity v3:version v3:protocol v3:module v3:semantic v3:extra
v3:id v3:snapshot-version v3:snapshot-path v3:frame v3:control
v4:accepted v4:identity v4:version v4:protocol v4:module v4:semantic v4:extra
v4:id v4:snapshot-version v4:snapshot-path v4:frame v4:control v4:ownership
""".split()
RETAINED_CASES = ["retained-v2:dispatch", "retained-v3:dispatch", "retained-v4:dispatch",
                  "retained:stale-source-denied"]


def fixture(retained=False, platform="posix"):
    suffix = ".exe" if platform == "nt" else ""
    return {
        "repository_sha": VERIFY.CONSUMER_SHA, "dependency_sha": VERIFY.DEPENDENCY_SHA,
        "platform": platform, "retained": retained, "status": "passed", "public_activation": False,
        "temporary_directories_removed": True, "runtime_path_tools": [],
        "build_directory": "/fixture/build", "installation_directory": "/fixture/install",
        "cargo_version": "cargo 1.97.1 (fixture)", "rustc_version": "rustc 1.97.1 (fixture)",
        "repository_lock_sha256": "a" * 64, "harness_lock_sha256": "b" * 64,
        "executable_sha256": "c" * 64,
        "installation_entries": ["empty-path", "fixtures", "unrelated-cwd", VERIFY.PROVIDER + suffix],
        "smoke": {"schema_version": 1, "provider": VERIFY.PROVIDER, "provider_version": "0.2.3",
                  "passed": FRONTEND_CASES + (RETAINED_CASES if retained else []),
                  "failed": [], "ignored": [], "retained": retained, "public_activation": False},
    }


class ReceiptTests(unittest.TestCase):
    def test_complete_independent_linux_and_windows_case_fixtures(self):
        for platform in ("posix", "nt"):
            for retained in (False, True):
                receipt = fixture(retained, platform)
                VERIFY.verify_receipt(receipt, retained, platform)
                self.assertEqual(len(receipt["smoke"]["passed"]), 40 if retained else 36)

    def test_wrong_source_platform_lane_toolchain_and_activation_reject(self):
        for field, value in (("repository_sha", "d" * 40), ("dependency_sha", "e" * 40),
                             ("platform", "nt"), ("retained", True), ("retained", 0),
                             ("status", "failed"), ("public_activation", True),
                             ("public_activation", 0), ("temporary_directories_removed", False),
                             ("runtime_path_tools", ["node"]), ("cargo_version", "cargo stale"),
                             ("rustc_version", "rustc stale"), ("error", "unconfirmed cleanup"),
                             ("build_directory", ""), ("installation_directory", 0),
                             ("extra", "widened")):
            with self.subTest(field=field, value=value):
                forged = fixture()
                forged[field] = value
                with self.assertRaises(ValueError):
                    VERIFY.verify_receipt(forged, False, "posix")

    def test_zero_partial_duplicate_reordered_and_wrong_cases_reject(self):
        for cases in ([], FRONTEND_CASES[1:], FRONTEND_CASES * 2, list(reversed(FRONTEND_CASES)),
                      FRONTEND_CASES[:-1] + ["unrelated"], FRONTEND_CASES + RETAINED_CASES):
            forged = fixture()
            forged["smoke"]["passed"] = cases
            with self.assertRaises(ValueError):
                VERIFY.verify_receipt(forged, False, "posix")

    def test_retained_dispatch_or_stale_source_proof_cannot_be_omitted(self):
        for case in RETAINED_CASES:
            forged = fixture(True)
            forged["smoke"]["passed"].remove(case)
            with self.assertRaises(ValueError):
                VERIFY.verify_receipt(forged, True, "posix")

    def test_failed_ignored_wrong_identity_and_json_type_mutations_reject(self):
        for field, value in (("failed", ["v2:identity"]), ("ignored", ["v2:identity"]),
                             ("schema_version", True), ("schema_version", 2),
                             ("provider", "typescript-6"), ("provider_version", "stale"),
                             ("retained", 0), ("public_activation", 0),
                             ("public_activation", True), ("extra", False)):
            forged = fixture()
            forged["smoke"][field] = value
            with self.assertRaises(ValueError):
                VERIFY.verify_receipt(forged, False, "posix")

    def test_invalid_or_missing_hash_identity_and_widened_installation_reject(self):
        for field in ("repository_lock_sha256", "harness_lock_sha256", "executable_sha256"):
            for value in (None, "a" * 63, "G" * 64, True):
                forged = fixture()
                forged[field] = value
                with self.assertRaises(ValueError):
                    VERIFY.verify_receipt(forged, False, "posix")
        forged = fixture()
        forged["installation_entries"].append("node_modules")
        with self.assertRaises(ValueError):
            VERIFY.verify_receipt(forged, False, "posix")

    def test_missing_fields_or_nonobject_receipts_reject(self):
        for field in fixture():
            forged = fixture()
            del forged[field]
            with self.assertRaises(ValueError):
                VERIFY.verify_receipt(forged, False, "posix")
        for value in (None, [], True, "passed"):
            with self.assertRaises(ValueError):
                VERIFY.verify_receipt(value, False, "posix")

    def test_json_duplicate_fields_and_trailing_frames_reject(self):
        with tempfile.TemporaryDirectory() as owned:
            path = Path(owned) / "receipt.json"
            payload = json.dumps(fixture())
            path.write_text(payload, encoding="utf-8")
            self.assertEqual(VERIFY.read_json(path), fixture())
            for forged in ('{"status":"failed","status":"passed"}', payload + "\n{}"):
                path.write_text(forged, encoding="utf-8")
                with self.assertRaises(ValueError):
                    VERIFY.read_json(path)

    def test_source_lane_fixture_is_independent_from_expected_case_helper(self):
        for retained in (False, True):
            expected = FRONTEND_CASES + (RETAINED_CASES if retained else [])
            self.assertEqual(VERIFY.expected_cases(retained), expected)
        self.assertNotEqual(fixture()["smoke"], copy.deepcopy(fixture(True)["smoke"]))


if __name__ == "__main__":
    unittest.main()
