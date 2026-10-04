#!/usr/bin/env python3
"""Compare real bootstrap/native providers across the frozen corpus without activating either."""

import argparse
import importlib.util
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tomllib

sys.dont_write_bytecode = True
ROOT = Path(__file__).resolve().parent.parent
SPEC = importlib.util.spec_from_file_location(
    "activation_support", ROOT / "scripts/run-native-provider-activation.py")
SUPPORT = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(SUPPORT)
PROVIDER = "zryna-native-provider-corpus"
DEPENDENCY_SHA = "0635c19f922f7af61fa1e05b1a632b1013e908e7"
PATH_DEPENDENCIES = (
    "zryna-source", "zryna-diagnostics", "zryna-frontend", "zryna-syntax", "zryna-driver",
    "zryna-semantics", "zryna-ir", "zryna-layout", "zryna-native-mir", "zryna-abi",
    "zryna-backend-javascript", "zryna-backend-webassembly", "zryna-backend-native",
)




RECEIPT_SPEC = importlib.util.spec_from_file_location(
    "corpus_receipts", ROOT / "tests/native-provider-corpus/receipt.py")
RECEIPTS = importlib.util.module_from_spec(RECEIPT_SPEC)
RECEIPT_SPEC.loader.exec_module(RECEIPTS)
strict_json = RECEIPTS.strict_json
expected_cases = RECEIPTS.expected_cases
runtime_oracles = RECEIPTS.runtime_oracles
exact_json = RECEIPTS.exact_json


def verify_receipt(receipt, inputs, *, allow_failed=False):
    return RECEIPTS.verify_receipt(receipt, inputs, root=ROOT, expected_inputs=inventory(ROOT),
                                   allow_failed=allow_failed)


def inventory(root):
    """Pin both registries and every source, including syntax fixtures outside the registry."""
    paths = set()
    for profile in ("m1", "m2", "m3"):
        registry = root / f"tests/{profile}-conformance-v1.json"
        paths.add(registry.relative_to(root).as_posix())
        data = json.loads(registry.read_text(encoding="utf-8"))
        if profile == "m1":
            paths.update((data["entrypoint"], data["invalidSource"]["path"],
                          data["gatedBooleanSource"]["path"]))
        else:
            fixtures = data["fixtureFiles" if profile == "m2" else "fixtures"]
            for fixture in fixtures:
                path = fixture["path"]
                if SUPPORT.digest(root / path) != fixture["sha256"]:
                    raise ValueError(f"frozen {profile} fixture digest changed: {path}")
                paths.add(path)
    for profile, expected in (("m2", 14), ("m3", 95)):
        sources = sorted((root / f"tests/{profile}-fixtures").rglob("*.zry"))
        if len(sources) != expected:
            raise ValueError(f"frozen {profile} source census changed: {len(sources)} != {expected}")
        paths.update(path.relative_to(root).as_posix() for path in sources)
    return {path: SUPPORT.digest(root / path) for path in sorted(paths)}


def manifest(package, version, lock):
    lines = ["[package]", f'name = "{PROVIDER}"', f'version = "{version}"',
             'edition = "2024"', 'rust-version = "1.97"', "[workspace]"]
    for name in PATH_DEPENDENCIES:
        lines.extend([f"[dependencies.{name}]",
                      f"path = {json.dumps(str(ROOT / 'crates' / name))}"])
    for name in ("serde", "serde_json", "sha2"):
        pins = [item["version"] for item in lock["package"] if item["name"] == name]
        if len(pins) != 1:
            raise ValueError(f"ambiguous repository pin: {name}")
        lines.extend([f"[dependencies.{name}]", f'version = "={pins[0]}"'])
        if name == "serde":
            lines.append('features = ["derive"]')
    lines.extend(["[profile.dev]", "debug = 0", "[lints.rust]", 'unsafe_code = "forbid"'])
    (package / "Cargo.toml").write_text("\n".join(lines) + "\n", encoding="utf-8")



