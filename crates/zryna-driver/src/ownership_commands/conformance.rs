//! Fixed observations through the authenticated internal candidate route.

use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, fs, path::Path};
use zryna_abi::{ScalarOutcome, ScalarValue};

use super::execute_and_publish_run;
use crate::{
    CommandFailureKind, DataOwnershipBuildRequest, OwnershipTarget, TargetSelection,
    ownership_pipeline::{
        prepare_data_ownership_for_test,
        test_support::{fixture_workspace, node_executable, route_guard},
    },
    ownership_publication::publish_data_ownership_build,
};

mod authority;
mod faults;

const REGISTRY: &str = include_str!("../../../../tests/m3-conformance-v1.json");

fn registry() -> Value {
    serde_json::from_str(REGISTRY).expect("fixed M3 registry")
}

fn install(root: &Path, registry: &Value, id: &str) {
    let fixture = registry["fixtures"]
        .as_array()
        .expect("fixtures")
        .iter()
        .find(|fixture| fixture["id"] == id)
        .expect("registered fixture");
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(fixture["path"].as_str().expect("path"));
    let bytes = fs::read(path).expect("fixture source");
    assert_eq!(format!("{:x}", Sha256::digest(&bytes)), fixture["sha256"]);
    fs::write(root.join("main.zry"), bytes).expect("isolated source");
    if let Some(dependency) = fixture["dependency"].as_str() {
        let other = registry["fixtures"]
            .as_array()
            .expect("fixtures")
            .iter()
            .find(|fixture| fixture["id"] == dependency)
            .expect("registered dependency");
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .join(other["path"].as_str().expect("dependency path"));
        let bytes = fs::read(path).expect("dependency source");
        assert_eq!(format!("{:x}", Sha256::digest(&bytes)), other["sha256"]);
        fs::write(root.join("math.zry"), bytes).expect("isolated dependency");
    }
}

fn request(root: &Path, targets: TargetSelection) -> DataOwnershipBuildRequest {
    DataOwnershipBuildRequest {
        workspace_root: root.to_owned(),
        entrypoint: "main.zry".to_owned(),
        artifact_stem: "fixed-oracle".to_owned(),
        targets,
        node_runtime: node_executable(),
    }
}

fn arguments(case: &Value) -> Vec<ScalarValue> {
    case["arguments"]
        .as_array()
        .expect("fixed arguments")
        .iter()
        .map(|value| {
            ScalarValue::I32(i32::try_from(value.as_i64().expect("integer")).expect("i32"))
        })
        .collect()
}

fn assert_no_artifacts(root: &Path) {
    let output = root.join(".zryna/out");
    if output.exists() {
        assert_eq!(
            fs::read_dir(output).expect("output inventory").count(),
            0,
            "failure left a final or private partial artifact"
        );
    }
}

fn inventory(root: &Path) -> BTreeMap<String, Vec<u8>> {
    fn visit(base: &Path, path: &Path, files: &mut BTreeMap<String, Vec<u8>>) {
        for entry in fs::read_dir(path).expect("inventory") {
            let path = entry.expect("entry").path();
            let metadata = fs::symlink_metadata(&path).expect("metadata");
            assert!(!metadata.file_type().is_symlink());
            if metadata.is_dir() {
                visit(base, &path, files);
            } else {
                assert!(metadata.is_file());
                files.insert(
                    path.strip_prefix(base)
                        .expect("contained path")
                        .to_string_lossy()
                        .replace('\\', "/"),
                    fs::read(path).expect("bytes"),
                );
            }
        }
    }
    let mut files = BTreeMap::new();
    visit(root, root, &mut files);
    files
}

