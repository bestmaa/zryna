#!/usr/bin/env python3
"""Build a private native provider harness and smoke its relocated no-tool installation."""

import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import tomllib

ROOT = Path(__file__).resolve().parent.parent
DEPENDENCY_SHA = "69af4dd853ac99b5ba99f45712e5406d04077a45"
PROVIDER = "zryna-native-activation-harness"


def expected_cases(retained):
    cases = []
    for protocol in range(2, 5):
        faults = ["accepted", "identity", "version", "protocol", "module", "semantic",
                  "extra", "id", "snapshot-version", "snapshot-path", "frame"]
        if protocol >= 3:
            faults.append("control")
        if protocol == 4:
            faults.append("ownership")
        cases.extend(f"v{protocol}:{fault}" for fault in faults)
    if retained:
        cases.extend(f"retained-v{protocol}:dispatch" for protocol in range(2, 5))
        cases.append("retained:stale-source-denied")
    return cases


def verify_receipt(receipt, retained, version):
    expected = {
        "schema_version": 1, "provider": PROVIDER, "provider_version": version,
        "passed": expected_cases(retained), "failed": [], "ignored": [],
        "retained": retained, "public_activation": False,
    }
    if receipt != expected:
        raise ValueError("smoke receipt must execute each exact case once with zero failure/ignore")


def runtime_environment(empty_path):
    env = {"PATH": str(empty_path)}
    # Windows needs these to start native executables and retain the existing Job Object runner.
    for name in ("SYSTEMROOT", "WINDIR", "SystemRoot"):
        if name in os.environ:
            env[name] = os.environ[name]
    for name in ("node", "pnpm", "cargo", "rustc"):
        if shutil.which(name, path=env["PATH"]):
            raise ValueError(f"runtime PATH unexpectedly contains {name}")
    return env


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def run(command, cwd, log, env=None, timeout=1800):
    with log.open("w", encoding="utf-8") as output:
        result = subprocess.run(command, cwd=cwd, env=env, stdout=output,
                                stderr=subprocess.STDOUT, timeout=timeout, check=False)
    if result.returncode:
        raise RuntimeError(f"command exited {result.returncode}: {command}; evidence: {log}")


def git(*args):
    return subprocess.check_output(["git", *args], cwd=ROOT, text=True).strip()


def manifest(package, retained, version, lock):
    dependencies = {"zryna-frontend": {}, "zryna-source": {}}
    if retained:
        dependencies.update({name: {"optional": True} for name in (
            "zryna-driver", "zryna-semantics", "zryna-backend-javascript",
            "zryna-backend-webassembly")})
    lines = ["[package]", f'name = "{PROVIDER}"', f'version = "{version}"',
             'edition = "2024"', 'rust-version = "1.97"', "[workspace]",
             "[features]", "default = []"]
    features = [f"dep:{name}" for name, options in dependencies.items() if options]
    lines.append(f"retained = {json.dumps(features)}")
    for name, options in dependencies.items():
        lines.extend([f"[dependencies.{name}]",
                      f"path = {json.dumps(str(ROOT / 'crates' / name))}"])
        if options:
            lines.append("optional = true")
    for name in ("serde", "serde_json"):
        pinned = [item["version"] for item in lock["package"] if item["name"] == name]
        if len(pinned) != 1:
            raise ValueError(f"ambiguous registry pin for {name}")
        lines.extend([f"[dependencies.{name}]", f'version = "={pinned[0]}"'])
        if name == "serde":
            lines.append('features = ["derive"]')
    lines.extend(["[profile.dev]", "debug = 0", "[lints.rust]", 'unsafe_code = "forbid"'])
    (package / "Cargo.toml").write_text("\n".join(lines) + "\n", encoding="utf-8")


