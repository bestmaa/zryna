"""Independent rejection controls for corpus inventory and exact-revision receipts."""

import copy
import importlib.util
import json
from pathlib import Path
import sys
import tempfile
import unittest

sys.dont_write_bytecode = True
ROOT = Path(__file__).resolve().parents[2]
SPEC = importlib.util.spec_from_file_location("corpus_runner", ROOT / "scripts/run-native-provider-corpus.py")
RUNNER = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(RUNNER)


class CorpusRunnerTests(unittest.TestCase):
    def test_strict_json_rejects_duplicate_fields_and_nonfinite_numbers(self):
        self.assertEqual(RUNNER.strict_json('{"ok":true,"number":1}'), {"ok": True, "number": 1})
        for content in ('{"passed":[],"passed":["invented"]}', '{"case":{"id":"a","id":"b"}}',
                        '{"value":NaN}', '{"value":Infinity}', '{"value":-Infinity}', '{"value":1e999}', '{"value":-1e999}'):
            with self.assertRaises(ValueError):
                RUNNER.strict_json(content)

    def test_frozen_inventory_includes_unregistered_parser_sources_and_all_registries(self):
        inventory = RUNNER.inventory(ROOT)
        for profile in ("m1", "m2", "m3"):
            self.assertIn(f"tests/{profile}-conformance-v1.json", inventory)
        self.assertEqual(sum(path.startswith("tests/m2-fixtures/") for path in inventory), 14)
        self.assertEqual(sum(path.startswith("tests/m3-fixtures/") for path in inventory), 95)
        self.assertIn("tests/m3-fixtures/borrow-exclusive-nonreference.zry", inventory)
        for digest in inventory.values():
            self.assertEqual(len(digest), 64)
            self.assertTrue(set(digest) <= set("0123456789abcdef"))

    def receipt(self):
        executed, blocked = RUNNER.expected_cases(ROOT)
        metadata = RUNNER.RECEIPTS.expected_metadata(ROOT)
        oracles = RUNNER.runtime_oracles(ROOT)
        records = []
        for name in executed + blocked:
            spec = metadata[name]
            record = {"id": name, "profile": spec["profile"], "source": spec["source"],
                      "syntax": "accepted", "semantic": "accepted", "artifacts": "matched",
                      "runtime": "not-run", "status": "pass" if name in executed else "blocked"}
            if name in blocked:
                record.update(syntax="not-applicable", semantic="not-applicable", artifacts="not-applicable", runtime="blocked")
            elif spec["expected"] == "syntax-rejected":
                record.update(syntax="rejected", semantic="not-entered", artifacts="not-emitted")
            elif spec["expected"] == "semantic-rejected":
                record.update(semantic="rejected", artifacts="not-emitted")
            elif spec["expected"] == "abi-rejected":
                record.update(artifacts="not-emitted", runtime="rejected")
            record["details"] = {key: record[key] for key in ("syntax", "semantic", "artifacts", "runtime")}
            if name in blocked:
                record["details"]["reason"] = "explicit unavailable owning interface"
            elif spec["expected"] in ("semantic-rejected", "syntax-rejected"):
                record["details"]["diagnostics"] = [{"code": code} for code in spec["codes"]]
            elif spec["expected"] == "abi-rejected":
                record["details"].update(abi_code=spec["codes"][0], owning_phase="scalar-abi-admission")
            if name in oracles:
                record["runtime"] = record["details"]["runtime"] = "executed"
                record["details"]["portable_observations"] = [
                    {"provider": provider, "target": target, "status": "executed", "outcome": oracles[name]}
                    for provider in ("bootstrap", "native") for target in ("javascript", "webassembly")]
                oracle = oracles[name]
                outcome = {"kind": "trapped", "code": oracle["value"]} if oracle["type"] == "trap" \
                    else {"kind": "returned", "value": oracle}
                record["details"]["native"] = {"status": "executed", "outcome": outcome} \
                    if RUNNER.RECEIPTS.native_supported() else {"status": "platform-unavailable"}
            if record["artifacts"] == "matched":
                record["details"]["artifact_hashes"] = {
                    target: {"sha256": "a" * 64, "bytes": 1} for target in ("javascript", "webassembly", "native-object")}
            records.append(record)
        return {"schema_version": 1, "public_activation": False, "cases": records,
                "passed": executed, "failed": [], "ignored": [], "blocked": blocked,
                "runtime_root": str(ROOT.parent / "external-runtime"),
                "limits": {name: "explicit boundary" for name in
                           ("frozen_parser_428", "native_execution", "source_inventory")}}

    def test_exact_census_and_dispositions_reject_missing_duplicate_and_false_success(self):
        inventory = RUNNER.inventory(ROOT)
        original = self.receipt()
        RUNNER.verify_receipt(original, inventory)
        mutations = []
        for name, value in (("schema_version", True), ("public_activation", 0),
                            ("public_activation", True), ("ignored", ["m1-source:valid"]),
                            ("failed", ["m1-source:valid"]), ("blocked", []),
                            ("runtime_root", "relative"), ("limits", {})):
            receipt = copy.deepcopy(original)
            receipt[name] = value
            mutations.append(receipt)
        for field in ("cases", "passed", "blocked"):
            for value in (original[field][1:], original[field] + original[field][:1]):
                receipt = copy.deepcopy(original)
                receipt[field] = value
                mutations.append(receipt)
        for field, value in (("status", "blocked"), ("syntax", True),
                             ("details", []), ("source", ""), ("id", "invented")):
            receipt = copy.deepcopy(original)
            receipt["cases"][0][field] = value
            mutations.append(receipt)
        runtime_index = next(index for index, case in enumerate(original["cases"])
                             if case["id"].startswith("m1-run:"))
        for key, value in (("portable_observations", []), ("artifact_hashes", {}), ("native", {})):
            receipt = copy.deepcopy(original)
            receipt["cases"][runtime_index]["details"][key] = value
            mutations.append(receipt)
        receipt = copy.deepcopy(original)
        receipt["cases"][runtime_index]["details"]["portable_observations"][-1] = \
            copy.deepcopy(receipt["cases"][runtime_index]["details"]["portable_observations"][0])
        mutations.append(receipt)
        receipt = copy.deepcopy(original)
        receipt["cases"][runtime_index]["details"]["artifact_hashes"]["javascript"]["bytes"] = True
        mutations.append(receipt)
        boolean_index = next(index for index, case in enumerate(original["cases"])
                             if case["id"] in RUNNER.runtime_oracles(ROOT)
                             and RUNNER.runtime_oracles(ROOT)[case["id"]]["type"] == "bool")
        receipt = copy.deepcopy(original)
        observation = receipt["cases"][boolean_index]["details"]["portable_observations"][0]
        observation["outcome"]["value"] = int(observation["outcome"]["value"])
        mutations.append(receipt)
        for receipt in mutations:
            with self.assertRaises(ValueError):
                RUNNER.verify_receipt(receipt, inventory)
        altered = copy.deepcopy(inventory)
        altered[next(iter(altered))] = "0" * 64
        with self.assertRaises(ValueError):
            RUNNER.verify_receipt(original, altered)

    def test_frozen_identity_and_owning_phase_cannot_be_substituted(self):
        original = self.receipt()
        mutations = []
        for key, value in (("profile", "other"), ("source", "fixture.zry")):
            receipt = copy.deepcopy(original)
            receipt["cases"][0][key] = value
            mutations.append(receipt)
        receipt = copy.deepcopy(original)
        for key in ("syntax", "semantic", "artifacts", "runtime"):
            receipt["cases"][0][key] = receipt["cases"][0]["details"][key] = "compared"
        mutations.append(receipt)
        receipt = copy.deepcopy(original)
        hostile = next(case for case in receipt["cases"] if case["id"] == "m3-source:borrow-exclusive-nonreference.zry")
        hostile["details"]["diagnostics"][0]["code"] = "invented"
        mutations.append(receipt)
        for receipt in mutations:
            with self.assertRaises(ValueError):
                RUNNER.verify_receipt(receipt, RUNNER.inventory(ROOT))

    def test_unsupported_native_host_requires_all_twenty_linked_blockers(self):
        from unittest.mock import patch
        with patch.object(RUNNER.RECEIPTS, "native_supported", return_value=False):
            receipt = self.receipt()
            self.assertEqual(len(receipt["cases"]), 240)
            self.assertEqual(len(receipt["blocked"]), 50)
            RUNNER.verify_receipt(receipt, RUNNER.inventory(ROOT))
            receipt["blocked"] = [name for name in receipt["blocked"] if not name.startswith("native-platform:")]
            with self.assertRaises(ValueError):
                RUNNER.verify_receipt(receipt, RUNNER.inventory(ROOT))

    def test_failed_receipt_is_validated_for_evidence_but_never_admitted_as_pass(self):
        receipt = self.receipt()
        record = receipt["cases"][0]
        receipt["passed"].remove(record["id"])
        receipt["failed"] = [record["id"]]
        record.update(status="fail", syntax="incomplete", semantic="incomplete",
                      artifacts="incomplete", runtime="incomplete", details={"error": "actual divergence"})
        RUNNER.verify_receipt(receipt, RUNNER.inventory(ROOT), allow_failed=True)
        with self.assertRaises(ValueError):
            RUNNER.verify_receipt(receipt, RUNNER.inventory(ROOT))
        receipt["failed"] = []
        with self.assertRaises(ValueError):
            RUNNER.verify_receipt(receipt, RUNNER.inventory(ROOT), allow_failed=True)

    def test_corpus_stdout_is_machine_readable_and_nonzero_exit_is_preserved(self):
        with tempfile.TemporaryDirectory() as owned:
            evidence = Path(owned)
            code = "import sys;print('diagnostic',file=sys.stderr);print('{\"failed\":[\"case\"]}');sys.exit(7)"
            result = RUNNER.run_corpus([sys.executable, "-c", code], evidence, evidence, {})
            self.assertEqual(result, 7)
            self.assertEqual(json.loads((evidence / "corpus.json").read_text()), {"failed": ["case"]})
            self.assertIn("diagnostic", (evidence / "corpus.stderr.log").read_text())

    def test_generated_manifest_has_only_original_registry_material_and_real_path_dependencies(self):
        lock = {"package": [{"name": name, "version": version} for name, version in
                            (("serde", "1.0.1"), ("serde_json", "1.0.2"), ("sha2", "0.10.9"))]}
        with tempfile.TemporaryDirectory() as owned:
            package = Path(owned)
            RUNNER.manifest(package, "0.2.3", lock)
            manifest = RUNNER.tomllib.loads((package / "Cargo.toml").read_text())
            self.assertEqual(manifest["workspace"], {})
            self.assertEqual(manifest["lints"]["rust"]["unsafe_code"], "forbid")
            for dependency in RUNNER.PATH_DEPENDENCIES:
                self.assertEqual(manifest["dependencies"][dependency]["path"],
                                 str(ROOT / "crates" / dependency))
            self.assertEqual(manifest["dependencies"]["sha2"]["version"], "=0.10.9")
            self.assertEqual(manifest["dependencies"]["serde"]["features"], ["derive"])
            forged = copy.deepcopy(lock)
            forged["package"].append({"name": "serde", "version": "other"})
            with self.assertRaises(ValueError):
                RUNNER.manifest(package, "0.2.3", forged)


if __name__ == "__main__":
    unittest.main()