#[test]
fn fixed_candidate_observations_match_every_registered_case() {
    let _guard = route_guard();
    let registry = registry();
    let cases = registry["valid"].as_array().expect("valid cases");
    assert_eq!(cases.len(), 15, "frozen executable inventory");
    for case in cases {
        let workspace = fixture_workspace();
        install(workspace.root(), &registry, case["fixture"].as_str().expect("fixture"));
        let selections = if cfg!(all(target_os = "linux", target_arch = "x86_64")) {
            vec![TargetSelection::All]
        } else {
            vec![TargetSelection::JavaScript, TargetSelection::WebAssembly]
        };
        for targets in selections {
            let build = request(workspace.root(), targets);
            let prepared = prepare_data_ownership_for_test(
                &build,
                Some((case["export"].as_str().expect("export").to_owned(), arguments(case))),
            )
            .unwrap_or_else(|error| panic!("{} prepare: {error:?}", case["id"]));
            let published = execute_and_publish_run(&prepared)
                .unwrap_or_else(|error| panic!("{} execute: {error:?}", case["id"]));
            let expected_targets = match targets {
                TargetSelection::All => vec![
                    OwnershipTarget::JavaScript,
                    OwnershipTarget::WebAssembly,
                    OwnershipTarget::Native,
                ],
                TargetSelection::JavaScript => vec![OwnershipTarget::JavaScript],
                TargetSelection::WebAssembly => vec![OwnershipTarget::WebAssembly],
                TargetSelection::Native
                | TargetSelection::Component
                | TargetSelection::BrowserComponent => unreachable!(),
            };
            assert_eq!(
                published
                    .results()
                    .iter()
                    .map(crate::OwnershipManifestResult::target)
                    .collect::<Vec<_>>(),
                expected_targets
            );
            let expected = ScalarOutcome::Returned {
                value: ScalarValue::I32(
                    i32::try_from(case["expected"].as_i64().expect("fixed expected i32"))
                        .expect("i32"),
                ),
            };
            for result in published.results() {
                assert_eq!(result.outcome(), expected, "{} {:?}", case["id"], result.target());
            }
            let manifest = crate::decode_ownership_manifest_v3(
                &fs::read(published.manifest_path()).expect("manifest bytes"),
            )
            .expect("strict manifest");
            assert_eq!(manifest.results(), published.results());
            fs::remove_dir_all(published.path()).expect("remove test-owned bundle");
            assert_no_artifacts(workspace.root());
        }
    }
}

#[test]
fn fixed_invalid_cases_fail_in_the_owning_phase_without_artifacts() {
    let _guard = route_guard();
    let registry = registry();
    let cases = registry["invalid"].as_array().expect("invalid cases");
    assert_eq!(cases.len(), 4, "frozen rejection inventory");
    for case in cases {
        let workspace = fixture_workspace();
        install(workspace.root(), &registry, case["fixture"].as_str().expect("fixture"));
        let run = case["export"].as_str().map(|name| (name.to_owned(), arguments(case)));
        let failure =
            prepare_data_ownership_for_test(&request(workspace.root(), TargetSelection::All), run)
                .expect_err("registered invalid input must fail");
        assert_eq!(case["phase"], "source");
        assert_eq!(failure.kind(), CommandFailureKind::Source, "{}", case["id"]);
        assert_eq!(failure.diagnostics().len(), 1, "{}: {failure:?}", case["id"]);
        assert_eq!(failure.diagnostics()[0].code(), case["code"], "{}", case["id"]);
        assert_no_artifacts(workspace.root());
    }
}

#[test]
fn fixed_corpus_repeat_builds_have_identical_complete_artifacts() {
    let _guard = route_guard();
    let registry = registry();
    let mut visited = std::collections::BTreeSet::new();
    for case in registry["valid"].as_array().expect("valid cases") {
        let fixture = case["fixture"].as_str().expect("fixture");
        if !visited.insert(fixture) {
            continue;
        }
        let workspace = fixture_workspace();
        install(workspace.root(), &registry, fixture);
        let build = request(workspace.root(), TargetSelection::All);
        let mut first = None;
        for _ in 0..2 {
            let prepared = prepare_data_ownership_for_test(&build, None)
                .unwrap_or_else(|error| panic!("{fixture} prepare: {error:?}"));
            let published = publish_data_ownership_build(&prepared).expect("complete build");
            assert_eq!(published.artifacts().len(), 3);
            let bytes = inventory(published.path());
            assert_eq!(bytes.len(), 4, "three artifacts and one manifest");
            if let Some(expected) = &first {
                assert_eq!(&bytes, expected, "{fixture} replay");
            } else {
                first = Some(bytes);
            }
            fs::remove_dir_all(published.path()).expect("remove test-owned bundle");
            assert_no_artifacts(workspace.root());
        }
    }
    assert_eq!(visited.len(), 13);
}

#[cfg(not(all(target_os = "linux", target_arch = "x86_64")))]
#[test]
fn candidate_native_run_is_rejected_on_unsupported_hosts_without_artifacts() {
    let _guard = route_guard();
    let workspace = fixture_workspace();
    let failure = prepare_data_ownership_for_test(
        &request(workspace.root(), TargetSelection::Native),
        Some(("score".to_owned(), vec![ScalarValue::I32(21)])),
    )
    .expect_err("native execution is Linux x86-64 only");
    assert_eq!(failure.kind(), CommandFailureKind::Preparation);
    assert_eq!(failure.diagnostics()[0].code(), "ZRYNA-N4002");
    assert_no_artifacts(workspace.root());
}