def verify_registry_lock(original, generated):
    def registry(lock):
        return {(p["name"], p["version"], p["source"], p.get("checksum"))
                for p in lock["package"] if "source" in p}
    if not registry(generated) <= registry(original):
        raise ValueError("harness lock selected registry material absent from the repository lock")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--evidence-dir", required=True, type=Path)
    parser.add_argument("--cargo", default="cargo")
    parser.add_argument("--target-dir", type=Path, help="optional private external Cargo build cache")
    parser.add_argument("--retained", action="store_true",
                        help="consume #413's pinned exported API in addition to the handshake")
    args = parser.parse_args()
    evidence = args.evidence_dir.resolve()
    if evidence == ROOT or ROOT in evidence.parents:
        raise ValueError("evidence must stay outside the controlled repository")
    evidence.mkdir(parents=True, exist_ok=False)
    state = {"repository_sha": git("rev-parse", "HEAD"), "dependency_sha": DEPENDENCY_SHA,
             "platform": os.name, "retained": args.retained, "status": "failed",
             "public_activation": False}
    try:
        if git("status", "--porcelain"):
            raise ValueError("exact-revision smoke requires a clean worktree")
        if args.retained:
            subprocess.run(["git", "merge-base", "--is-ancestor", DEPENDENCY_SHA, "HEAD"],
                           cwd=ROOT, check=True)
        version = tomllib.loads((ROOT / "Cargo.toml").read_text())["workspace"]["package"]["version"]
        cargo = shutil.which(args.cargo)
        if cargo is None:
            raise ValueError("pinned Cargo is unavailable")
        cargo_version = subprocess.check_output([cargo, "--version"], text=True).strip()
        if not cargo_version.startswith("cargo 1.97.1 "):
            raise ValueError(f"expected pinned Cargo 1.97.1, observed {cargo_version}")
        state["cargo_version"] = cargo_version
        state["repository_lock_sha256"] = digest(ROOT / "Cargo.lock")
        original = tomllib.loads((ROOT / "Cargo.lock").read_text())
        with tempfile.TemporaryDirectory(prefix="zryna-414-build-") as build_dir:
            package = Path(build_dir)
            src = package / "src"
            src.mkdir()
            shutil.copy2(ROOT / "rustfmt.toml", package / "rustfmt.toml")
            for source in (ROOT / "tests/native-provider-activation").glob("*.rs"):
                shutil.copy2(source, src / source.name)
            manifest(package, args.retained, version, original)
            shutil.copy2(ROOT / "Cargo.lock", package / "Cargo.lock")
            run([cargo, "generate-lockfile", "--offline"], package, evidence / "lock.log")
            verify_registry_lock(original, tomllib.loads((package / "Cargo.lock").read_text()))
            shutil.copy2(package / "Cargo.lock", evidence / "harness.Cargo.lock")
            state["harness_lock_sha256"] = digest(package / "Cargo.lock")
            build_env = dict(os.environ)
            # The private package cannot inherit a workspace's feature or toolchain selection.
            target = args.target_dir.resolve() if args.target_dir else package / "target"
            if target == ROOT or ROOT in target.parents:
                raise ValueError("private harness target must stay outside the repository")
            build_env["CARGO_TARGET_DIR"] = str(target)
            command = [cargo, "build", "--locked", "--offline", "-j", "2"]
            if args.retained:
                command.extend(["--features", "retained"])
            run(command, package, evidence / "build.log", env=build_env)
            run([cargo, "fmt", "--", "--check"], package, evidence / "format.log", env=build_env)
            lint = [cargo, "clippy", "--locked", "--offline", "-j", "2"]
            if args.retained:
                lint.extend(["--features", "retained"])
            run([*lint, "--", "-D", "warnings"], package, evidence / "clippy.log", env=build_env)
            suffix = ".exe" if os.name == "nt" else ""
            binary = target / "debug" / (PROVIDER + suffix)
            with tempfile.TemporaryDirectory(prefix="zryna-414-install-") as installation:
                installed = Path(installation)
                executable = installed / (PROVIDER + suffix)
                shutil.copy2(binary, executable)
                assert digest(executable) == digest(binary)
                state["executable_sha256"] = digest(executable)
                empty_path = installed / "empty-path"
                empty_path.mkdir()
                fixtures = installed / "fixtures"
                fixtures.mkdir()
                cwd = installed / "unrelated-cwd"
                cwd.mkdir()
                runtime_env = runtime_environment(empty_path)
                # Only one executable and owned empty/fixture directories are installed.
                state["installation_entries"] = sorted(p.name for p in installed.iterdir())
                run([str(executable), "--smoke", str(fixtures)], cwd,
                    evidence / "smoke.log", env=runtime_env, timeout=300)
                receipt = json.loads((evidence / "smoke.log").read_text())
                verify_receipt(receipt, args.retained, version)
                state["smoke"] = receipt
                state["runtime_path_tools"] = []
        if git("status", "--porcelain") or git("rev-parse", "HEAD") != state["repository_sha"]:
            raise ValueError("repository changed during exact-revision smoke")
        state["status"] = "passed"
    except Exception as error:
        state["error"] = str(error)
        raise
    finally:
        (evidence / "receipt.json").write_text(json.dumps(state, indent=2) + "\n", encoding="utf-8")
    print(json.dumps(state, indent=2))


if __name__ == "__main__":
    main()
