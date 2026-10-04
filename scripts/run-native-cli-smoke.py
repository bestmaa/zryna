#!/usr/bin/env python3
"""Source-checkout private CLI proof; never admits or activates an installed provider."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys

CASES = (
    ("m1", "examples/universal/add.zry", None, 1),
    ("m2", "tests/m2-fixtures/valid/main.zry", "control-flow-v1", 2),
    ("m3-pair", "tests/m3-fixtures/conformance/pair.zry", "data-ownership-v1", 3),
    ("m3-array", "tests/m3-fixtures/conformance/array.zry", "data-ownership-v1", 3),
    ("m3-borrow", "tests/m3-fixtures/conformance/borrow.zry", "data-ownership-v1", 3),
    ("m3-vec", "tests/m3-fixtures/conformance/vec.zry", "data-ownership-v1", 3),
    ("m3-string", "tests/m3-fixtures/conformance/string-body.zry", "data-ownership-v1", 3),
    ("m3-owned-aggregate", "tests/m3-fixtures/conformance/owned-aggregate-body.zry", "data-ownership-v1", 3),
    ("m3-owned-vec", "tests/m3-fixtures/conformance/owned-vec-body.zry", "data-ownership-v1", 3),
)
NEGATIVE = (
    ("m1-negative", "tests/m1-fixtures/invalid-any.zry", None, "ZRYNA-M1004"),
    ("m1-bool", "tests/m1-fixtures/bool-gated.zry", None, "ZRYNA-I1006"),
    ("m3-moved", "tests/m3-fixtures/conformance/moved.zry", "data-ownership-v1", "ZRYNA-M3011"),
)


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def git(root, *args):
    return subprocess.check_output(["git", "-C", str(root), *args], text=True).strip()


def inventory(root):
    return {name: digest(root / name) for name in git(root, "ls-files").splitlines()}


def tree(path):
    result = {}
    for file in sorted(path.rglob("*")):
        if file.is_symlink():
            raise ValueError("bundle contains a symlink")
        if file.is_file():
            result[file.relative_to(path).as_posix()] = {"bytes": file.stat().st_size, "sha256": digest(file)}
    if not result:
        raise ValueError("empty bundle")
    return result


def main():
    if not __debug__:
        raise RuntimeError("smoke verification requires Python assertions; optimization is forbidden")
    parser = argparse.ArgumentParser(description=__doc__)
    for flag in ("root", "feature-cli", "default-cli", "node", "cargo", "rustc", "output"):
        parser.add_argument("--" + flag, type=Path, required=True)
    args = parser.parse_args()
    root = args.root.resolve(strict=True)
    output = args.output.resolve()
    if args.output.is_symlink() or output.exists() or output.is_relative_to(root):
        parser.error("output must be a new external evidence directory")
    if git(root, "status", "--porcelain"):
        parser.error("proof requires a clean committed source checkout")
    # Preserve executable names: rustup's cargo/rustc shims dispatch using argv[0].
    binaries = {name: getattr(args, name).absolute() for name in
                ("feature_cli", "default_cli", "node", "cargo", "rustc")}
    for path in binaries.values():
        if not path.is_file():
            parser.error("tool path must name an existing executable file")
    identity = {name: {"path": str(path), "sha256": digest(path)} for name, path in binaries.items()}
    head = git(root, "rev-parse", "HEAD")
    before = inventory(root)
    output.mkdir()
    empty = output / "empty-path"
    empty.mkdir()
    env = dict(os.environ, PATH=str(empty), CARGO=str(binaries["cargo"]), RUSTC=str(binaries["rustc"]),
               CARGO_NET_OFFLINE="true")
    for key in tuple(env):
        if key.startswith(("NODE", "NPM", "PNPM", "ZRYNA_NODE")):
            env.pop(key)
    assert all(shutil.which(name, path=env["PATH"]) is None for name in ("node", "pnpm", "npm"))
    records = []
    generated_inputs = {}
    fixture_base = root / ".zryna/cache" / ("native-cli-smoke-" + hashlib.sha256(str(output).encode()).hexdigest()[:16])

    def fixture_entry(label, source):
        # These frozen dependency bodies are admitted only through their registered import context.
        contexts = {"m3-string": "string", "m3-owned-aggregate": "owned-aggregate", "m3-owned-vec": "owned-vec"}
        if label not in contexts:
            return source
        registry = json.loads((root / "tests/m3-conformance-v1.json").read_text())
        fixtures = {fixture["id"]: fixture for fixture in registry["fixtures"]}
        entry = fixtures[contexts[label]]
        body = fixtures[entry["dependency"]]
        assert body["path"] == source
        # Refuse links and pre-existing case paths before creating task-owned source fixtures.
        for directory in (root / ".zryna", root / ".zryna/cache"):
            if directory.exists() or directory.is_symlink():
                assert directory.is_dir() and not directory.is_symlink(), "fixture parent is not a real directory"
            else:
                directory.mkdir()
        directory = fixture_base / label
        if not fixture_base.exists():
            fixture_base.mkdir()
        assert not fixture_base.is_symlink(), "fixture root is linked"
        directory.mkdir(exist_ok=False)
        for name, fixture in (("main.zry", entry), ("math.zry", body)):
            original = root / fixture["path"]
            assert digest(original) == fixture["sha256"] == before[fixture["path"]]
            destination = directory / name
            destination.write_bytes(original.read_bytes())
            assert digest(destination) == fixture["sha256"]
            generated_inputs[destination.relative_to(root).as_posix()] = {
                "original": fixture["path"], "sha256": fixture["sha256"]}
        return (directory / "main.zry").relative_to(root).as_posix()

    def execute(label, command, process_env=env):
        process = subprocess.run(command, cwd=root, env=process_env, capture_output=True, timeout=120)
        (output / (label + ".stdout")).write_bytes(process.stdout)
        (output / (label + ".stderr")).write_bytes(process.stderr)
        return process

    def case(label, operation):
        try:
            detail = operation()
            records.append({"id": label, "status": "passed", "detail": detail})
        except (AssertionError, ValueError, OSError, subprocess.SubprocessError) as error:
            records.append({"id": label, "status": "failed", "reason": str(error) or type(error).__name__})

    def command(binary, source, stem, profile=None):
        result = [str(binary), "build", source, "--root", str(root), "--name", stem,
                  "--target", "all", "--json"]
        if profile:
            result += ["--profile", profile]
        return result

    def positive(label, source, profile, version):
        source = fixture_entry(label, source)
        stem = "private-smoke-" + label
        destination = root / ".zryna/out" / (stem + ".build")
        assert not destination.exists(), "refuse a pre-existing output"
        baseline = execute(label + "-bootstrap", command(binaries["default_cli"], source, stem, profile)
                           + ["--node", str(binaries["node"])])
        assert baseline.returncode == 0, "bootstrap build failed: " + baseline.stderr.decode(errors="replace")
        baseline_json = json.loads(baseline.stdout)
        assert baseline_json["ok"] is True and baseline_json["results"] == []
        frozen = tree(destination)
        retained = output / (label + "-bootstrap-bundle")
        shutil.move(destination, retained)
        assert tree(retained) == frozen, "retained bootstrap recovery differs"
        native = execute(label + "-native", command(binaries["feature_cli"], source, stem, profile)
                         + ["--native-frontend"])
        assert native.returncode == 0, "private native build failed: " + native.stderr.decode(errors="replace")
        assert native.stdout == baseline.stdout, "complete CLI success JSON bytes differ"
        actual = tree(destination)
        assert actual == frozen, "artifact or manifest bytes differ"
        manifest = destination / f"zryna-manifest-v{version}.json"
        assert manifest.name in actual
        # Verify create-only behavior and full recovery before moving generated proof outputs.
        repeated = execute(label + "-create-only", command(binaries["feature_cli"], source, stem, profile)
                           + ["--native-frontend"])
        assert repeated.returncode != 0 and json.loads(repeated.stdout)["ok"] is False
        assert tree(destination) == actual, "rejected overwrite changed admitted output"
        saved = output / (label + "-native-bundle")
        shutil.move(destination, saved)
        assert tree(saved) == actual
        return {"source": source, "profile": profile or "i32-v1", "files": actual,
                "success_json_exact": True, "manifest_bytes_exact": True, "create_only": True,
                "node_on_path": False, "pnpm_on_path": False}

    for values in CASES:
        case(values[0], lambda values=values: positive(*values))

    def negative(label, source, profile, code):
        stem = "private-smoke-" + label
        destination = root / ".zryna/out" / (stem + ".build")
        assert not destination.exists()
        baseline = execute(label + "-bootstrap", command(binaries["default_cli"], source, stem, profile)
                           + ["--node", str(binaries["node"])])
        assert baseline.returncode != 0 and not destination.exists()
        response = execute(label, command(binaries["feature_cli"], source, stem, profile) + ["--native-frontend"])
        assert response.returncode == baseline.returncode and response.stdout == baseline.stdout, "complete negative CLI diagnostics differ"
        assert response.returncode != 0
        data = json.loads(response.stdout)
        assert data["ok"] is False and code in [diagnostic["code"] for diagnostic in data["diagnostics"]], data
        assert not destination.exists(), "negative source published a final bundle"
        return {"expected_code": code, "exit": response.returncode, "final_bundle_absent": True}

    for values in NEGATIVE:
        case(values[0], lambda values=values: negative(*values))

    def reject(label, binary, extra, expected=None):
        arguments = command(binary, CASES[0][1], "private-smoke-denied")
        if "--target" in extra:
            index = arguments.index("--target")
            del arguments[index:index + 2]
        process = execute(label, arguments + extra)
        assert process.returncode != 0, "unauthorized route succeeded"
        if expected:
            data = json.loads(process.stdout)
            assert expected in [diagnostic["code"] for diagnostic in data["diagnostics"]], data
        assert not (root / ".zryna/out/private-smoke-denied.build").exists()
        return {"exit": process.returncode, "expected_code": expected}

    case("default-feature-disabled", lambda: reject("default-feature-disabled", binaries["default_cli"],
                                                   ["--native-frontend"]))
    case("ordinary-feature-build-needs-node", lambda: reject("ordinary-feature-build-needs-node",
                                                           binaries["feature_cli"], []))
    case("private-project-denied", lambda: reject("private-project-denied", binaries["feature_cli"],
                                                 ["--native-frontend", "--project-root", str(root)], "ZRYNA-C2001"))
    case("private-component-denied", lambda: reject("private-component-denied", binaries["feature_cli"],
                                                   ["--native-frontend", "--target", "component"], "ZRYNA-C1013"))
    def no_cargo():
        without_cargo = {key: value for key, value in env.items() if key not in ("CARGO", "RUSTC")}
        process = execute("source-checkout-still-needs-cargo",
                          command(binaries["feature_cli"], CASES[0][1], "private-smoke-denied")
                          + ["--native-frontend"], without_cargo)
        assert process.returncode != 0
        assert "ZRYNA-A1101" in process.stdout.decode(), "architecture check was bypassed"
        return {"exit": process.returncode, "architecture_gate_retained": True}
    case("source-checkout-still-needs-cargo", no_cargo)
    stable = head == git(root, "rev-parse", "HEAD") and before == inventory(root)
    stable &= all(digest(root / name) == value["sha256"] for name, value in generated_inputs.items())
    stable &= not git(root, "status", "--porcelain")
    stable &= all(digest(binaries[name]) == value["sha256"] for name, value in identity.items())
    records.append({"id": "source-and-binary-identity", "status": "passed" if stable else "failed"})
    blocked = ["ordinary installed CLI without Node/pnpm/Cargo", "public activation", "native run selection",
               "cross-platform installed distribution proof"]
    receipt = {"version": 1, "head": head, "tree": git(root, "rev-parse", "HEAD^{tree}"),
               "inputs": before, "generated_inputs": generated_inputs, "binaries": identity, "path": str(empty), "records": records,
               "counts": {status: sum(record["status"] == status for record in records)
                          for status in ("passed", "failed", "ignored")},
               "blocked_acceptance": blocked, "public_activation": False}
    (output / "receipt.json").write_text(json.dumps(receipt, indent=2) + "\n")
    print(json.dumps({key: receipt[key] for key in ("head", "counts", "blocked_acceptance", "public_activation")}))
    return int(receipt["counts"]["failed"] != 0)


if __name__ == "__main__":
    sys.exit(main())
