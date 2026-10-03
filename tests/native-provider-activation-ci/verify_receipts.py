"""Independent CI admission for the pinned private activation harness receipts."""

import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess
import tomllib

CONSUMER_SHA = "f09f5abb69fe7e05352221c3a0302138ccde4984"
DEPENDENCY_SHA = "af415a682330b1015818e9c0a8d61555fdd8d18c"
PROVIDER = "zryna-native-activation-harness"
VERSION = "0.2.3"


def require(condition, message):
    if not condition:
        raise ValueError(message)


def unique_object(pairs):
    result = {}
    for key, value in pairs:
        require(key not in result, f"duplicate JSON field: {key}")
        result[key] = value
    return result


def read_json(path):
    return json.loads(path.read_text(encoding="utf-8"), object_pairs_hook=unique_object)


def expected_cases(retained):
    names = []
    faults = ("accepted", "identity", "version", "protocol", "module", "semantic", "extra",
              "id", "snapshot-version", "snapshot-path", "frame")
    for protocol in (2, 3, 4):
        names.extend(f"v{protocol}:{fault}" for fault in faults)
        if protocol >= 3:
            names.append(f"v{protocol}:control")
        if protocol == 4:
            names.append("v4:ownership")
    if retained:
        names.extend(("retained-v2:dispatch", "retained-v3:dispatch", "retained-v4:dispatch",
                      "retained:stale-source-denied"))
    return names


def verify_receipt(receipt, retained, platform):
    require(type(receipt) is dict, "receipt must be an object")
    exact = {"repository_sha": CONSUMER_SHA, "dependency_sha": DEPENDENCY_SHA,
             "platform": platform, "retained": retained, "status": "passed",
             "public_activation": False, "temporary_directories_removed": True,
             "runtime_path_tools": []}
    for key, value in exact.items():
        observed = receipt.get(key)
        require(type(observed) is type(value) and observed == value, f"invalid {key}")
    expected_keys = set(exact) | {"cargo_version", "rustc_version", "repository_lock_sha256",
                                 "harness_lock_sha256", "executable_sha256", "installation_entries",
                                 "build_directory", "installation_directory", "smoke"}
    require(set(receipt) == expected_keys, "outer receipt fields differ")
    for key in ("build_directory", "installation_directory"):
        require(type(receipt[key]) is str and bool(receipt[key]), f"invalid {key}")
    for key, prefix in (("cargo_version", "cargo 1.97.1 "), ("rustc_version", "rustc 1.97.1 ")):
        require(type(receipt.get(key)) is str and receipt[key].startswith(prefix), f"invalid {key}")
    for key in ("repository_lock_sha256", "harness_lock_sha256", "executable_sha256"):
        require(type(receipt.get(key)) is str and re.fullmatch(r"[0-9a-f]{64}", receipt[key]),
                f"invalid {key}")
    suffix = ".exe" if platform == "nt" else ""
    require(receipt.get("installation_entries") ==
            ["empty-path", "fixtures", "unrelated-cwd", PROVIDER + suffix], "installation widened")
    smoke = receipt.get("smoke")
    require(type(smoke) is dict, "missing smoke object")
    expected = {"schema_version": 1, "provider": PROVIDER, "provider_version": VERSION,
                "passed": expected_cases(retained), "failed": [], "ignored": [],
                "retained": retained, "public_activation": False}
    require(set(smoke) == set(expected), "smoke fields differ")
    for key, value in expected.items():
        require(type(smoke[key]) is type(value) and smoke[key] == value, f"invalid smoke {key}")
    require(len(smoke["passed"]) == (40 if retained else 36), "incomplete smoke")
    require(all(type(name) is str for name in smoke["passed"]), "non-string case")
    require("error" not in receipt, "failure information in passing receipt")


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def git(consumer, *args):
    return subprocess.check_output(["git", "-C", str(consumer), *args], text=True).strip()


def verify_evidence(consumer, evidence, target, retained):
    consumer, evidence, target = (path.resolve() for path in (consumer, evidence, target))
    for external in (evidence, target):
        require(external != consumer and consumer not in external.parents,
                "evidence and target must be outside consumer")
    require(git(consumer, "rev-parse", "HEAD") == CONSUMER_SHA, "consumer head differs")
    require(not git(consumer, "status", "--porcelain"), "consumer is dirty")
    version = tomllib.loads((consumer / "Cargo.toml").read_text())["workspace"]["package"]["version"]
    require(version == VERSION, "consumer version differs")
    receipt = read_json(evidence / "receipt.json")
    verify_receipt(receipt, retained, os.name)
    smoke = read_json(evidence / "smoke.log")
    verify_receipt({**receipt, "smoke": smoke}, retained, os.name)
    require(smoke == receipt["smoke"], "smoke log differs")
    for key in ("build_directory", "installation_directory"):
        require(not Path(receipt[key]).exists(), "temporary directory remains")
    require(digest(consumer / "Cargo.lock") == receipt["repository_lock_sha256"],
            "repository lock differs")
    require(digest(evidence / "harness.Cargo.lock") == receipt["harness_lock_sha256"],
            "harness lock differs")
    suffix = ".exe" if os.name == "nt" else ""
    require(digest(target / "debug" / (PROVIDER + suffix)) == receipt["executable_sha256"],
            "current lane executable differs")
    require(git(consumer, "rev-parse", "HEAD") == CONSUMER_SHA and
            not git(consumer, "status", "--porcelain"), "consumer changed during verification")
    return {"consumer_sha": CONSUMER_SHA, "platform": os.name, "retained": retained,
            "passed": len(receipt["smoke"]["passed"]), "failed": 0, "ignored": 0,
            "public_activation": False, "executable_sha256": receipt["executable_sha256"]}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--consumer", required=True, type=Path)
    parser.add_argument("--evidence-dir", required=True, type=Path)
    parser.add_argument("--target-dir", required=True, type=Path)
    parser.add_argument("--retained", action="store_true")
    args = parser.parse_args()
    result = verify_evidence(args.consumer, args.evidence_dir, args.target_dir, args.retained)
    (args.evidence_dir / "ci-verification.json").write_text(json.dumps(result, indent=2) + "\n",
                                                          encoding="utf-8")
    print(json.dumps(result))


if __name__ == "__main__":
    main()