def run_corpus(command, cwd, evidence, env):
    """Keep machine-readable stdout separate from assertion/progress evidence."""
    options = {"start_new_session": True} if os.name == "posix" else {
        "creationflags": subprocess.CREATE_NEW_PROCESS_GROUP,
    }
    with (evidence / "corpus.json").open("w", encoding="utf-8") as output, \
            (evidence / "corpus.stderr.log").open("w", encoding="utf-8") as errors:
        process = subprocess.Popen(command, cwd=cwd, env=env, stdout=output,
                                   stderr=errors, **options)
        try:
            return process.wait(timeout=1800)
        except BaseException:
            SUPPORT.stop_tree(process, errors)
            raise


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--evidence-dir", required=True, type=Path)
    parser.add_argument("--cargo", default="cargo")
    parser.add_argument("--node", required=True, type=Path)
    parser.add_argument("--target-dir", required=True, type=Path)
    parser.add_argument("--prepare-only", action="store_true",
                        help="prepare a retained external package for implementation checks")
    args = parser.parse_args()
    evidence = args.evidence_dir.resolve()
    target = args.target_dir.resolve()
    for path in (evidence, target):
        if path == ROOT or ROOT in path.parents:
            raise ValueError("generated evidence/cache must stay outside the controlled repository")
    evidence.mkdir(parents=True, exist_ok=False)
    state = {"schema_version": 1, "repository_sha": SUPPORT.git("rev-parse", "HEAD"),
             "dependency_sha": DEPENDENCY_SHA, "platform": sys.platform,
             "public_activation": False, "status": "failed", "passed": [],
             "failed": [], "ignored": [], "blocked": []}
    try:
        if not args.prepare_only and SUPPORT.git("status", "--porcelain"):
            raise ValueError("exact-revision corpus proof requires a clean worktree")
        subprocess.run(["git", "merge-base", "--is-ancestor", DEPENDENCY_SHA, "HEAD"],
                       cwd=ROOT, check=True)
        state["inputs"] = inventory(ROOT)
        state["repository_lock_sha256"] = SUPPORT.digest(ROOT / "Cargo.lock")
        node = args.node.resolve(strict=True)
        node_version = subprocess.check_output([str(node), "--version"], text=True).strip()
        if node_version != "v22.22.1":
            raise ValueError(f"expected pinned Node v22.22.1, observed {node_version}")
        state["node"] = {"version": node_version, "sha256": SUPPORT.digest(node)}
        cargo = shutil.which(args.cargo)
        if cargo is None:
            raise ValueError("pinned Cargo unavailable")
        env = dict(os.environ)
        for name in ("RUSTC", "RUSTC_WRAPPER", "RUSTC_WORKSPACE_WRAPPER"):
            env.pop(name, None)
        env["RUSTUP_TOOLCHAIN"] = "1.97.1"
        env["CARGO_TARGET_DIR"] = str(target)
        rustc = Path(cargo).parent / ("rustc.exe" if os.name == "nt" else "rustc")
        env["RUSTC"] = str(rustc)
        for tool, expected in ((cargo, "cargo 1.97.1 "), (str(rustc), "rustc 1.97.1 ")):
            version = subprocess.check_output([tool, "--version"], env=env, text=True).strip()
            if not version.startswith(expected):
                raise ValueError(f"pinned tool mismatch: {version}")
            state[Path(tool).stem + "_version"] = version
        package = evidence / "package"
        src = package / "src"
        src.mkdir(parents=True)
        for source in (ROOT / "tests/native-provider-corpus").glob("*.rs"):
            shutil.copy2(source, src / source.name)
        for name in ("rustfmt.toml", "rust-toolchain.toml", "Cargo.lock"):
            shutil.copy2(ROOT / name, package / name)
        original = tomllib.loads((ROOT / "Cargo.lock").read_text())
        version = tomllib.loads((ROOT / "Cargo.toml").read_text())["workspace"]["package"]["version"]
        manifest(package, version, original)
        SUPPORT.run([cargo, "generate-lockfile", "--offline"], package,
                    evidence / "lock.log", env=env)
        SUPPORT.verify_registry_lock(original, tomllib.loads((package / "Cargo.lock").read_text()))
        state["harness_lock_sha256"] = SUPPORT.digest(package / "Cargo.lock")
        if args.prepare_only:
            state["status"] = "prepared-not-tested"
            state["blocked"].append("implementation checks only; no exact-revision acceptance claim")
            return
        SUPPORT.run([cargo, "build", "--locked", "--offline", "-j", "2"], package,
                    evidence / "build.log", env=env)
        state["passed"].append("locked-offline-build")
        SUPPORT.run([cargo, "fmt", "--", "--check"], package,
                    evidence / "format.log", env=env)
        SUPPORT.run([cargo, "clippy", "--locked", "--offline", "-j", "2", "--", "-D", "warnings"],
                    package, evidence / "clippy.log", env=env)
        state["passed"].extend(("format", "strict-clippy"))
        runtime = evidence / "runtime"
        binary = target / "debug" / (PROVIDER + (".exe" if os.name == "nt" else ""))
        state["executable_sha256"] = SUPPORT.digest(binary)
        corpus_exit = run_corpus([str(binary), str(ROOT), str(node), str(runtime)],
                                 evidence, evidence, env)
        state["corpus_exit"] = corpus_exit
        receipt = strict_json((evidence / "corpus.json").read_text(encoding="utf-8"))
        state["corpus"] = receipt
        verify_receipt(receipt, state["inputs"], allow_failed=True)
        state["corpus_receipt_validated"] = True
        state["blocked"] = receipt["blocked"]
        state["corpus_passed"] = receipt["passed"]
        state["corpus_failed"] = receipt["failed"]
        state["after_sha"] = SUPPORT.git("rev-parse", "HEAD")
        state["after_clean"] = not bool(SUPPORT.git("status", "--porcelain"))
        state["after_inputs"] = inventory(ROOT)
        state["after_repository_lock_sha256"] = SUPPORT.digest(ROOT / "Cargo.lock")
        state["after_executable_sha256"] = SUPPORT.digest(binary)
        state["after_node_sha256"] = SUPPORT.digest(node)
        if state["after_sha"] != state["repository_sha"] or not state["after_clean"] \
                or state["after_inputs"] != state["inputs"] \
                or state["after_repository_lock_sha256"] != state["repository_lock_sha256"] \
                or state["after_executable_sha256"] != state["executable_sha256"] \
                or state["after_node_sha256"] != state["node"]["sha256"]:
            raise ValueError("exact-revision inputs or executable changed during the corpus proof")
        state["exact_revision_after_corpus"] = True
        if corpus_exit:
            raise ValueError(f"corpus comparisons exited {corpus_exit}; see corpus.json and corpus.stderr.log")
        verify_receipt(receipt, state["inputs"])
        state["passed"].append("exhaustive-provider-corpus")
        state["blocked"] = receipt["blocked"]
        if inventory(ROOT) != state["inputs"] or SUPPORT.git("status", "--porcelain"):
            raise ValueError("corpus/worktree changed during exact-revision proof")
        if SUPPORT.git("rev-parse", "HEAD") != state["repository_sha"]:
            raise ValueError("revision changed during corpus proof")
        state["status"] = "passed"
    except Exception as error:
        state["failed"].append(str(error))
        raise
    finally:
        (evidence / "receipt.json").write_text(json.dumps(state, indent=2) + "\n", encoding="utf-8")


if __name__ == "__main__":
    main()
