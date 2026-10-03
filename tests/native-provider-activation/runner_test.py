"""Independent fail-closed evidence and dependency controls for the activation smoke runner."""

import copy
import importlib.util
import json
import os
from pathlib import Path
import tempfile
import subprocess
import sys
import time
import unittest

SCRIPT = Path(__file__).resolve().parents[2] / "scripts/run-native-provider-activation.py"
SPEC = importlib.util.spec_from_file_location("activation_runner", SCRIPT)
RUNNER = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(RUNNER)


class RunnerTests(unittest.TestCase):
    def receipt(self, retained):
        return {"schema_version": 1, "provider": RUNNER.PROVIDER, "provider_version": "0.2.3",
                "passed": RUNNER.expected_cases(retained), "failed": [], "ignored": [],
                "retained": retained, "public_activation": False}

    def test_requires_all_exact_cases_once_and_no_ignored_or_activation(self):
        for retained in (False, True):
            receipt = self.receipt(retained)
            RUNNER.verify_receipt(receipt, retained, "0.2.3")
            mutations = []
            for key, value in (("schema_version", True), ("retained", int(retained)),
                               ("public_activation", 0), ("provider", "typescript-6"), ("provider_version", "stale"),
                               ("retained", not retained), ("public_activation", True),
                               ("failed", ["v2:identity"]), ("ignored", ["v2:identity"]),
                               ("schema_version", 2)):
                mutated = copy.deepcopy(receipt)
                mutated[key] = value
                mutations.append(mutated)
            for cases in ([], receipt["passed"][1:], receipt["passed"] * 2,
                          list(reversed(receipt["passed"]))):
                mutated = copy.deepcopy(receipt)
                mutated["passed"] = cases
                mutations.append(mutated)
            for mutated in mutations:
                with self.assertRaises(ValueError):
                    RUNNER.verify_receipt(mutated, retained, "0.2.3")

    def test_retained_is_a_separate_four_case_requirement(self):
        self.assertEqual(len(RUNNER.expected_cases(False)), 36)
        self.assertEqual(len(RUNNER.expected_cases(True)), 40)
        with self.assertRaises(ValueError):
            RUNNER.verify_receipt(self.receipt(False), True, "0.2.3")

    def test_runtime_environment_strips_all_bootstrap_and_compiler_influence(self):
        with tempfile.TemporaryDirectory() as owned:
            env = RUNNER.runtime_environment(Path(owned))
            self.assertEqual(env["PATH"], owned)
            self.assertTrue(set(env) <= {"PATH", "SYSTEMROOT", "SystemRoot", "WINDIR"})
            self.assertNotIn("NODE_PATH", env)
            self.assertNotIn("CARGO_HOME", env)

    def test_runtime_tool_presence_rejects(self):
        with tempfile.TemporaryDirectory() as owned:
            node = Path(owned) / ("node.exe" if os.name == "nt" else "node")
            node.write_bytes(b"forbidden")
            node.chmod(0o700)
            with self.assertRaises(ValueError):
                RUNNER.runtime_environment(Path(owned))

    def test_registry_material_must_be_in_original_lock_with_same_checksum(self):
        original = {"package": [{"name": "serde", "version": "1.0.0",
                                 "source": "registry+source", "checksum": "original"}]}
        RUNNER.verify_registry_lock(original, copy.deepcopy(original))
        RUNNER.verify_registry_lock(original, {"package": []})
        for field in ("name", "version", "source", "checksum"):
            forged = copy.deepcopy(original)
            forged["package"][0][field] = "substituted"
            with self.assertRaises(ValueError):
                RUNNER.verify_registry_lock(original, forged)

    def test_owned_directories_are_removed_only_on_success(self):
        with RUNNER.owned_directory("zryna-414-test-") as success:
            (success / "owned").write_text("fixture")
        self.assertFalse(success.exists())
        with self.assertRaises(RuntimeError):
            with RUNNER.owned_directory("zryna-414-test-") as failure:
                (failure / "owned").write_text("retain")
                raise RuntimeError("unconfirmed")
        self.assertTrue((failure / "owned").is_file())
        RUNNER.shutil.rmtree(failure)

    @unittest.skipUnless(os.name == "posix", "POSIX process-group timeout boundary")
    def test_timeout_terminates_descendant_before_owned_cleanup(self):
        with tempfile.TemporaryDirectory() as owned:
            root = Path(owned)
            marker = root / "orphan.marker"
            child = "import time,pathlib;time.sleep(0.5);pathlib.Path(%r).write_text('orphan')" % str(marker)
            parent = "import subprocess,time;subprocess.Popen(%r);time.sleep(10)" % [sys.executable, "-c", child]
            with self.assertRaises((subprocess.TimeoutExpired, RuntimeError)):
                RUNNER.run([sys.executable, "-c", parent], root, root / "timeout.log", timeout=0.1)
            time.sleep(0.6)
            self.assertFalse(marker.exists(), "descendant outlived the timeout boundary")

    def test_manifest_adds_only_existing_paths_and_exact_serde_pins(self):
        lock = {"package": [{"name": "serde", "version": "1.0.1"},
                            {"name": "serde_json", "version": "1.0.2"}]}
        with tempfile.TemporaryDirectory() as owned:
            package = Path(owned)
            RUNNER.manifest(package, True, "0.2.3", lock)
            manifest = RUNNER.tomllib.loads((package / "Cargo.toml").read_text())
            self.assertEqual(manifest["workspace"], {})
            self.assertEqual(manifest["features"]["default"], [])
            self.assertEqual(manifest["dependencies"]["serde"]["version"], "=1.0.1")
            self.assertEqual(manifest["dependencies"]["serde_json"]["version"], "=1.0.2")
            self.assertTrue(manifest["dependencies"]["zryna-driver"]["optional"])
            self.assertEqual(manifest["dependencies"]["zryna-driver"]["path"],
                             str(RUNNER.ROOT / "crates/zryna-driver"))
            self.assertEqual(len(manifest["features"]["retained"]), 4)
            RUNNER.manifest(package, False, "0.2.3", lock)
            frontend = RUNNER.tomllib.loads((package / "Cargo.toml").read_text())
            self.assertNotIn("zryna-driver", frontend["dependencies"])
            self.assertEqual(frontend["features"]["retained"], [])


if __name__ == "__main__":
    unittest.main()
