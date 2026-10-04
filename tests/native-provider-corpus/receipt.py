"""Exact typed census and admission rules for private frozen-provider evidence."""

import json
import math
import platform
import sys
from pathlib import Path

def strict_json(content):
    def pairs(items):
        result = {}
        for key, value in items:
            if key in result:
                raise ValueError(f"duplicate JSON object key: {key}")
            result[key] = value
        return result

    def constant(value):
        raise ValueError(f"invalid JSON number: {value}")

    def finite(value):
        number = float(value)
        if not math.isfinite(number):
            raise ValueError(f"nonfinite JSON number: {value}")
        return number

    return json.loads(content, object_pairs_hook=pairs, parse_constant=constant, parse_float=finite)



def native_supported():
    return sys.platform == "linux" and platform.machine().lower() in {"x86_64", "amd64"}


def expected_metadata(root):
    registries = {profile: strict_json((root / f"tests/{profile}-conformance-v1.json").read_text())
                  for profile in ("m1", "m2", "m3")}
    metadata = {}
    def add(case_id, profile, source, expected="any", codes=(), exact_codes=False):
        metadata[case_id] = {"profile": profile, "source": source, "expected": expected,
                             "codes": list(codes), "exact_codes": exact_codes}
    m1, m2, m3 = (registries[profile] for profile in ("m1", "m2", "m3"))
    add("m1-source:valid", "m1", m1["entrypoint"], "accepted")
    for case in m1["cases"]:
        add("m1-run:" + case["id"], "m1", m1["entrypoint"], "accepted")
    for case_id, key in (("invalid-any", "invalidSource"), ("bool-gated", "gatedBooleanSource")):
        add("m1-source:" + case_id, "m1", m1[key]["path"], "semantic-rejected", [m1[key]["expectedCode"]])
    for profile in ("m2", "m3"):
        base = root / f"tests/{profile}-fixtures"
        for source in sorted(base.rglob("*.zry")):
            relative = source.relative_to(base).as_posix()
            add(f"{profile}-source:{relative}", profile, source.relative_to(root).as_posix(),
                "syntax-accepted" if profile == "m3" else "any")
    metadata["m3-source:borrow-exclusive-nonreference.zry"].update(
        expected="syntax-rejected", codes=["ZRYNA-Y4002"], exact_codes=True)
    for case in m2["invalidCases"]:
        for case_id in ("m2-invalid:" + case["id"],
                        "m2-source:" + case["entrypoint"].removeprefix("tests/m2-fixtures/")):
            expected = "syntax-rejected" if case["diagnosticCodes"][0].startswith(("ZRYNA-F", "ZRYNA-D")) \
                else "semantic-rejected"
            add(case_id, "m2", case["entrypoint"], expected, case["diagnosticCodes"], True)
    for case in m2["validCases"]:
        add("m2-run:" + case["id"], "m2", m2["graph"]["entrypoint"], "accepted")
    fixtures = {fixture["id"]: fixture for fixture in m3["fixtures"]}
    required_valid = {case["fixture"] for key in ("valid", "runtimeInvalid", "faults") for case in m3[key]}
    for fixture in m3["fixtures"]:
        add("m3-context:" + fixture["id"], "m3", fixture["path"],
            "accepted" if fixture["id"] in required_valid else "any")
    for key, prefix in (("valid", "m3-run"), ("runtimeInvalid", "m3-runtime-invalid")):
        for case in m3[key]:
            add(prefix + ":" + case["id"], "m3", fixtures[case["fixture"]]["path"], "accepted")
    for case in m3["invalid"]:
        source = fixtures[case["fixture"]]["path"]
        expected = "abi-rejected" if case["code"] == "ZRYNA-B2102" else "semantic-rejected"
        add("m3-invalid:" + case["id"], "m3", source, expected, [case["code"]])
        metadata["m3-context:" + case["fixture"]].update(
            expected="accepted" if expected == "abi-rejected" else expected,
            codes=[] if expected == "abi-rejected" else [case["code"]])
    for case in m3["faults"]:
        add("m3-fault:" + case["id"], "m3", "", "blocked")
    for case_id, profile in (("production-manifest-parity", "m1-m3"), ("ordinary-no-node-cli", "m1-m3"),
                             ("m2-native-execution", "m2"), ("m3-native-fault-observation", "m3")):
        add(case_id, profile, "", "blocked")
    if not native_supported():
        for case_id, spec in list(metadata.items()):
            if case_id.startswith(("m1-run:", "m3-run:", "m3-runtime-invalid:")):
                add("native-platform:" + case_id, spec["profile"], "", "blocked")
    return metadata