#[test]
fn fixed_runtime_bounds_fail_without_partial_publication() {
    let _guard = route_guard();
    let registry = registry();
    for case in registry["runtimeInvalid"].as_array().expect("runtime invalid cases") {
        for (target, name) in [
            (TargetSelection::JavaScript, "javascript"),
            (TargetSelection::WebAssembly, "webassembly"),
            (TargetSelection::Native, "native"),
        ] {
            if name == "native" && !cfg!(all(target_os = "linux", target_arch = "x86_64")) {
                continue;
            }
            let workspace = fixture_workspace();
            install(workspace.root(), &registry, case["fixture"].as_str().expect("fixture"));
            let prepared = prepare_data_ownership_for_test(
                &request(workspace.root(), target),
                Some((case["export"].as_str().expect("export").to_owned(), arguments(case))),
            )
            .expect("bounds case is source-valid");
            let bundle =
                execute_and_publish_run(&prepared).expect("complete typed trap observation");
            assert_eq!(case["phase"], "execution");
            for result in bundle.results() {
                assert_eq!(
                    serde_json::to_value(result.outcome()).expect("fixed test authority"),
                    serde_json::json!({"kind":"trapped", "code":case["expectedTrap"]}),
                    "{name}"
                );
            }
            fs::remove_dir_all(bundle.path()).expect("remove complete observation");
            assert_no_artifacts(workspace.root());
        }
    }
}

#[test]
fn candidate_artifact_stem_exact_limit_and_first_extra_are_atomic() {
    let _guard = route_guard();
    let workspace = fixture_workspace();
    let mut build = request(workspace.root(), TargetSelection::All);
    build.artifact_stem = "a".repeat(crate::MAX_ARTIFACT_STEM_BYTES);
    let prepared = prepare_data_ownership_for_test(&build, None).expect("exact stem");
    let bundle = publish_data_ownership_build(&prepared).expect("exact stem publishes");
    let before = inventory(bundle.path());
    build.artifact_stem.push('a');
    let failure = prepare_data_ownership_for_test(&build, None).expect_err("first extra stem");
    assert_eq!(failure.kind(), CommandFailureKind::Request);
    assert_eq!(failure.diagnostics()[0].code(), "ZRYNA-D2001");
    assert_eq!(inventory(bundle.path()), before);
    assert_eq!(fs::read_dir(workspace.root().join(".zryna/out")).expect("inventory").count(), 1);
    fs::remove_dir_all(bundle.path()).expect("remove exact-boundary bundle");
    assert_no_artifacts(workspace.root());
}

#[test]
fn fixed_bounds_trap_retains_language_identity_across_targets() {
    let _guard = route_guard();
    let registry = registry();
    let case = &registry["runtimeInvalid"][0];
    let mut missing = Vec::new();
    for (target, name) in [
        (TargetSelection::JavaScript, "javascript"),
        (TargetSelection::WebAssembly, "webassembly"),
        (TargetSelection::Native, "native"),
    ] {
        if name == "native" && !cfg!(all(target_os = "linux", target_arch = "x86_64")) {
            continue;
        }
        let workspace = fixture_workspace();
        install(workspace.root(), &registry, case["fixture"].as_str().expect("fixture"));
        let prepared = prepare_data_ownership_for_test(
            &request(workspace.root(), target),
            Some((case["export"].as_str().expect("export").to_owned(), arguments(case))),
        )
        .expect("source-valid bounds failure");
        match execute_and_publish_run(&prepared) {
            Ok(bundle) => {
                let results = serde_json::to_value(bundle.results()).expect("typed observations");
                for result in results.as_array().expect("results") {
                    if result["outcome"]["kind"] != "trapped"
                        || result["outcome"]["code"] != case["expectedTrap"]
                    {
                        missing.push(format!("{name}: {result}"));
                    }
                }
                fs::remove_dir_all(bundle.path()).expect("test bundle cleanup");
            }
            Err(error) => missing.push(format!(
                "{name}: {:?} {}",
                error.kind(),
                error.diagnostics()[0].code()
            )),
        }
        assert_no_artifacts(workspace.root());
    }
    assert!(
        missing.is_empty(),
        "required {} is missing; host/process failure is not a language trap: {missing:?}",
        case["expectedTrap"]
    );
}