def expected_cases(root):
    m1 = json.loads((root / "tests/m1-conformance-v1.json").read_text())
    m2 = json.loads((root / "tests/m2-conformance-v1.json").read_text())
    m3 = json.loads((root / "tests/m3-conformance-v1.json").read_text())
    executed = ["m1-source:valid", "m1-source:invalid-any", "m1-source:bool-gated"]
    executed.extend("m1-run:" + case["id"] for case in m1["cases"])
    for profile in ("m2", "m3"):
        base = root / f"tests/{profile}-fixtures"
        executed.extend(f"{profile}-source:" + source.relative_to(base).as_posix()
                        for source in sorted(base.rglob("*.zry")))
    for prefix, cases in (("m2-invalid", m2["invalidCases"]), ("m2-run", m2["validCases"]),
                          ("m3-context", m3["fixtures"]), ("m3-run", m3["valid"]),
                          ("m3-invalid", m3["invalid"]),
                          ("m3-runtime-invalid", m3["runtimeInvalid"])):
        executed.extend(prefix + ":" + case["id"] for case in cases)
    blocked = ["m3-fault:" + case["id"] for case in m3["faults"]]
    blocked.extend(("production-manifest-parity", "m2-native-execution",
                    "m3-native-fault-observation", "ordinary-no-node-cli"))
    if len(executed) != 190 or len(blocked) != 30:
        raise ValueError("frozen executable/blocked case census changed")
    if not native_supported():
        blocked.extend("native-platform:" + case_id for case_id in executed
                       if case_id.startswith(("m1-run:", "m3-run:", "m3-runtime-invalid:")))
    return executed, blocked



def runtime_oracles(root):
    result = {}
    for profile, key in (("m1", "cases"), ("m2", "validCases")):
        registry = json.loads((root / f"tests/{profile}-conformance-v1.json").read_text())
        result.update({f"{profile}-run:" + case["id"]: case["expected"] for case in registry[key]})
    m3 = json.loads((root / "tests/m3-conformance-v1.json").read_text())
    result.update({"m3-run:" + case["id"]: {"type": "i32", "value": case["expected"]}
                   for case in m3["valid"]})
    result.update({"m3-runtime-invalid:" + case["id"]:
                   {"type": "trap", "value": case["expectedTrap"]} for case in m3["runtimeInvalid"]})
    return result


def exact_json(left, right):
    return json.dumps(left, sort_keys=True, separators=(",", ":")) == \
        json.dumps(right, sort_keys=True, separators=(",", ":"))



def validate_dispositions(case, spec):
    state = tuple(case[name] for name in ("syntax", "semantic", "artifacts", "runtime"))
    allowed = {("accepted", "accepted", "matched", "not-run"),
               ("accepted", "accepted", "matched", "executed"),
               ("accepted", "rejected", "not-emitted", "not-run"),
               ("rejected", "not-entered", "not-emitted", "not-run")}
    if spec["expected"] == "abi-rejected":
        allowed = {("accepted", "accepted", "not-emitted", "rejected")}
        if case["details"].get("abi_code") != spec["codes"][0] \
                or case["details"].get("owning_phase") != "scalar-abi-admission":
            raise ValueError("fixed ABI negative must reject at scalar ABI admission")
    if state not in allowed:
        raise ValueError("corpus owning-phase dispositions are inconsistent")
    expectation = spec["expected"]
    if expectation == "accepted" and case["semantic"] != "accepted":
        raise ValueError("registered source-positive cannot be rejected")
    if expectation == "syntax-accepted" and case["syntax"] != "accepted":
        raise ValueError("frozen admitted v4 source must reach verified syntax")
    if expectation == "syntax-rejected" and case["syntax"] != "rejected":
        raise ValueError("known verifier-hostile source must be rejected")
    if expectation == "semantic-rejected" and case["semantic"] != "rejected":
        raise ValueError("fixed source-negative must reject semantic admission")
    if expectation == "rejected" and case["semantic"] == "accepted":
        raise ValueError("frozen negative cannot become accepted")
    if case["syntax"] == "rejected" or case["semantic"] == "rejected":
        diagnostics = case["details"].get("diagnostics")
        if type(diagnostics) is dict:
            if set(diagnostics) != {"schema_version", "diagnostics"} \
                    or type(diagnostics["schema_version"]) is not int or diagnostics["schema_version"] != 1:
                raise ValueError("rendered diagnostics have the wrong exact schema")
            diagnostics = diagnostics["diagnostics"]
        if type(diagnostics) is not list or not diagnostics:
            raise ValueError("matching rejection must retain actual diagnostics")
        if any(type(item) is not dict or type(item.get("code")) is not str or not item["code"]
               for item in diagnostics):
            raise ValueError("rejection diagnostics must carry actual owning codes")
        codes = [item["code"] for item in diagnostics]
        if spec["exact_codes"] and codes != spec["codes"]:
            raise ValueError("fixed diagnostic code sequence changed")
        if not set(spec["codes"]) <= set(codes):
            raise ValueError("fixed diagnostic code is missing")


def verify_receipt(receipt, inputs, *, root, expected_inputs, allow_failed=False):
    if type(receipt) is not dict or set(receipt) != {
            "schema_version", "public_activation", "cases", "passed", "failed", "ignored", "blocked",
            "runtime_root", "limits"}:
        raise ValueError("corpus receipt has the wrong exact schema")
    if type(receipt["schema_version"]) is not int or receipt["schema_version"] != 1:
        raise ValueError("corpus receipt schema_version must be exact integer 1")
    if type(receipt["public_activation"]) is not bool or receipt["public_activation"]:
        raise ValueError("public activation must remain exact false")
    if type(receipt["runtime_root"]) is not str or not Path(receipt["runtime_root"]).is_absolute():
        raise ValueError("corpus runtime root must be an absolute path")
    if type(receipt["limits"]) is not dict or set(receipt["limits"]) != {
            "frozen_parser_428", "native_execution", "source_inventory"}:
        raise ValueError("corpus limits must explicitly describe the three coverage boundaries")
    if any(type(value) is not str or not value for value in receipt["limits"].values()):
        raise ValueError("corpus limits must be nonempty strings")
    for name in ("cases", "passed", "failed", "ignored", "blocked"):
        if type(receipt[name]) is not list:
            raise ValueError(f"corpus receipt {name} must be an array")
    for name in ("passed", "failed", "ignored", "blocked"):
        if any(type(case) is not str for case in receipt[name]):
            raise ValueError(f"corpus receipt {name} IDs must be strings")
        if len(set(receipt[name])) != len(receipt[name]):
            raise ValueError(f"duplicate corpus receipt {name} ID")
    if receipt["ignored"] or (receipt["failed"] and not allow_failed):
        raise ValueError("a failed or ignored corpus comparison cannot be admitted")
    executed, blocked = expected_cases(root)
    oracles = runtime_oracles(root)
    metadata = expected_metadata(root)
    if (set(receipt["passed"]) | set(receipt["failed"]) != set(executed) \
            or set(receipt["passed"]) & set(receipt["failed"]) \
            or set(receipt["blocked"]) != set(blocked)):
        raise ValueError("corpus receipt must account for all exact executed and blocked cases")
    records = {}
    for case in receipt["cases"]:
        fields = {"id", "profile", "source", "syntax", "semantic", "artifacts", "runtime", "status", "details"}
        if type(case) is not dict or set(case) != fields:
            raise ValueError("corpus case has the wrong exact schema")
        if any(type(case[name]) is not str or not case[name] for name in fields - {"details", "source"}):
            raise ValueError("corpus case identity and dispositions must be nonempty strings")
        if type(case["source"]) is not str or (case["status"] != "blocked" and not case["source"]):
            raise ValueError("executed corpus case must identify its source")
        if type(case["details"]) is not dict:
            raise ValueError("corpus case evidence must be a JSON object")
        if case["id"] in records:
            raise ValueError("duplicate corpus case record")
        spec = metadata.get(case["id"])
        if spec is None or case["profile"] != spec["profile"] or case["source"] != spec["source"]:
            raise ValueError("corpus identity does not match its frozen profile/source")
        records[case["id"]] = case
    if set(records) != set(executed + blocked):
        raise ValueError("missing or unexpected corpus case record")
    for case_id, case in records.items():
        required_status = "fail" if case_id in receipt["failed"] else \
            ("pass" if case_id in executed else "blocked")
        if case["status"] != required_status:
            raise ValueError("corpus record status contradicts its disposition")
    for case_id, case in records.items():
        if case["status"] == "fail":
            error = case["details"].get("error")
            if type(error) is not str or not error or len(error) > 8192:
                raise ValueError("failed comparison must retain its bounded actual assertion error")
            continue
        if case["status"] == "blocked":
            if (case["syntax"], case["semantic"], case["artifacts"], case["runtime"]) != (
                    "not-applicable", "not-applicable", "not-applicable", "blocked"):
                raise ValueError("blocked obligation cannot claim executed compiler work")
            if type(case["details"].get("reason")) is not str or not case["details"]["reason"]:
                raise ValueError("blocked obligation must retain its actual reason")
            continue
        validate_dispositions(case, metadata[case_id])
        if case_id in executed and any(case[name] != case["details"].get(name)
                                       for name in ("syntax", "semantic", "artifacts", "runtime")):
            raise ValueError("case dispositions disagree with actual comparison details")
        if case_id.startswith(("m1-run:", "m2-run:", "m3-run:", "m3-runtime-invalid:")):
            if any(case[name] != value for name, value in
                   (("syntax", "accepted"), ("semantic", "accepted"),
                    ("artifacts", "matched"), ("runtime", "executed"))):
                raise ValueError("fixed runtime case must execute accepted verified artifacts")
            observations = case["details"].get("portable_observations")
            if type(observations) is not list or len(observations) != 4:
                raise ValueError("fixed runtime case requires all four actual portable observations")
            pairs = set()
            for observation in observations:
                if type(observation) is not dict or set(observation) != {"provider", "target", "status", "outcome"}:
                    raise ValueError("portable observation has the wrong exact schema")
                pair = (observation["provider"], observation["target"])
                if pair in pairs or observation["status"] != "executed" or type(observation["outcome"]) is not dict:
                    raise ValueError("portable observation must execute each provider and target once")
                if not exact_json(observation["outcome"], oracles[case_id]):
                    raise ValueError("portable observation differs from the frozen typed oracle")
                pairs.add(pair)
            if pairs != {(provider, target) for provider in ("bootstrap", "native")
                         for target in ("javascript", "webassembly")}:
                raise ValueError("portable observation provider/target census changed")
            if not case_id.startswith("m2-run:"):
                native = case["details"].get("native")
                if type(native) is not dict or native.get("status") not in {"executed", "platform-unavailable"}:
                    raise ValueError("native execution must have an explicit actual disposition")
                required_native = "executed" if native_supported() else "platform-unavailable"
                if native["status"] != required_native:
                    raise ValueError("native runtime disposition contradicts the supported host")
                if native["status"] == "executed" and type(native.get("outcome")) is not dict:
                    raise ValueError("native execution must retain its typed actual outcome")
                if native["status"] == "executed":
                    oracle = oracles[case_id]
                    expected = {"kind": "trapped", "code": oracle["value"]} if oracle["type"] == "trap" \
                        else {"kind": "returned", "value": oracle}
                    if not exact_json(native["outcome"], expected):
                        raise ValueError("native observation differs from the frozen typed oracle")
        if case["artifacts"] == "matched":
            hashes = case["details"].get("artifact_hashes")
            if type(hashes) is not dict or set(hashes) != {"javascript", "webassembly", "native-object"}:
                raise ValueError("matched artifacts require all three actual byte identities")
            for identity in hashes.values():
                if type(identity) is not dict or set(identity) != {"sha256", "bytes"}:
                    raise ValueError("artifact identity has the wrong exact schema")
                digest = identity["sha256"]
                if type(identity["bytes"]) is not int or identity["bytes"] <= 0:
                    raise ValueError("artifact byte count must be an exact positive integer")
                if type(digest) is not str or len(digest) != 64 or set(digest) - set("0123456789abcdef"):
                    raise ValueError("artifact digest must be lowercase SHA-256")
    if inputs != expected_inputs:
        raise ValueError("receipt input census changed")
